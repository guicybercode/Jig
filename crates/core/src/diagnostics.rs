use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ApiError, redact_json_value};

/// Sanitized snapshot that can be copied from the diagnostics screen.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsReport {
    /// Application version string.
    pub app_version: String,
    /// Operating system family, such as `linux` or `macos`.
    pub os: String,
    /// CPU architecture reported by the current build.
    pub arch: String,
    /// User data directory.
    pub data_dir: PathBuf,
    /// User config directory.
    pub config_dir: PathBuf,
    /// Runtime directory used for the daemon socket.
    pub runtime_dir: PathBuf,
    /// `SQLite` database path.
    pub database_path: PathBuf,
    /// Log directory.
    pub log_dir: PathBuf,
    /// `git --version` output, when Git is available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_version: Option<String>,
    /// Whether Git was found.
    pub git_available: bool,
    /// Daemon connectivity.
    pub daemon: DaemonDiagnostics,
    /// `SQLite` reachability without dumping schema contents.
    pub sqlite: SqliteDiagnostics,
    /// Detected agent executables.
    pub agents: Vec<AgentDiagnostics>,
    /// Resolved helper executables such as `git`.
    pub executables: Vec<ExecutableDiagnostics>,
    /// Number of persisted sessions.
    pub session_count: u64,
    /// Number of persisted worktrees.
    pub worktree_count: u64,
    /// Recent sanitized log records.
    pub recent_logs: Vec<LogRecordDto>,
    /// Recent user-safe errors.
    pub recent_errors: Vec<ApiError>,
}

impl DiagnosticsReport {
    /// Renders a copy-paste diagnostics bundle. It never includes environment
    /// maps, tokens, or terminal contents.
    #[must_use]
    pub fn to_export_text(&self) -> String {
        serde_json::to_value(self)
            .map(|report| {
                let mut report = redact_json_value("diagnostics", report);
                redact_home_paths(&mut report);
                report
            })
            .and_then(|report| serde_json::to_string_pretty(&report))
            .unwrap_or_else(|_| {
            format!(
                "{{\n  \"appVersion\": {},\n  \"error\": \"diagnostics serialization failed\"\n}}",
                json_string(&self.app_version)
            )
        })
    }
}

/// Daemon reachability without exposing socket credentials.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonDiagnostics {
    /// Whether a daemon handshake succeeded.
    pub connected: bool,
    /// Daemon instance identifier when connected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    /// Safe status explanation.
    pub status: String,
}

/// `SQLite` file presence and schema version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SqliteDiagnostics {
    /// Whether the database file exists.
    pub file_exists: bool,
    /// Whether the file could be opened.
    pub available: bool,
    /// Current schema version when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<u32>,
    /// Safe status explanation.
    pub status: String,
}

/// Detection result for one agent adapter.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDiagnostics {
    /// Stable adapter key.
    pub key: String,
    /// User-facing name.
    pub display_name: String,
    /// Whether an executable was resolved.
    pub detected: bool,
    /// Resolved executable path when found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable: Option<PathBuf>,
}

/// A helper executable resolved for diagnostics.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutableDiagnostics {
    /// Bare name such as `git`.
    pub name: String,
    /// Absolute path when resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
}

/// One sanitized structured log record.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogRecordDto {
    /// RFC 3339 timestamp.
    pub timestamp: String,
    /// `trace`, `debug`, `info`, `warn`, or `error`.
    pub level: String,
    /// Code component, such as `git` or `session`.
    pub target: String,
    /// Operation name, such as `worktree.remove`.
    pub operation: String,
    /// Session identifier when the event is session-scoped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Project identifier when the event is project-scoped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// Stable error code when the event records a failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// Already-redacted message.
    pub message: String,
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"unknown\"".to_owned())
}

fn redact_home_paths(value: &mut Value) {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return;
    };
    if home == Path::new("/") || home.as_os_str().is_empty() {
        return;
    }
    let Some(home) = home.to_str() else {
        return;
    };
    redact_string_values(value, home);
}

fn redact_string_values(value: &mut Value, home: &str) {
    match value {
        Value::String(text) => {
            if text.contains(home) {
                *text = text.replace(home, "~");
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_string_values(item, home);
            }
        }
        Value::Object(map) => {
            for item in map.values_mut() {
                redact_string_values(item, home);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_text_is_pretty_json_without_environment() {
        let report = DiagnosticsReport {
            app_version: "0.1.0".to_owned(),
            os: "linux".to_owned(),
            arch: "x86_64".to_owned(),
            data_dir: PathBuf::from("/tmp/data"),
            config_dir: PathBuf::from("/tmp/config"),
            runtime_dir: PathBuf::from("/tmp/runtime"),
            database_path: PathBuf::from("/tmp/data/cli-master.db"),
            log_dir: PathBuf::from("/tmp/logs"),
            git_version: Some("2.43.0".to_owned()),
            git_available: true,
            daemon: DaemonDiagnostics {
                connected: false,
                instance_id: None,
                status: "No daemon is running.".to_owned(),
            },
            sqlite: SqliteDiagnostics {
                file_exists: false,
                available: false,
                schema_version: None,
                status: "Database file has not been created yet.".to_owned(),
            },
            agents: Vec::new(),
            executables: Vec::new(),
            session_count: 0,
            worktree_count: 0,
            recent_logs: Vec::new(),
            recent_errors: Vec::new(),
        };

        let export = report.to_export_text();
        assert!(export.contains("\"appVersion\": \"0.1.0\""));
        assert!(!export.to_ascii_uppercase().contains("TOKEN"));
        assert!(!export.contains("environ"));
    }

    #[test]
    fn export_replaces_the_user_home_prefix() {
        let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
            return;
        };
        if home == Path::new("/") || home.as_os_str().is_empty() {
            return;
        }
        let report = DiagnosticsReport {
            app_version: "0.1.0".to_owned(),
            os: "linux".to_owned(),
            arch: "x86_64".to_owned(),
            data_dir: home.join("private/data"),
            config_dir: home.join("private/config"),
            runtime_dir: home.join("private/runtime"),
            database_path: home.join("private/data/cli-master.db"),
            log_dir: home.join("private/logs"),
            git_version: None,
            git_available: false,
            daemon: DaemonDiagnostics {
                connected: false,
                instance_id: None,
                status: "offline".to_owned(),
            },
            sqlite: SqliteDiagnostics {
                file_exists: false,
                available: false,
                schema_version: None,
                status: "missing".to_owned(),
            },
            agents: Vec::new(),
            executables: Vec::new(),
            session_count: 0,
            worktree_count: 0,
            recent_logs: Vec::new(),
            recent_errors: Vec::new(),
        };
        let export = report.to_export_text();
        assert!(!export.contains(&home.to_string_lossy().into_owned()));
        assert!(export.contains("~/private/data"));
    }

    #[test]
    fn export_redacts_untrusted_strings_from_every_report_field() {
        let report = DiagnosticsReport {
            app_version: "0.1.0".to_owned(),
            os: "linux".to_owned(),
            arch: "x86_64".to_owned(),
            data_dir: PathBuf::from("/tmp/data"),
            config_dir: PathBuf::from("/tmp/config"),
            runtime_dir: PathBuf::from("/tmp/runtime"),
            database_path: PathBuf::from("/tmp/data/cli-master.db"),
            log_dir: PathBuf::from("/tmp/logs"),
            git_version: Some("2.43 TOKEN=git-version-secret".to_owned()),
            git_available: true,
            daemon: DaemonDiagnostics {
                connected: false,
                instance_id: None,
                status: "Authorization: Basic daemon-secret".to_owned(),
            },
            sqlite: SqliteDiagnostics {
                file_exists: false,
                available: false,
                schema_version: None,
                status: "missing".to_owned(),
            },
            agents: Vec::new(),
            executables: Vec::new(),
            session_count: 0,
            worktree_count: 0,
            recent_logs: Vec::new(),
            recent_errors: Vec::new(),
        };

        let export = report.to_export_text();
        assert!(!export.contains("git-version-secret"));
        assert!(!export.contains("daemon-secret"));
    }
}
