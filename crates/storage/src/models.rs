use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use chrono::DateTime;
use cli_master_core::{
    AgentDefinition, AgentId, AgentSource, CommandSpec, ProjectId, SessionId, SessionStatus,
    WorktreeId,
};

pub use cli_master_core::WorktreeState;

use crate::StorageError;
use crate::error::{corrupt_data, invalid_input};
use crate::paths::validate_absolute_path;

pub(crate) const MAX_DISPLAY_NAME_BYTES: usize = 256;
pub(crate) const MAX_COMMAND_JSON_BYTES: usize = 1024 * 1024;

/// Persisted launch metadata for a built-in or custom coding agent.
#[derive(Clone, Eq, PartialEq)]
pub struct StoredAgent {
    /// Stable identifier referenced by sessions.
    pub id: AgentId,
    /// Whether the definition ships with the app or was created by the user.
    pub source: AgentSource,
    /// User-facing name.
    pub display_name: String,
    /// Executable name or absolute path, passed directly to the process API.
    pub executable: String,
    /// Ordered process arguments, never flattened into a shell command.
    pub args: Vec<String>,
    /// Non-secret environment overrides; secret-bearing key names are rejected.
    /// Values must never be logged.
    pub env: BTreeMap<String, String>,
    /// Whether the definition can be selected for a new session.
    pub enabled: bool,
    /// Creation time as Unix epoch milliseconds.
    pub created_at_ms: i64,
    /// Last metadata update as Unix epoch milliseconds.
    pub updated_at_ms: i64,
}

impl fmt::Debug for StoredAgent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoredAgent")
            .field("id", &self.id)
            .field("source", &self.source)
            .field("display_name", &self.display_name)
            .field("executable", &self.executable)
            .field("args_count", &self.args.len())
            .field("env_keys", &self.env.keys().collect::<Vec<_>>())
            .field("enabled", &self.enabled)
            .field("created_at_ms", &self.created_at_ms)
            .field("updated_at_ms", &self.updated_at_ms)
            .finish()
    }
}

impl StoredAgent {
    /// Builds the validated launch command for a concrete session directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the working directory is not an absolute NUL-free
    /// path or stored command metadata violates current validation rules.
    pub fn command_for_cwd(&self, cwd: impl Into<PathBuf>) -> Result<CommandSpec, StorageError> {
        let cwd = cwd.into();
        validate_absolute_path(&cwd, "agent launch cwd")?;
        CommandSpec::try_from_parts(
            self.executable.clone(),
            self.args.clone(),
            cwd,
            self.env.clone(),
        )
        .map_err(|error| invalid_input("agent launch command", error.to_string()))
    }

    /// Builds the command-free core catalog DTO.
    #[must_use]
    pub fn definition(&self) -> AgentDefinition {
        AgentDefinition {
            id: self.id.clone(),
            display_name: self.display_name.clone(),
            description: None,
            source: self.source,
            enabled: self.enabled,
        }
    }

    pub(crate) fn validate(&self) -> Result<(), StorageError> {
        validate_display_name("agent display name", &self.display_name)?;
        if self.executable.trim().is_empty() {
            return Err(invalid_input("agent executable", "must not be blank"));
        }
        validate_non_secret_environment(&self.env)?;
        validate_timestamp("agent created_at_ms", self.created_at_ms)?;
        validate_timestamp("agent updated_at_ms", self.updated_at_ms)?;
        CommandSpec::try_from_parts(
            self.executable.clone(),
            self.args.clone(),
            Path::new("."),
            self.env.clone(),
        )
        .map_err(|error| invalid_input("agent command", error.to_string()))?;
        Ok(())
    }
}

/// Complete session row, including daemon ownership and recovery metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredSession {
    /// Stable session identifier.
    pub id: SessionId,
    /// Owning project identifier.
    pub project_id: ProjectId,
    /// Agent definition used to launch the process.
    pub agent_id: AgentId,
    /// User-facing session name.
    pub name: String,
    /// Absolute process working directory; storage does not canonicalize symlinks.
    pub cwd: PathBuf,
    /// Last known lifecycle status.
    pub status: SessionStatus,
    /// Last known child process identifier.
    pub runtime_pid: Option<u32>,
    /// Daemon instance that owns the live PTY, when applicable.
    pub daemon_instance_id: Option<String>,
    /// Process exit code after termination.
    pub exit_code: Option<i32>,
    /// Stable machine-readable failure code, when applicable.
    pub error_code: Option<String>,
    /// Creation time as Unix epoch milliseconds.
    pub created_at_ms: i64,
    /// Last metadata update as Unix epoch milliseconds.
    pub updated_at_ms: i64,
    /// Most recent terminal activity as Unix epoch milliseconds.
    pub last_activity_at_ms: Option<i64>,
}

impl StoredSession {
    pub(crate) fn validate(&self) -> Result<(), StorageError> {
        validate_display_name("session name", &self.name)?;
        validate_absolute_path(&self.cwd, "session cwd")?;
        validate_timestamp("session created_at_ms", self.created_at_ms)?;
        validate_timestamp("session updated_at_ms", self.updated_at_ms)?;
        validate_optional_timestamp("session last_activity_at_ms", self.last_activity_at_ms)?;
        validate_session_runtime(
            self.status,
            self.runtime_pid,
            self.daemon_instance_id.as_deref(),
            self.exit_code,
            self.error_code.as_deref(),
        )
    }
}

/// Atomic replacement for a session's daemon-owned runtime metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionRuntimeUpdate {
    /// New lifecycle status.
    pub status: SessionStatus,
    /// Current child process identifier.
    pub runtime_pid: Option<u32>,
    /// Daemon instance that owns the live PTY.
    pub daemon_instance_id: Option<String>,
    /// Process exit code after termination.
    pub exit_code: Option<i32>,
    /// Stable machine-readable failure code.
    pub error_code: Option<String>,
    /// Last terminal activity as Unix epoch milliseconds.
    pub last_activity_at_ms: Option<i64>,
    /// Metadata update time as Unix epoch milliseconds.
    pub updated_at_ms: i64,
}

impl SessionRuntimeUpdate {
    pub(crate) fn validate(&self) -> Result<(), StorageError> {
        validate_timestamp("session updated_at_ms", self.updated_at_ms)?;
        validate_optional_timestamp("session last_activity_at_ms", self.last_activity_at_ms)?;
        validate_session_runtime(
            self.status,
            self.runtime_pid,
            self.daemon_instance_id.as_deref(),
            self.exit_code,
            self.error_code.as_deref(),
        )
    }
}

/// Complete metadata for one managed Git worktree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredWorktree {
    /// Stable worktree identifier.
    pub id: WorktreeId,
    /// Project repository that owns the worktree.
    pub project_id: ProjectId,
    /// Session currently associated with the worktree.
    pub session_id: Option<SessionId>,
    /// Absolute worktree root path; storage does not canonicalize symlinks.
    pub path: PathBuf,
    /// Checked-out branch name.
    pub branch: String,
    /// Persisted lifecycle state.
    pub state: WorktreeState,
    /// Dirty flag from the latest Git status inspection.
    pub is_dirty: bool,
    /// Creation time as Unix epoch milliseconds.
    pub created_at_ms: i64,
    /// Last metadata update as Unix epoch milliseconds.
    pub updated_at_ms: i64,
}

impl StoredWorktree {
    pub(crate) fn validate(&self) -> Result<(), StorageError> {
        validate_absolute_path(&self.path, "worktree path")?;
        if self.branch.trim().is_empty() {
            return Err(invalid_input("worktree branch", "must not be blank"));
        }
        if self.branch.len() > 1024 {
            return Err(invalid_input(
                "worktree branch",
                "must be at most 1024 bytes",
            ));
        }
        validate_timestamp("worktree created_at_ms", self.created_at_ms)?;
        validate_timestamp("worktree updated_at_ms", self.updated_at_ms)
    }
}

pub(crate) const fn agent_source_to_database(source: AgentSource) -> &'static str {
    match source {
        AgentSource::BuiltIn => "built_in",
        AgentSource::Custom => "custom",
    }
}

pub(crate) fn agent_source_from_database(value: &str) -> Result<AgentSource, StorageError> {
    match value {
        "built_in" => Ok(AgentSource::BuiltIn),
        "custom" => Ok(AgentSource::Custom),
        _ => Err(corrupt_data(
            "agent",
            "source",
            format!("unsupported value {value:?}"),
        )),
    }
}

pub(crate) const fn worktree_state_to_database(state: WorktreeState) -> &'static str {
    match state {
        WorktreeState::Creating => "creating",
        WorktreeState::Active => "active",
        WorktreeState::RemovePending => "remove_pending",
        WorktreeState::Orphaned => "orphaned",
    }
}

pub(crate) fn worktree_state_from_database(value: &str) -> Result<WorktreeState, StorageError> {
    match value {
        "creating" => Ok(WorktreeState::Creating),
        "active" => Ok(WorktreeState::Active),
        "remove_pending" => Ok(WorktreeState::RemovePending),
        "orphaned" => Ok(WorktreeState::Orphaned),
        _ => Err(corrupt_data(
            "worktree",
            "state",
            format!("unsupported value {value:?}"),
        )),
    }
}

pub(crate) const fn session_status_to_database(status: SessionStatus) -> &'static str {
    match status {
        SessionStatus::Created => "created",
        SessionStatus::Starting => "starting",
        SessionStatus::Running => "running",
        SessionStatus::Idle => "idle",
        SessionStatus::Stopping => "stopping",
        SessionStatus::Exited => "exited",
        SessionStatus::Failed => "failed",
        SessionStatus::Unknown => "unknown",
    }
}

pub(crate) fn session_status_from_database(value: &str) -> Result<SessionStatus, StorageError> {
    match value {
        "created" => Ok(SessionStatus::Created),
        "starting" => Ok(SessionStatus::Starting),
        "running" => Ok(SessionStatus::Running),
        "idle" => Ok(SessionStatus::Idle),
        "stopping" => Ok(SessionStatus::Stopping),
        "exited" => Ok(SessionStatus::Exited),
        "failed" => Ok(SessionStatus::Failed),
        "unknown" => Ok(SessionStatus::Unknown),
        _ => Err(corrupt_data(
            "session",
            "status",
            format!("unsupported value {value:?}"),
        )),
    }
}

pub(crate) fn validate_display_name(field: &'static str, name: &str) -> Result<(), StorageError> {
    if name.trim().is_empty() {
        return Err(invalid_input(field, "must not be blank"));
    }
    if name.len() > MAX_DISPLAY_NAME_BYTES {
        return Err(invalid_input(
            field,
            format!("must be at most {MAX_DISPLAY_NAME_BYTES} bytes"),
        ));
    }
    Ok(())
}

pub(crate) fn validate_timestamp(field: &'static str, value: i64) -> Result<(), StorageError> {
    if value < 0 {
        return Err(invalid_input(
            field,
            "must be a non-negative Unix epoch millisecond value",
        ));
    }
    if DateTime::<chrono::Utc>::from_timestamp_millis(value).is_none() {
        return Err(invalid_input(
            field,
            "must fit in the RFC 3339 timestamp range",
        ));
    }
    Ok(())
}

pub(crate) fn validate_rfc3339_timestamp(
    field: &'static str,
    value: &str,
) -> Result<(), StorageError> {
    let parsed = DateTime::parse_from_rfc3339(value)
        .map_err(|_| invalid_input(field, "must be an RFC 3339 timestamp"))?;
    if parsed.offset().local_minus_utc() != 0 {
        return Err(invalid_input(field, "must use a UTC offset"));
    }
    Ok(())
}

fn validate_optional_timestamp(
    field: &'static str,
    value: Option<i64>,
) -> Result<(), StorageError> {
    value.map_or(Ok(()), |timestamp| validate_timestamp(field, timestamp))
}

fn validate_session_runtime(
    status: SessionStatus,
    runtime_pid: Option<u32>,
    daemon_instance_id: Option<&str>,
    exit_code: Option<i32>,
    error_code: Option<&str>,
) -> Result<(), StorageError> {
    if let Some(daemon_instance_id) = daemon_instance_id {
        validate_daemon_instance_id(daemon_instance_id)?;
    }
    if runtime_pid == Some(0) {
        return Err(invalid_input(
            "session runtime_pid",
            "must be greater than zero",
        ));
    }
    if error_code.is_some_and(|code| code.trim().is_empty()) {
        return Err(invalid_input("session error code", "must not be blank"));
    }
    if error_code.is_some_and(|code| code.len() > 256 || code.contains('\0')) {
        return Err(invalid_input(
            "session error code",
            "must be at most 256 bytes and contain no NUL byte",
        ));
    }

    match status {
        SessionStatus::Created => {
            if runtime_pid.is_some() || daemon_instance_id.is_some() {
                return Err(invalid_input(
                    "session runtime metadata",
                    "must be empty before a session starts",
                ));
            }
            require_no_terminal_result(exit_code, error_code)
        }
        SessionStatus::Starting => {
            require_daemon_instance(daemon_instance_id)?;
            require_no_terminal_result(exit_code, error_code)
        }
        SessionStatus::Running | SessionStatus::Idle | SessionStatus::Stopping => {
            require_daemon_instance(daemon_instance_id)?;
            if runtime_pid.is_none() {
                return Err(invalid_input(
                    "session runtime_pid",
                    "is required while a session is running or idle",
                ));
            }
            require_no_terminal_result(exit_code, error_code)
        }
        SessionStatus::Exited | SessionStatus::Failed | SessionStatus::Unknown => {
            if runtime_pid.is_some() {
                return Err(invalid_input(
                    "session runtime_pid",
                    "must be empty for a terminal or unknown session",
                ));
            }
            Ok(())
        }
    }
}

pub(crate) fn validate_daemon_instance_id(id: &str) -> Result<(), StorageError> {
    if id.trim().is_empty() {
        return Err(invalid_input("daemon instance id", "must not be blank"));
    }
    if id.len() > 128 || id.contains('\0') {
        return Err(invalid_input(
            "daemon instance id",
            "must be at most 128 bytes and contain no NUL byte",
        ));
    }
    Ok(())
}

fn validate_non_secret_environment(
    environment: &BTreeMap<String, String>,
) -> Result<(), StorageError> {
    if environment
        .keys()
        .any(|key| is_sensitive_environment_key(key))
    {
        return Err(invalid_input(
            "agent environment",
            "secret-bearing variables are not persisted; use inherited process environment or a secret manager",
        ));
    }
    Ok(())
}

fn is_sensitive_environment_key(key: &str) -> bool {
    const SENSITIVE_SEGMENTS: &[&str] = &[
        "TOKEN",
        "SECRET",
        "PASSWORD",
        "PASSWD",
        "CREDENTIAL",
        "CREDENTIALS",
    ];
    const SENSITIVE_COMPOUNDS: &[&str] = &[
        "APIKEY",
        "PRIVATEKEY",
        "ACCESSKEY",
        "CLIENTSECRET",
        "AUTHTOKEN",
        "ACCESSTOKEN",
        "REFRESHTOKEN",
    ];

    let compact = key
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|character| character.to_ascii_uppercase())
        .collect::<String>();
    let segments = key
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|segment| !segment.is_empty())
        .map(str::to_ascii_uppercase)
        .collect::<Vec<_>>();
    segments.iter().any(|segment| {
        SENSITIVE_SEGMENTS.contains(&segment.as_str())
            || SENSITIVE_COMPOUNDS.contains(&segment.as_str())
    }) || SENSITIVE_SEGMENTS
        .iter()
        .chain(SENSITIVE_COMPOUNDS)
        .any(|suffix| compact.ends_with(suffix))
        || segments.windows(2).any(|pair| {
            matches!(
                (pair[0].as_str(), pair[1].as_str()),
                ("API" | "PRIVATE" | "ACCESS", "KEY")
                    | ("CLIENT", "SECRET")
                    | ("AUTH" | "ACCESS" | "REFRESH", "TOKEN")
            )
        })
}

fn require_daemon_instance(daemon_instance_id: Option<&str>) -> Result<(), StorageError> {
    if daemon_instance_id.is_none() {
        Err(invalid_input(
            "daemon instance id",
            "is required for a live session",
        ))
    } else {
        Ok(())
    }
}

fn require_no_terminal_result(
    exit_code: Option<i32>,
    error_code: Option<&str>,
) -> Result<(), StorageError> {
    if exit_code.is_some() || error_code.is_some() {
        Err(invalid_input(
            "session terminal result",
            "must be empty while a session is live",
        ))
    } else {
        Ok(())
    }
}
