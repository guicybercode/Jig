use std::{error::Error, fmt, io, path::PathBuf};

use cli_master_core::{ApiError, SessionId, WorktreeId};
use cli_master_git::{GitError, GitErrorKind};
use cli_master_storage::StorageError;

use crate::create::CreateStep;

/// Stable category for PTY runtime and worktree-saga failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionErrorKind {
    /// No runtime or durable row exists for the requested identifier.
    NotFound,
    /// The session already has a live process.
    AlreadyRunning,
    /// The session has no live PTY.
    NotRunning,
    /// Runtime metadata cannot be deleted while its process is live.
    StillRunning,
    /// PTY rows or columns were invalid.
    InvalidSize,
    /// A session name was invalid.
    InvalidName,
    /// A process working directory was invalid.
    InvalidWorkingDirectory,
    /// Opening or configuring a PTY failed.
    Pty,
    /// The operating system rejected a spawn.
    Spawn,
    /// PTY I/O failed.
    Io,
    /// The bounded PTY writer queue timed out.
    WriteTimeout,
    /// Process-group shutdown did not complete before its deadline.
    StopTimeout,
    /// Process-group signaling failed.
    Signal,
    /// Caller input failed orchestration validation.
    InvalidInput,
    /// Two creates targeted the same planned destination at once.
    ConcurrentCreate,
    /// A mutation guard is already held for the worktree.
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
    /// The Git crate rejected an orchestration operation.
    Git,
    /// SQLite rejected an orchestration operation.
    Storage,
}

/// Actionable context attached to a Git/SQLite orchestration failure.
///
/// Fields stay private so callers use [`SessionError`] accessors and the
/// stable [`ApiError`] conversion instead of depending on internal sources.
#[derive(Debug)]
pub struct SessionOperationDetails {
    kind: SessionErrorKind,
    message: String,
    action: String,
    path: Option<PathBuf>,
    worktree_id: Option<WorktreeId>,
    session_id: Option<SessionId>,
    source_git: Option<Box<GitError>>,
    source_storage: Option<Box<StorageError>>,
}

/// Failure while orchestrating, starting, I/O-ing, or stopping a session.
///
/// One error surface keeps the daemon adapter from having to translate a
/// separate PTY error hierarchy and saga error hierarchy into the wire
/// contract. Operation details never contain command environment values.
#[derive(Debug)]
pub enum SessionError {
    /// No in-memory session exists for the requested identifier.
    NotFound(SessionId),
    /// The session already has a live process.
    AlreadyRunning(SessionId),
    /// The session has no live PTY to receive input or resize.
    NotRunning(SessionId),
    /// Metadata cannot be deleted while the process is still live.
    StillRunning(SessionId),
    /// Rows or columns were zero.
    InvalidSize,
    /// The session name was empty after trimming.
    InvalidName,
    /// The working directory is missing or not a directory.
    InvalidWorkingDirectory(PathBuf),
    /// Opening or configuring the PTY failed.
    Pty(String),
    /// The operating system rejected the spawn.
    Spawn(String),
    /// A PTY read or write failed.
    Io(String),
    /// The writer queue stayed full past the write timeout.
    WriteTimeout,
    /// The process group was signaled through SIGKILL but did not exit in time.
    StopTimeout(SessionId),
    /// Signaling the process group failed.
    Signal(String),
    /// A recoverable orchestration operation failed with actionable context.
    Operation(Box<SessionOperationDetails>),
}

impl SessionError {
    pub(crate) fn io(error: &io::Error) -> Self {
        Self::Io(error.to_string())
    }

    pub(crate) fn new(
        kind: SessionErrorKind,
        message: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self::Operation(Box::new(SessionOperationDetails {
            kind,
            message: message.into(),
            action: action.into(),
            path: None,
            worktree_id: None,
            session_id: None,
            source_git: None,
            source_storage: None,
        }))
    }

    pub(crate) fn injected(step: CreateStep) -> Self {
        Self::new(
            SessionErrorKind::InjectedFailure,
            format!("Injected saga failure after {step:?}"),
            "This fault exists only for tests; retry without fail_after",
        )
    }

    pub(crate) fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        if let Self::Operation(details) = &mut self {
            details.path = Some(path.into());
        }
        self
    }

    pub(crate) fn with_worktree_id(mut self, id: WorktreeId) -> Self {
        if let Self::Operation(details) = &mut self {
            details.worktree_id = Some(id);
        }
        self
    }

    pub(crate) fn with_session_id(mut self, id: SessionId) -> Self {
        if let Self::Operation(details) = &mut self {
            details.session_id = Some(id);
        }
        self
    }

    pub(crate) fn from_git(error: GitError) -> Self {
        let kind = match error.kind() {
            GitErrorKind::PartialWorktree => SessionErrorKind::PartialWorktree,
            GitErrorKind::DirtyWorktree => SessionErrorKind::DirtyWorktree,
            GitErrorKind::WorktreeInUse => SessionErrorKind::WorktreeInUse,
            GitErrorKind::InvalidInput | GitErrorKind::UnsafePath | GitErrorKind::NotRepository => {
                SessionErrorKind::InvalidInput
            }
            GitErrorKind::NotFound => SessionErrorKind::NotFound,
            _ => SessionErrorKind::Git,
        };
        Self::Operation(Box::new(SessionOperationDetails {
            kind,
            message: error.message().to_owned(),
            action: error.action().to_owned(),
            path: error.path().map(PathBuf::from),
            worktree_id: None,
            session_id: None,
            source_git: Some(Box::new(error)),
            source_storage: None,
        }))
    }

    pub(crate) fn from_storage(error: StorageError) -> Self {
        let kind = match &error {
            StorageError::NotFound { .. } => SessionErrorKind::NotFound,
            StorageError::InvalidInput { .. } => SessionErrorKind::InvalidInput,
            _ => SessionErrorKind::Storage,
        };
        Self::Operation(Box::new(SessionOperationDetails {
            kind,
            message: error.to_string(),
            action: "Inspect SQLite metadata and retry the session operation".to_owned(),
            path: None,
            worktree_id: None,
            session_id: None,
            source_git: None,
            source_storage: Some(Box::new(error)),
        }))
    }

    pub(crate) fn partial_worktree(path: impl Into<PathBuf>, detail: impl Into<String>) -> Self {
        let path = path.into();
        Self::new(
            SessionErrorKind::PartialWorktree,
            format!(
                "Worktree data was preserved because compensation could not be proven: {}",
                detail.into()
            ),
            "Inspect the path with `git worktree list` and preserve any user data; do not retry automatically",
        )
        .with_path(path)
    }

    /// Returns the stable category for logging and wire translation.
    #[must_use]
    pub const fn kind(&self) -> SessionErrorKind {
        match self {
            Self::NotFound(_) => SessionErrorKind::NotFound,
            Self::AlreadyRunning(_) => SessionErrorKind::AlreadyRunning,
            Self::NotRunning(_) => SessionErrorKind::NotRunning,
            Self::StillRunning(_) => SessionErrorKind::StillRunning,
            Self::InvalidSize => SessionErrorKind::InvalidSize,
            Self::InvalidName => SessionErrorKind::InvalidName,
            Self::InvalidWorkingDirectory(_) => SessionErrorKind::InvalidWorkingDirectory,
            Self::Pty(_) => SessionErrorKind::Pty,
            Self::Spawn(_) => SessionErrorKind::Spawn,
            Self::Io(_) => SessionErrorKind::Io,
            Self::WriteTimeout => SessionErrorKind::WriteTimeout,
            Self::StopTimeout(_) => SessionErrorKind::StopTimeout,
            Self::Signal(_) => SessionErrorKind::Signal,
            Self::Operation(details) => details.kind,
        }
    }

    /// Stable IPC error code for this failure.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self.kind() {
            SessionErrorKind::NotFound => "session_not_found",
            SessionErrorKind::AlreadyRunning => "session_already_running",
            SessionErrorKind::NotRunning => "session_not_running",
            SessionErrorKind::StillRunning | SessionErrorKind::SessionInUse => {
                "session_still_running"
            }
            SessionErrorKind::InvalidSize => "session_invalid_size",
            SessionErrorKind::InvalidName => "session_invalid_name",
            SessionErrorKind::InvalidWorkingDirectory => "session_invalid_cwd",
            SessionErrorKind::Pty => "session_pty_failed",
            SessionErrorKind::Spawn => "session_spawn_failed",
            SessionErrorKind::Io => "session_io_failed",
            SessionErrorKind::WriteTimeout => "session_write_timeout",
            SessionErrorKind::StopTimeout => "session_stop_timeout",
            SessionErrorKind::Signal => "session_signal_failed",
            SessionErrorKind::InvalidInput => "session_invalid_input",
            SessionErrorKind::ConcurrentCreate => "session_create_in_progress",
            SessionErrorKind::MutationInProgress => "worktree_mutation_in_progress",
            SessionErrorKind::InvalidToken => "worktree_confirmation_invalid",
            SessionErrorKind::PartialWorktree => "worktree_partial",
            SessionErrorKind::DirtyWorktree => "worktree_dirty",
            SessionErrorKind::WorktreeInUse => "worktree_in_use",
            SessionErrorKind::InjectedFailure => "session_injected_failure",
            SessionErrorKind::Git => "session_git_failed",
            SessionErrorKind::Storage => "session_storage_failed",
        }
    }

    /// Returns a concise operation message without exposing command secrets.
    #[must_use]
    pub fn message(&self) -> &str {
        match self {
            Self::Operation(details) => &details.message,
            Self::Pty(message)
            | Self::Spawn(message)
            | Self::Io(message)
            | Self::Signal(message) => message,
            Self::NotFound(_) => "session was not found",
            Self::AlreadyRunning(_) => "session is already running",
            Self::NotRunning(_) => "session is not running",
            Self::StillRunning(_) => "session is still running",
            Self::InvalidSize => "PTY size is invalid",
            Self::InvalidName => "session name is invalid",
            Self::InvalidWorkingDirectory(_) => "session working directory is invalid",
            Self::WriteTimeout => "timed out writing to the PTY",
            Self::StopTimeout(_) => "session process group did not stop in time",
        }
    }

    /// Returns the orchestration recovery action, when one is available.
    #[must_use]
    pub fn action(&self) -> Option<&str> {
        match self {
            Self::Operation(details) => Some(&details.action),
            _ => None,
        }
    }

    /// Returns the relevant local path, when one is safe to expose.
    #[must_use]
    pub fn path(&self) -> Option<&std::path::Path> {
        match self {
            Self::InvalidWorkingDirectory(path) => Some(path),
            Self::Operation(details) => details.path.as_deref(),
            _ => None,
        }
    }

    /// Returns the worktree identifier associated with the failure.
    #[must_use]
    pub const fn worktree_id(&self) -> Option<WorktreeId> {
        match self {
            Self::Operation(details) => details.worktree_id,
            _ => None,
        }
    }

    /// Returns the session identifier associated with the failure.
    #[must_use]
    pub const fn session_id(&self) -> Option<SessionId> {
        match self {
            Self::NotFound(id)
            | Self::AlreadyRunning(id)
            | Self::NotRunning(id)
            | Self::StillRunning(id)
            | Self::StopTimeout(id) => Some(*id),
            Self::Operation(details) => details.session_id,
            _ => None,
        }
    }
}

impl fmt::Display for SessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(id) => write!(formatter, "session {id} was not found"),
            Self::AlreadyRunning(id) => write!(formatter, "session {id} is already running"),
            Self::NotRunning(id) => write!(formatter, "session {id} is not running"),
            Self::StillRunning(id) => {
                write!(
                    formatter,
                    "session {id} is still running and cannot be deleted"
                )
            }
            Self::InvalidSize => {
                formatter.write_str("PTY rows and columns must be greater than zero")
            }
            Self::InvalidName => formatter.write_str("session name must not be empty"),
            Self::InvalidWorkingDirectory(path) => write!(
                formatter,
                "working directory is not a directory: {}",
                path.display()
            ),
            Self::Pty(message) => write!(formatter, "PTY error: {message}"),
            Self::Spawn(message) => write!(formatter, "failed to spawn process: {message}"),
            Self::Io(message) => write!(formatter, "session I/O error: {message}"),
            Self::WriteTimeout => formatter.write_str("timed out writing to the PTY"),
            Self::StopTimeout(id) => {
                write!(
                    formatter,
                    "session {id} did not exit after signal escalation"
                )
            }
            Self::Signal(message) => write!(formatter, "failed to signal process group: {message}"),
            Self::Operation(details) => {
                write!(formatter, "{}. Action: {}", details.message, details.action)
            }
        }
    }
}

impl Error for SessionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        let Self::Operation(details) = self else {
            return None;
        };
        details
            .source_git
            .as_deref()
            .map(|error| error as &(dyn Error + 'static))
            .or_else(|| {
                details
                    .source_storage
                    .as_deref()
                    .map(|error| error as &(dyn Error + 'static))
            })
    }
}

impl From<GitError> for SessionError {
    fn from(error: GitError) -> Self {
        Self::from_git(error)
    }
}

impl From<StorageError> for SessionError {
    fn from(error: StorageError) -> Self {
        Self::from_storage(error)
    }
}

impl From<SessionError> for ApiError {
    fn from(error: SessionError) -> Self {
        let mut api = Self::new(error.code(), error.to_string());
        match error {
            SessionError::InvalidWorkingDirectory(path) => {
                api = api.with_detail("cwd", path.display().to_string());
            }
            SessionError::NotFound(id)
            | SessionError::AlreadyRunning(id)
            | SessionError::NotRunning(id)
            | SessionError::StillRunning(id)
            | SessionError::StopTimeout(id) => {
                api = api.with_detail("sessionId", id.to_string());
            }
            SessionError::Operation(details) => {
                api = api.with_action(details.action);
                if let Some(path) = details.path {
                    api = api.with_detail("path", path.display().to_string());
                }
                if let Some(worktree_id) = details.worktree_id {
                    api = api.with_detail("worktreeId", worktree_id.to_string());
                }
                if let Some(session_id) = details.session_id {
                    api = api.with_detail("sessionId", session_id.to_string());
                }
            }
            _ => {}
        }
        api
    }
}

/// Backward-compatible name for saga-specific call sites.
pub type SagaError = SessionError;
/// Backward-compatible name for saga-specific category assertions.
pub type SagaErrorKind = SessionErrorKind;
