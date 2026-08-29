use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Stable machine-readable error codes returned across the IPC boundary.
///
/// Codes are additive within protocol v1. Removing or renaming a code requires
/// a new protocol major version.
pub mod error_codes {
    /// The client requested a protocol version the daemon does not speak.
    pub const PROTOCOL_UNSUPPORTED: &str = "PROTOCOL_UNSUPPORTED";
    /// The request named a method that is not in the v1 catalog.
    pub const PROTOCOL_UNKNOWN_METHOD: &str = "PROTOCOL_UNKNOWN_METHOD";
    /// The request payload failed schema or domain validation.
    pub const PROTOCOL_INVALID_PAYLOAD: &str = "PROTOCOL_INVALID_PAYLOAD";

    /// No daemon is listening, or the handshake could not complete.
    pub const DAEMON_UNAVAILABLE: &str = "DAEMON_UNAVAILABLE";
    /// Another `cli-masterd` instance already owns this user's lock.
    pub const DAEMON_ALREADY_RUNNING: &str = "DAEMON_ALREADY_RUNNING";

    /// The referenced project does not exist in metadata.
    pub const PROJECT_NOT_FOUND: &str = "PROJECT_NOT_FOUND";
    /// The selected path is not a Git repository root.
    pub const PROJECT_NOT_A_REPOSITORY: &str = "PROJECT_NOT_A_REPOSITORY";
    /// The project is still referenced by sessions or worktrees.
    pub const PROJECT_IN_USE: &str = "PROJECT_IN_USE";
    /// A project with this canonical path is already registered.
    pub const PROJECT_DUPLICATE_PATH: &str = "PROJECT_DUPLICATE_PATH";

    /// The referenced agent definition does not exist.
    pub const AGENT_NOT_FOUND: &str = "AGENT_NOT_FOUND";
    /// The agent executable was not found in the configured search path.
    pub const AGENT_EXECUTABLE_NOT_FOUND: &str = "AGENT_EXECUTABLE_NOT_FOUND";
    /// A candidate exists but is not an executable regular file.
    pub const AGENT_NOT_EXECUTABLE: &str = "AGENT_NOT_EXECUTABLE";
    /// Built-in agent defaults cannot be mutated.
    pub const AGENT_BUILTIN_IMMUTABLE: &str = "AGENT_BUILTIN_IMMUTABLE";
    /// The agent is still referenced by session history.
    pub const AGENT_IN_USE: &str = "AGENT_IN_USE";
    /// A custom agent key collides with a built-in or existing custom key.
    pub const AGENT_DUPLICATE_ID: &str = "AGENT_DUPLICATE_ID";

    /// The referenced session does not exist in metadata.
    pub const SESSION_NOT_FOUND: &str = "SESSION_NOT_FOUND";
    /// The requested session status transition is not allowed.
    pub const SESSION_INVALID_TRANSITION: &str = "SESSION_INVALID_TRANSITION";
    /// The session has no live PTY, so write/resize/stop cannot run.
    pub const SESSION_NOT_LIVE: &str = "SESSION_NOT_LIVE";
    /// Delete was requested while a process is still live.
    pub const SESSION_STILL_RUNNING: &str = "SESSION_STILL_RUNNING";

    /// The referenced worktree does not exist in metadata.
    pub const WORKTREE_NOT_FOUND: &str = "WORKTREE_NOT_FOUND";
    /// The worktree is in use by a live session.
    pub const WORKTREE_IN_USE: &str = "WORKTREE_IN_USE";
    /// Removal is blocked because the worktree has uncommitted changes.
    pub const WORKTREE_DIRTY: &str = "WORKTREE_DIRTY";
    /// The confirmation token does not match the current worktree state.
    pub const WORKTREE_STALE_TOKEN: &str = "WORKTREE_STALE_TOKEN";
    /// Generated or requested worktree path escaped the managed root.
    pub const WORKTREE_PATH_UNSAFE: &str = "WORKTREE_PATH_UNSAFE";

    /// The system Git executable could not be resolved.
    pub const GIT_EXECUTABLE_NOT_FOUND: &str = "GIT_EXECUTABLE_NOT_FOUND";
    /// Git exited unsuccessfully. Details contain the sanitized stderr.
    pub const GIT_COMMAND_FAILED: &str = "GIT_COMMAND_FAILED";

    /// SQLite rejected an operation or the schema is newer than this binary.
    pub const STORAGE_FAILURE: &str = "STORAGE_FAILURE";
}

/// A stable, actionable error returned across the IPC boundary.
///
/// This is the only error shape the desktop UI is allowed to render from
/// command failures. Internal Rust error types stay behind the daemon.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ApplicationError {
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
}

impl ApplicationError {
    /// Creates an error without an action or diagnostic details.
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            action: None,
            details: BTreeMap::new(),
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
}

/// Backward-compatible alias used by early protocol tests and call sites.
pub type ApiError = ApplicationError;
