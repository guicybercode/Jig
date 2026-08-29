use std::{error::Error, fmt, path::PathBuf};

use cli_master_core::{SessionId, WorktreeId};
use cli_master_git::{GitError, GitErrorKind};
use cli_master_storage::StorageError;

use crate::create::CreateStep;

/// Stable category for a session/worktree saga failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SagaErrorKind {
    /// Caller input failed validation.
    InvalidInput,
    /// A required project, agent, session, or worktree row is missing.
    NotFound,
    /// Two creates targeted the same planned destination at once.
    ConcurrentCreate,
    /// A mutation guard is already held for this worktree.
    MutationInProgress,
    /// Confirmation token is missing, expired, or bound to a different state.
    InvalidToken,
    /// Git created or left a worktree whose cleanup cannot be proven.
    PartialWorktree,
    /// Removal is blocked by dirty content or index protections.
    DirtyWorktree,
    /// Removal is blocked by a live session, lock, or other use.
    WorktreeInUse,
    /// Session metadata cannot be deleted while the process is live.
    SessionInUse,
    /// A test-only injected fault fired after a completed saga effect.
    InjectedFailure,
    /// The Git crate rejected the operation.
    Git,
    /// SQLite rejected the operation.
    Storage,
}

#[derive(Debug)]
struct SagaErrorDetails {
    message: String,
    action: String,
    path: Option<PathBuf>,
    worktree_id: Option<WorktreeId>,
    session_id: Option<SessionId>,
    source_git: Option<Box<GitError>>,
    source_storage: Option<Box<StorageError>>,
}

/// An actionable failure from the session/worktree saga.
#[derive(Debug)]
pub struct SagaError {
    kind: SagaErrorKind,
    details: Box<SagaErrorDetails>,
}

impl SagaError {
    pub(crate) fn new(
        kind: SagaErrorKind,
        message: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            details: Box::new(SagaErrorDetails {
                message: message.into(),
                action: action.into(),
                path: None,
                worktree_id: None,
                session_id: None,
                source_git: None,
                source_storage: None,
            }),
        }
    }

    pub(crate) fn injected(step: CreateStep) -> Self {
        Self::new(
            SagaErrorKind::InjectedFailure,
            format!("Injected saga failure after {step:?}"),
            "This fault exists only for tests; retry without fail_after",
        )
    }

    pub(crate) fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.details.path = Some(path.into());
        self
    }

    pub(crate) fn with_worktree_id(mut self, id: WorktreeId) -> Self {
        self.details.worktree_id = Some(id);
        self
    }

    pub(crate) fn with_session_id(mut self, id: SessionId) -> Self {
        self.details.session_id = Some(id);
        self
    }

    pub(crate) fn from_git(error: GitError) -> Self {
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
        Self {
            kind,
            details: Box::new(SagaErrorDetails {
                message: error.message().to_owned(),
                action: error.action().to_owned(),
                path: error.path().map(PathBuf::from),
                worktree_id: None,
                session_id: None,
                source_git: Some(Box::new(error)),
                source_storage: None,
            }),
        }
    }

    pub(crate) fn from_storage(error: StorageError) -> Self {
        let kind = match &error {
            StorageError::NotFound { .. } => SagaErrorKind::NotFound,
            StorageError::InvalidInput { .. } => SagaErrorKind::InvalidInput,
            _ => SagaErrorKind::Storage,
        };
        Self {
            kind,
            details: Box::new(SagaErrorDetails {
                message: error.to_string(),
                action: "Inspect SQLite metadata and retry the saga".to_owned(),
                path: None,
                worktree_id: None,
                session_id: None,
                source_git: None,
                source_storage: Some(Box::new(error)),
            }),
        }
    }

    pub(crate) fn partial_worktree(path: impl Into<PathBuf>, detail: impl Into<String>) -> Self {
        let path = path.into();
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

    /// Returns the stable error category.
    #[must_use]
    pub const fn kind(&self) -> SagaErrorKind {
        self.kind
    }

    /// Returns the concise user-facing failure description.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.details.message
    }

    /// Returns a suggested recovery action.
    #[must_use]
    pub fn action(&self) -> &str {
        &self.details.action
    }

    /// Returns the relevant local path, when one is safe to expose.
    #[must_use]
    pub fn path(&self) -> Option<&std::path::Path> {
        self.details.path.as_deref()
    }

    /// Returns the worktree identifier associated with the failure.
    #[must_use]
    pub const fn worktree_id(&self) -> Option<WorktreeId> {
        self.details.worktree_id
    }

    /// Returns the session identifier associated with the failure.
    #[must_use]
    pub const fn session_id(&self) -> Option<SessionId> {
        self.details.session_id
    }
}

impl fmt::Display for SagaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}. Action: {}",
            self.details.message, self.details.action
        )
    }
}

impl Error for SagaError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.details
            .source_git
            .as_deref()
            .map(|error| error as &(dyn Error + 'static))
            .or_else(|| {
                self.details
                    .source_storage
                    .as_deref()
                    .map(|error| error as &(dyn Error + 'static))
            })
    }
}

impl From<GitError> for SagaError {
    fn from(error: GitError) -> Self {
        Self::from_git(error)
    }
}

impl From<StorageError> for SagaError {
    fn from(error: StorageError) -> Self {
        Self::from_storage(error)
    }
}
