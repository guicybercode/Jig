use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::redact::{redact_json_value, redact_text};

/// Stable machine-readable error codes used across IPC, logs, and the UI.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum ErrorCode {
    /// No agent definition matched the requested identifier.
    AgentNotFound,
    /// Opening a PTY or spawning the agent process failed.
    PtySpawnFailed,
    /// No registered project matched the requested identifier.
    ProjectNotFound,
    /// The selected directory is not a Git repository.
    NotAGitRepository,
    /// A worktree has uncommitted, staged, or untracked changes.
    WorktreeDirty,
    /// A live session currently uses the worktree.
    WorktreeInUse,
    /// A Git subprocess failed, timed out, or returned a non-zero status.
    GitCommandFailed,
    /// SQLite could not be opened, migrated, or queried.
    DatabaseUnavailable,
    /// The desktop bridge is not connected to a live daemon.
    DaemonDisconnected,
    /// A supplied filesystem path is missing, escapes a root, or is malformed.
    InvalidPath,
    /// The operating system refused access to a path or process.
    PermissionDenied,
    /// A custom agent definition is missing or malformed.
    InvalidAgentDefinition,
    /// A process was asked to start with a disallowed shell invocation.
    ShellInvocationRefused,
    /// A command exceeded its timeout or output limit.
    CommandTimeout,
    /// A path is outside the application's managed directories.
    UnmanagedPath,
    /// Removal of `/`, the user home, or a project root was refused.
    CriticalPathRefused,
    /// The stored repository path no longer matches the Git toplevel.
    RepositoryMoved,
    /// A PID no longer matches the recorded process identity.
    ProcessIdentityMismatch,
    /// A destructive operation requires an explicit confirmation token.
    ConfirmationRequired,
    /// A confirmation token expired or does not match the current state.
    ConfirmationMismatch,
    /// An agent definition is still referenced by session history.
    AgentInUse,
    /// A session process is still live.
    SessionInUse,
    /// A project still has sessions or worktrees and cannot be unregistered.
    ProjectInUse,
    /// Incoming IPC failed schema or path validation.
    InvalidIpcPayload,
}

impl ErrorCode {
    /// Returns the stable wire value for this code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AgentNotFound => "AGENT_NOT_FOUND",
            Self::PtySpawnFailed => "PTY_SPAWN_FAILED",
            Self::ProjectNotFound => "PROJECT_NOT_FOUND",
            Self::NotAGitRepository => "NOT_A_GIT_REPOSITORY",
            Self::WorktreeDirty => "WORKTREE_DIRTY",
            Self::WorktreeInUse => "WORKTREE_IN_USE",
            Self::GitCommandFailed => "GIT_COMMAND_FAILED",
            Self::DatabaseUnavailable => "DATABASE_UNAVAILABLE",
            Self::DaemonDisconnected => "DAEMON_DISCONNECTED",
            Self::InvalidPath => "INVALID_PATH",
            Self::PermissionDenied => "PERMISSION_DENIED",
            Self::InvalidAgentDefinition => "INVALID_AGENT_DEFINITION",
            Self::ShellInvocationRefused => "SHELL_INVOCATION_REFUSED",
            Self::CommandTimeout => "COMMAND_TIMEOUT",
            Self::UnmanagedPath => "UNMANAGED_PATH",
            Self::CriticalPathRefused => "CRITICAL_PATH_REFUSED",
            Self::RepositoryMoved => "REPOSITORY_MOVED",
            Self::ProcessIdentityMismatch => "PROCESS_IDENTITY_MISMATCH",
            Self::ConfirmationRequired => "CONFIRMATION_REQUIRED",
            Self::ConfirmationMismatch => "CONFIRMATION_MISMATCH",
            Self::AgentInUse => "AGENT_IN_USE",
            Self::SessionInUse => "SESSION_IN_USE",
            Self::ProjectInUse => "PROJECT_IN_USE",
            Self::InvalidIpcPayload => "INVALID_IPC_PAYLOAD",
        }
    }

    /// Returns a short title suitable for dialogs and log lines.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::AgentNotFound => "Agent not found",
            Self::PtySpawnFailed => "Could not start the process",
            Self::ProjectNotFound => "Project not found",
            Self::NotAGitRepository => "Not a Git repository",
            Self::WorktreeDirty => "Worktree has uncommitted changes",
            Self::WorktreeInUse => "Worktree is in use",
            Self::GitCommandFailed => "Git command failed",
            Self::DatabaseUnavailable => "Database unavailable",
            Self::DaemonDisconnected => "Daemon disconnected",
            Self::InvalidPath => "Invalid path",
            Self::PermissionDenied => "Permission denied",
            Self::InvalidAgentDefinition => "Invalid agent definition",
            Self::ShellInvocationRefused => "Shell invocation refused",
            Self::CommandTimeout => "Command timed out",
            Self::UnmanagedPath => "Path is not managed",
            Self::CriticalPathRefused => "Critical path protected",
            Self::RepositoryMoved => "Repository moved",
            Self::ProcessIdentityMismatch => "Process identity mismatch",
            Self::ConfirmationRequired => "Confirmation required",
            Self::ConfirmationMismatch => "Confirmation is no longer valid",
            Self::AgentInUse => "Agent is in use",
            Self::SessionInUse => "Session is still running",
            Self::ProjectInUse => "Project is still in use",
            Self::InvalidIpcPayload => "Invalid request",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A stable, actionable error returned across the IPC boundary.
///
/// Internal causes and technical messages are not part of this type. Those
/// belong on [`ApplicationError`] and are written only to structured logs.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ApiError {
    /// Machine-readable error code suitable for branching and diagnostics.
    pub code: String,
    /// Concise human-readable explanation of the failure.
    pub message: String,
    /// Suggested action the user can take to resolve the failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// Structured diagnostic context. Secrets must never be inserted here.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, Value>,
    /// Short dialog title. Optional so existing clients can ignore it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Whether retrying or choosing another path can succeed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recoverable: Option<bool>,
}

impl ApiError {
    /// Creates an error without an action or diagnostic details.
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            action: None,
            details: BTreeMap::new(),
            title: None,
            recoverable: None,
        }
    }

    /// Adds a user-facing remediation action.
    #[must_use]
    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = Some(action.into());
        self
    }

    /// Adds one structured diagnostic detail.
    #[must_use]
    pub fn with_detail(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.details.insert(key.into(), value.into());
        self
    }

    /// Adds a short title without changing the user-facing message.
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Marks whether the caller can recover without losing data.
    #[must_use]
    pub const fn with_recoverable(mut self, recoverable: bool) -> Self {
        self.recoverable = Some(recoverable);
        self
    }
}

/// Internal application error with a safe IPC projection and a log-only chain.
///
/// `technical_message` and `source_chain` are never serialized on the wire.
/// The payload is boxed so `Result<T, ApplicationError>` stays small.
#[derive(Clone, Debug)]
pub struct ApplicationError {
    inner: Box<ApplicationErrorInner>,
}

#[derive(Clone, Debug)]
struct ApplicationErrorInner {
    code: ErrorCode,
    title: String,
    user_message: String,
    technical_message: String,
    recoverable: bool,
    suggested_action: Option<String>,
    context: BTreeMap<String, Value>,
    source_chain: Option<String>,
}

impl ApplicationError {
    /// Creates a recoverable error with a user-facing message.
    #[must_use]
    pub fn new(code: ErrorCode, user_message: impl Into<String>) -> Self {
        let user_message = user_message.into();
        Self {
            inner: Box::new(ApplicationErrorInner {
                title: code.title().to_owned(),
                technical_message: user_message.clone(),
                user_message,
                code,
                recoverable: true,
                suggested_action: None,
                context: BTreeMap::new(),
                source_chain: None,
            }),
        }
    }

    /// Marks the failure as not recoverable by retrying the same action.
    #[must_use]
    pub fn not_recoverable(mut self) -> Self {
        self.inner.recoverable = false;
        self
    }

    /// Replaces the short title shown in dialogs.
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.inner.title = title.into();
        self
    }

    /// Adds a user-facing remediation action.
    #[must_use]
    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.inner.suggested_action = Some(action.into());
        self
    }

    /// Replaces the log-only technical explanation.
    #[must_use]
    pub fn with_technical(mut self, message: impl Into<String>) -> Self {
        self.inner.technical_message = redact_text(&message.into());
        self
    }

    /// Adds one sanitized diagnostic detail. Sensitive keys are redacted.
    #[must_use]
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        let key = key.into();
        self.inner
            .context
            .insert(key.clone(), redact_json_value(&key, value.into()));
        self
    }

    /// Records a source chain for logs without exposing it over IPC.
    #[must_use]
    pub fn with_source(mut self, error: &dyn Error) -> Self {
        self.inner.source_chain = Some(format_error_chain(error));
        self
    }

    /// Returns the stable error code.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        self.inner.code
    }

    /// Returns the dialog title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.inner.title
    }

    /// Returns the user-facing message.
    #[must_use]
    pub fn user_message(&self) -> &str {
        &self.inner.user_message
    }

    /// Returns the log-only technical message.
    #[must_use]
    pub fn technical_message(&self) -> &str {
        &self.inner.technical_message
    }

    /// Returns whether the caller can recover without data loss.
    #[must_use]
    pub fn recoverable(&self) -> bool {
        self.inner.recoverable
    }

    /// Returns the suggested user action, when one exists.
    #[must_use]
    pub fn suggested_action(&self) -> Option<&str> {
        self.inner.suggested_action.as_deref()
    }

    /// Returns sanitized diagnostic context.
    #[must_use]
    pub fn context(&self) -> &BTreeMap<String, Value> {
        &self.inner.context
    }

    /// Returns the log-only source chain.
    #[must_use]
    pub fn source_chain(&self) -> Option<&str> {
        self.inner.source_chain.as_deref()
    }

    /// Projects this error onto the stable IPC type.
    #[must_use]
    pub fn to_api_error(&self) -> ApiError {
        ApiError {
            code: self.inner.code.as_str().to_owned(),
            message: self.inner.user_message.clone(),
            action: self.inner.suggested_action.clone(),
            details: self.inner.context.clone(),
            title: Some(self.inner.title.clone()),
            recoverable: Some(self.inner.recoverable),
        }
    }
}

impl fmt::Display for ApplicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}: {}",
            self.inner.code, self.inner.user_message
        )
    }
}

impl Error for ApplicationError {}

impl From<ApplicationError> for ApiError {
    fn from(value: ApplicationError) -> Self {
        value.to_api_error()
    }
}

impl From<&ApplicationError> for ApiError {
    fn from(value: &ApplicationError) -> Self {
        value.to_api_error()
    }
}

fn format_error_chain(error: &dyn Error) -> String {
    let mut parts = vec![redact_text(&error.to_string())];
    let mut current = error.source();
    while let Some(source) = current {
        parts.push(redact_text(&source.to_string()));
        current = source.source();
    }
    parts.join(" => ")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn api_error_from_application_error_omits_technical_fields() {
        let cause = std::io::Error::other("password=super-secret leaked");
        let error = ApplicationError::new(
            ErrorCode::GitCommandFailed,
            "Git could not inspect the worktree.",
        )
        .with_action("Check that Git is installed and the repository is readable.")
        .with_technical("git status exited 128 with password=super-secret")
        .with_context("GIT_TOKEN", "abc123")
        .with_context("path", "/tmp/project café")
        .with_source(&cause)
        .not_recoverable();

        let api = error.to_api_error();
        let value = serde_json::to_value(&api).expect("api error should serialize");

        assert_eq!(api.code, "GIT_COMMAND_FAILED");
        assert_eq!(api.message, "Git could not inspect the worktree.");
        assert_eq!(
            api.action.as_deref(),
            Some("Check that Git is installed and the repository is readable.")
        );
        assert_eq!(api.title.as_deref(), Some("Git command failed"));
        assert_eq!(api.recoverable, Some(false));
        assert_eq!(value["details"]["GIT_TOKEN"], "[redacted]");
        assert_eq!(value["details"]["path"], "/tmp/project café");
        assert!(value.get("technicalMessage").is_none());
        assert!(value.get("technical_message").is_none());
        assert!(value.get("sourceChain").is_none());
        assert!(value.get("source_chain").is_none());
        assert!(!value.to_string().contains("super-secret"));
        assert!(!value.to_string().contains("abc123"));
        assert!(error.source_chain().is_some());
        assert!(error.technical_message().contains("[redacted]"));
    }

    #[test]
    fn error_codes_have_stable_wire_values() {
        assert_eq!(ErrorCode::AgentNotFound.as_str(), "AGENT_NOT_FOUND");
        assert_eq!(ErrorCode::PtySpawnFailed.as_str(), "PTY_SPAWN_FAILED");
        assert_eq!(ErrorCode::WorktreeDirty.as_str(), "WORKTREE_DIRTY");
        assert_eq!(ErrorCode::InvalidPath.as_str(), "INVALID_PATH");
    }

    #[test]
    fn legacy_api_error_constructor_keeps_previous_shape() {
        let error = ApiError::new("executable_not_found", "Could not start Codex")
            .with_action("Install Codex")
            .with_detail("executable", "codex");
        let value = serde_json::to_value(&error).expect("api error should serialize");
        assert_eq!(
            value,
            json!({
                "code": "executable_not_found",
                "message": "Could not start Codex",
                "action": "Install Codex",
                "details": { "executable": "codex" }
            })
        );
    }
}
