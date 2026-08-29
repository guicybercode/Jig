use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use cli_master_core::{
    ApplicationError, LOG_FILE_MAX_BYTES, LOG_FILE_RETENTION, LogRecordDto, RECENT_ERRORS_MAX,
    RECENT_LOGS_MAX, redact_text,
};
use serde::Serialize;

/// Severity of a structured log record.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    /// Fine-grained tracing.
    Trace,
    /// Debug details that never include secrets.
    Debug,
    /// Normal operational events.
    Info,
    /// Recoverable problems.
    Warn,
    /// Failures the user should see.
    Error,
}

impl LogLevel {
    /// Returns the lowercase wire value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

/// One structured log event. Messages are redacted at construction time.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StructuredLog {
    /// RFC 3339 timestamp.
    pub timestamp: String,
    /// Severity.
    pub level: LogLevel,
    /// Component name.
    pub target: String,
    /// Operation name.
    pub operation: String,
    /// Session identifier when relevant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Project identifier when relevant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// Stable error code when recording a failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// Already-redacted message. Never includes full env, prompts, or PTY output.
    pub message: String,
}

impl StructuredLog {
    /// Creates a redacted log record.
    #[must_use]
    pub fn new(
        level: LogLevel,
        target: impl Into<String>,
        operation: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            timestamp: rfc3339_now(),
            level,
            target: target.into(),
            operation: operation.into(),
            session_id: None,
            project_id: None,
            error_code: None,
            message: redact_text(&message.into()),
        }
    }

    /// Attaches a session identifier.
    #[must_use]
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Attaches a project identifier.
    #[must_use]
    pub fn with_project(mut self, project_id: impl Into<String>) -> Self {
        self.project_id = Some(project_id.into());
        self
    }

    /// Attaches a stable error code.
    #[must_use]
    pub fn with_error_code(mut self, error_code: impl Into<String>) -> Self {
        self.error_code = Some(error_code.into());
        self
    }

    /// Records an [`ApplicationError`] without its source chain on the wire
    /// message; the chain is appended only to the local log line.
    #[must_use]
    pub fn from_error(
        target: impl Into<String>,
        operation: impl Into<String>,
        error: &ApplicationError,
    ) -> Self {
        let mut message = error.technical_message().to_owned();
        if let Some(chain) = error.source_chain() {
            message = format!("{message} | source={chain}");
        }
        Self::new(LogLevel::Error, target, operation, message)
            .with_error_code(error.code().as_str())
    }

    /// Converts the record into a diagnostics DTO.
    #[must_use]
    pub fn to_dto(&self) -> LogRecordDto {
        LogRecordDto {
            timestamp: self.timestamp.clone(),
            level: self.level.as_str().to_owned(),
            target: self.target.clone(),
            operation: self.operation.clone(),
            session_id: self.session_id.clone(),
            project_id: self.project_id.clone(),
            error_code: self.error_code.clone(),
            message: self.message.clone(),
        }
    }
}

/// Process-wide structured logger with a bounded diagnostics ring buffer.
pub struct Logger {
    recent: Mutex<VecDeque<StructuredLog>>,
    recent_errors: Mutex<VecDeque<ApplicationError>>,
    file_path: Mutex<Option<PathBuf>>,
}

impl Logger {
    /// Returns the process logger.
    #[must_use]
    pub fn global() -> &'static Self {
        static LOGGER: OnceLock<Logger> = OnceLock::new();
        LOGGER.get_or_init(|| Self {
            recent: Mutex::new(VecDeque::new()),
            recent_errors: Mutex::new(VecDeque::new()),
            file_path: Mutex::new(None),
        })
    }

    /// Directs file output at `path` with user-only permissions.
    pub fn set_file_path(&self, path: PathBuf) {
        if let Ok(mut guard) = self.file_path.lock() {
            *guard = Some(path);
        }
    }

    /// Writes a structured record to the ring buffer and optional log file.
    pub fn write(&self, record: &StructuredLog) {
        if let Ok(mut recent) = self.recent.lock() {
            recent.push_back(record.clone());
            while recent.len() > RECENT_LOGS_MAX {
                recent.pop_front();
            }
        }
        write_line(self, record);
    }

    /// Records an error for diagnostics and logs its technical details.
    pub fn error(&self, target: &str, operation: &str, error: &ApplicationError) {
        if let Ok(mut recent) = self.recent_errors.lock() {
            recent.push_back(error.clone());
            while recent.len() > RECENT_ERRORS_MAX {
                recent.pop_front();
            }
        }
        self.write(&StructuredLog::from_error(target, operation, error));
    }

    /// Returns a copy of recent sanitized logs.
    #[must_use]
    pub fn recent_logs(&self) -> Vec<LogRecordDto> {
        self.recent
            .lock()
            .map(|records| records.iter().map(StructuredLog::to_dto).collect())
            .unwrap_or_default()
    }

    /// Returns recent IPC-safe errors.
    #[must_use]
    pub fn recent_errors(&self) -> Vec<cli_master_core::ApiError> {
        self.recent_errors
            .lock()
            .map(|errors| errors.iter().map(ApplicationError::to_api_error).collect())
            .unwrap_or_default()
    }
}

fn write_line(logger: &Logger, record: &StructuredLog) {
    let Ok(json) = serde_json::to_string(record) else {
        return;
    };
    let Some(path) = logger.file_path.lock().ok().and_then(|guard| guard.clone()) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
        let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
    }
    rotate_if_needed(&path);
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(&path)
    {
        let _ = writeln!(file, "{json}");
    }
}

fn rotate_if_needed(path: &Path) {
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    if metadata.len() < LOG_FILE_MAX_BYTES as u64 {
        return;
    }
    for index in (1..LOG_FILE_RETENTION).rev() {
        let from = path.with_extension(format!("log.{index}"));
        let to = path.with_extension(format!("log.{}", index + 1));
        let _ = fs::rename(from, to);
    }
    let rotated = path.with_extension("log.1");
    let _ = fs::rename(path, rotated);
}

fn rfc3339_now() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = now.as_secs();
    let millis = now.subsec_millis();
    let days = seconds / 86_400;
    let tod = seconds % 86_400;
    let hour = tod / 3600;
    let minute = (tod % 3600) / 60;
    let second = tod % 60;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

fn civil_from_days(days: u64) -> (i32, u32, u32) {
    let z = i64::try_from(days).unwrap_or(0) + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = u64::try_from(z - era * 146_097).unwrap_or(0);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = i32::try_from(yoe).unwrap_or(0) + i32::try_from(era).unwrap_or(0) * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    let year = year + i32::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use cli_master_core::{ApplicationError, ErrorCode};

    use super::*;

    #[test]
    fn logs_redact_secret_assignments_and_keep_error_codes() {
        let record = StructuredLog::new(
            LogLevel::Info,
            "process",
            "agent.launch",
            "starting with TOKEN=abc123",
        )
        .with_session("session-1")
        .with_project("project-1")
        .with_error_code("PTY_SPAWN_FAILED");
        let json = serde_json::to_string(&record).expect("json");
        assert!(json.contains("TOKEN=[redacted]"));
        assert!(!json.contains("abc123"));
        assert!(json.contains("session-1"));
        assert!(json.contains("PTY_SPAWN_FAILED"));
        assert!(json.contains("\"timestamp\""));
        assert!(json.contains("\"level\""));
        assert!(json.contains("\"target\""));
        assert!(json.contains("\"operation\""));
    }

    #[test]
    fn error_source_chain_stays_in_logs_not_api_errors() {
        let io = std::io::Error::other("COOKIE=secret-cookie");
        let error = ApplicationError::new(ErrorCode::DatabaseUnavailable, "Database is locked.")
            .with_action("Close other CLI Master instances and retry.")
            .with_source(&io);
        let record = StructuredLog::from_error("storage", "storage.open", &error);
        assert!(record.message.contains("[redacted]"));
        assert!(!record.message.contains("secret-cookie"));
        let api = error.to_api_error();
        let json = serde_json::to_string(&api).expect("api");
        assert!(!json.contains("source"));
        assert!(!json.contains("secret-cookie"));
    }

    #[test]
    fn ring_buffer_returns_sanitized_dtos() {
        let logger = Logger::global();
        logger.write(&StructuredLog::new(
            LogLevel::Warn,
            "git",
            "git.status",
            "PASSWORD=hidden",
        ));
        let logs = logger.recent_logs();
        assert!(logs.iter().any(|record| {
            record.message.contains("[redacted]") && !record.message.contains("hidden")
        }));
    }
}
