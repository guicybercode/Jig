use std::{io, path::PathBuf};

use cli_master_core::{ApiError, SessionId, SessionStatus, WorktreeId};
use cli_master_git::{GitError, GitErrorKind};
use cli_master_storage::StorageError;
use thiserror::Error;

use crate::create::CreateStep;

/// Failure returned by PTY session operations.
#[derive(Debug, Error)]
pub enum SessionError {
    /// A manager resource limit or deadline is invalid.
    #[error("invalid session manager configuration `{field}`: {reason}")]
    InvalidConfiguration {
        /// Invalid configuration field.
        field: &'static str,
        /// Expected constraint.
        reason: &'static str,
    },
    /// Rows and columns must both be non-zero.
    #[error("invalid terminal size rows={rows}, cols={cols}: both must be greater than zero")]
    InvalidTerminalSize {
        /// Requested row count.
        rows: u16,
        /// Requested column count.
        cols: u16,
    },
    /// The command working directory is missing or is not a directory.
    #[error("working directory is unavailable: {path:?}")]
    WorkingDirectoryUnavailable {
        /// Requested working directory.
        path: PathBuf,
    },
    /// The native PTY could not be opened.
    #[error("failed to open a native pseudo-terminal")]
    OpenPty {
        /// Underlying PTY error.
        #[source]
        source: anyhow::Error,
    },
    /// A PTY reader or writer could not be acquired.
    #[error("failed to acquire the pseudo-terminal {stream} stream")]
    OpenPtyStream {
        /// Stream direction.
        stream: &'static str,
        /// Underlying PTY error.
        #[source]
        source: anyhow::Error,
    },
    /// The structured executable could not be launched.
    #[error("failed to spawn executable {executable:?} in the pseudo-terminal")]
    Spawn {
        /// Executable name or path; arguments and environment are omitted.
        executable: String,
        /// Underlying process error.
        #[source]
        source: anyhow::Error,
    },
    /// The PTY backend launched a process without a usable Unix identifier.
    #[error("spawned executable {executable:?} did not expose a usable process identifier")]
    ProcessIdUnavailable {
        /// Executable name or path; arguments and environment are omitted.
        executable: String,
    },
    /// The operating-system process tree could not be inspected safely.
    #[error("failed to inspect the process tree for session {session_id}")]
    ProcessInspection {
        /// Target session identifier.
        session_id: SessionId,
        /// Underlying operating-system error.
        #[source]
        source: io::Error,
    },
    /// A required background worker could not be created.
    #[error("failed to start session worker `{role}`")]
    WorkerStart {
        /// Worker responsibility.
        role: &'static str,
        /// Underlying operating-system error.
        #[source]
        source: io::Error,
    },
    /// A background worker panicked during explicit cleanup.
    #[error("session worker `{role}` terminated unexpectedly")]
    WorkerPanicked {
        /// Worker responsibility.
        role: &'static str,
    },
    /// A background worker did not finish before bounded cleanup elapsed.
    #[error("session worker `{role}` did not stop before the cleanup deadline")]
    WorkerJoinTimedOut {
        /// Worker responsibility.
        role: &'static str,
    },
    /// No runtime exists for the requested identifier.
    #[error("session {session_id} was not found")]
    NotFound {
        /// Requested session identifier.
        session_id: SessionId,
    },
    /// A runtime is already registered for the requested identifier.
    #[error("session {session_id} is already registered")]
    DuplicateSessionId {
        /// Identifier that is already owned by this manager.
        session_id: SessionId,
    },
    /// The manager has begun its one-way shutdown and accepts no new sessions.
    #[error("session manager is shutting down")]
    ManagerShuttingDown,
    /// The operation requires a live process.
    #[error("session {session_id} is not live (status: {status:?})")]
    NotLive {
        /// Target session identifier.
        session_id: SessionId,
        /// Current terminal state.
        status: SessionStatus,
    },
    /// The runtime is still being torn down but no longer accepts interaction.
    #[error("interaction with session {session_id} is unavailable")]
    InteractionUnavailable {
        /// Target session identifier.
        session_id: SessionId,
    },
    /// One input payload exceeded the configured bound.
    #[error("input for session {session_id} has {actual} bytes; maximum is {maximum}")]
    InputTooLarge {
        /// Target session identifier.
        session_id: SessionId,
        /// Received byte count.
        actual: usize,
        /// Configured byte limit.
        maximum: usize,
    },
    /// The bounded writer queue is full.
    #[error("input queue for session {session_id} is full")]
    InputBackpressure {
        /// Target session identifier.
        session_id: SessionId,
    },
    /// The PTY writer is no longer available.
    #[error("input stream for session {session_id} is unavailable")]
    InputUnavailable {
        /// Target session identifier.
        session_id: SessionId,
    },
    /// A queued write was cancelled before any byte reached the PTY.
    #[error("input write for session {session_id} exceeded its deadline before delivery")]
    InputTimedOut {
        /// Target session identifier.
        session_id: SessionId,
    },
    /// A write deadline or I/O failure occurred after delivery may have begun.
    ///
    /// The runtime is invalidated and proven process-tree termination is attempted
    /// before this error is returned. Callers must not retry the input because
    /// any prefix, or the complete payload, may already have reached the child.
    #[error("input delivery for session {session_id} is ambiguous; the session was invalidated")]
    InputDeliveryAmbiguous {
        /// Target session identifier.
        session_id: SessionId,
    },
    /// The operating system rejected a PTY write.
    #[error("input write for session {session_id} failed")]
    WriteFailed {
        /// Target session identifier.
        session_id: SessionId,
    },
    /// The operating system rejected a PTY resize.
    #[error("failed to resize pseudo-terminal for session {session_id}")]
    Resize {
        /// Target session identifier.
        session_id: SessionId,
        /// Underlying PTY error.
        #[source]
        source: anyhow::Error,
    },
    /// A process-tree signal could not be delivered.
    #[error("failed to deliver {signal} to session {session_id}")]
    Signal {
        /// Target session identifier.
        session_id: SessionId,
        /// Signal name.
        signal: &'static str,
        /// Underlying operating-system error.
        #[source]
        source: nix::errno::Errno,
    },
    /// The child process could not be polled or reaped reliably.
    #[error("failed to supervise child process for session {session_id}")]
    Supervision {
        /// Target session identifier.
        session_id: SessionId,
        /// Underlying operating-system error.
        #[source]
        source: io::Error,
    },
    /// The force-kill deadline expired before the child was reaped.
    #[error("session {session_id} did not exit before the kill deadline")]
    StopTimedOut {
        /// Target session identifier.
        session_id: SessionId,
    },
    /// Only completed sessions may be removed from the manager.
    #[error("session {session_id} cannot be removed while status is {status:?}")]
    RemoveLive {
        /// Target session identifier.
        session_id: SessionId,
        /// Current live state.
        status: SessionStatus,
    },
    /// A reconnect cursor points beyond all output produced by the session.
    #[error(
        "replay cursor {requested} for session {session_id} is ahead of latest sequence {latest}"
    )]
    ReplayCursorAhead {
        /// Target session identifier.
        session_id: SessionId,
        /// Requested last-seen sequence.
        requested: u64,
        /// Latest sequence produced by the session.
        latest: u64,
    },
    /// The output sequence counter reached its representable maximum.
    #[error("output sequence for session {session_id} is exhausted")]
    SequenceExhausted {
        /// Target session identifier.
        session_id: SessionId,
    },
}

/// Stable category for recoverable worktree-saga failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SagaErrorKind {
    /// Required durable metadata was not found.
    NotFound,
    /// The process launcher rejected or could not start a command.
    Spawn,
    /// Caller input failed orchestration validation.
    InvalidInput,
    /// Two creates targeted the same destination concurrently.
    ConcurrentCreate,
    /// Another worktree mutation is already in progress.
    MutationInProgress,
    /// A confirmation token is invalid, expired, or stale.
    InvalidToken,
    /// Git left a worktree whose cleanup cannot be proven.
    PartialWorktree,
    /// Worktree removal is blocked by local changes.
    DirtyWorktree,
    /// Worktree removal is blocked by a live owner or Git lock.
    WorktreeInUse,
    /// Durable session metadata is still owned by a live process.
    SessionInUse,
    /// A test-only injected saga fault fired.
    InjectedFailure,
    /// The Git adapter rejected an operation.
    Git,
    /// The storage adapter rejected an operation.
    Storage,
}

/// Actionable failure returned by the recoverable worktree saga.
#[derive(Debug)]
pub struct SagaError {
    kind: SagaErrorKind,
    message: String,
    action: String,
    path: Option<PathBuf>,
    worktree_id: Option<WorktreeId>,
    session_id: Option<SessionId>,
}

impl SagaError {
    /// Creates an actionable saga failure with a stable category.
    #[must_use]
    pub fn new(kind: SagaErrorKind, message: impl Into<String>, action: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            action: action.into(),
            path: None,
            worktree_id: None,
            session_id: None,
        }
    }

    pub(crate) fn injected(step: CreateStep) -> Self {
        Self::new(
            SagaErrorKind::InjectedFailure,
            format!("Injected saga failure after {step:?}"),
            "Retry without the test-only fault hook",
        )
    }

    pub(crate) fn partial_worktree(path: impl Into<PathBuf>, detail: impl Into<String>) -> Self {
        Self::new(
            SagaErrorKind::PartialWorktree,
            format!(
                "Worktree data was preserved because compensation could not be proven: {}",
                detail.into()
            ),
            "Inspect the path with `git worktree list` and preserve any user data; do not retry automatically",
        )
        .with_path(path)
    }

    pub(crate) fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = Some(path.into());
        self
    }

    pub(crate) const fn with_worktree_id(mut self, id: WorktreeId) -> Self {
        self.worktree_id = Some(id);
        self
    }

    pub(crate) const fn with_session_id(mut self, id: SessionId) -> Self {
        self.session_id = Some(id);
        self
    }

    /// Returns the stable category for logging and wire translation.
    #[must_use]
    pub const fn kind(&self) -> SagaErrorKind {
        self.kind
    }

    /// Returns the stable IPC error code for this failure.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self.kind {
            SagaErrorKind::NotFound => "session_not_found",
            SagaErrorKind::Spawn => "session_spawn_failed",
            SagaErrorKind::InvalidInput => "session_invalid_input",
            SagaErrorKind::ConcurrentCreate => "session_create_in_progress",
            SagaErrorKind::MutationInProgress => "worktree_mutation_in_progress",
            SagaErrorKind::InvalidToken => "worktree_confirmation_invalid",
            SagaErrorKind::PartialWorktree => "worktree_partial",
            SagaErrorKind::DirtyWorktree => "worktree_dirty",
            SagaErrorKind::WorktreeInUse => "worktree_in_use",
            SagaErrorKind::SessionInUse => "session_still_running",
            SagaErrorKind::InjectedFailure => "session_injected_failure",
            SagaErrorKind::Git => "session_git_failed",
            SagaErrorKind::Storage => "session_storage_failed",
        }
    }

    /// Returns a concise, non-secret operation message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the suggested recovery action.
    #[must_use]
    pub fn action(&self) -> &str {
        &self.action
    }

    /// Returns the relevant local path, when present.
    #[must_use]
    pub fn path(&self) -> Option<&std::path::Path> {
        self.path.as_deref()
    }

    /// Returns the related worktree identifier, when present.
    #[must_use]
    pub const fn worktree_id(&self) -> Option<WorktreeId> {
        self.worktree_id
    }

    /// Returns the related session identifier, when present.
    #[must_use]
    pub const fn session_id(&self) -> Option<SessionId> {
        self.session_id
    }
}

impl std::fmt::Display for SagaError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}. Action: {}", self.message, self.action)
    }
}

impl std::error::Error for SagaError {}

impl From<GitError> for SagaError {
    fn from(error: GitError) -> Self {
        let kind = match error.kind() {
            GitErrorKind::PartialWorktree => SagaErrorKind::PartialWorktree,
            GitErrorKind::DirtyWorktree => SagaErrorKind::DirtyWorktree,
            GitErrorKind::WorktreeInUse => SagaErrorKind::WorktreeInUse,
            GitErrorKind::InvalidInput | GitErrorKind::UnsafePath | GitErrorKind::NotRepository => {
                SagaErrorKind::InvalidInput
            }
            GitErrorKind::NotFound => SagaErrorKind::NotFound,
            _ => SagaErrorKind::Git,
        };
        let mut saga = Self::new(kind, error.message(), error.action());
        if let Some(path) = error.path() {
            saga = saga.with_path(path);
        }
        saga
    }
}

impl From<StorageError> for SagaError {
    fn from(error: StorageError) -> Self {
        let kind = match &error {
            StorageError::NotFound { .. } => SagaErrorKind::NotFound,
            StorageError::InvalidInput { .. } => SagaErrorKind::InvalidInput,
            _ => SagaErrorKind::Storage,
        };
        Self::new(
            kind,
            error.to_string(),
            "Inspect the local metadata and retry the session operation",
        )
    }
}

impl From<SessionError> for SagaError {
    fn from(error: SessionError) -> Self {
        let kind = match &error {
            SessionError::NotFound { .. } => SagaErrorKind::NotFound,
            SessionError::Spawn { .. } | SessionError::ProcessIdUnavailable { .. } => {
                SagaErrorKind::Spawn
            }
            _ => SagaErrorKind::Spawn,
        };
        Self::new(
            kind,
            error.to_string(),
            "Retry the session or restart Jig if the process cannot be recovered",
        )
    }
}

impl From<SagaError> for ApiError {
    fn from(error: SagaError) -> Self {
        let mut api = Self::new(error.code(), error.message).with_action(error.action);
        if let Some(path) = error.path {
            api = api.with_detail("path", path.display().to_string());
        }
        if let Some(worktree_id) = error.worktree_id {
            api = api.with_detail("worktreeId", worktree_id.to_string());
        }
        if let Some(session_id) = error.session_id {
            api = api.with_detail("sessionId", session_id.to_string());
        }
        api
    }
}
