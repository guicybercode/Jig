use std::{error::Error, fmt, io, path::PathBuf};

/// Stable category for a Git integration failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitErrorKind {
    /// Git could not be located or a required path does not exist.
    NotFound,
    /// Caller input or a filesystem path failed validation.
    InvalidInput,
    /// The directory is not a Git repository.
    NotRepository,
    /// Git exited unsuccessfully.
    CommandFailed,
    /// Git exceeded the configured execution deadline.
    Timeout,
    /// Git returned output that could not be interpreted safely.
    InvalidOutput,
    /// Filesystem or process I/O failed.
    Io,
    /// Removal was refused because the worktree contains changes.
    DirtyWorktree,
    /// Removal was refused because the worktree is running or in use.
    WorktreeInUse,
    /// A path escaped its configured managed worktree root.
    UnsafePath,
}

/// An actionable failure from the Git integration.
#[derive(Debug)]
pub struct GitError {
    kind: GitErrorKind,
    message: String,
    action: String,
    path: Option<PathBuf>,
    exit_status: Option<i32>,
    source: Option<io::Error>,
}

impl GitError {
    pub(crate) fn new(
        kind: GitErrorKind,
        message: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            message: message.into(),
            action: action.into(),
            path: None,
            exit_status: None,
            source: None,
        }
    }

    pub(crate) fn io(operation: &'static str, error: io::Error) -> Self {
        Self {
            kind: GitErrorKind::Io,
            message: format!("Could not {operation}: {error}"),
            action: "Check filesystem permissions and try again".to_owned(),
            path: None,
            exit_status: None,
            source: Some(error),
        }
    }

    pub(crate) fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = Some(path.into());
        self
    }

    pub(crate) const fn with_exit_status(mut self, status: Option<i32>) -> Self {
        self.exit_status = status;
        self
    }

    /// Returns the stable error category.
    #[must_use]
    pub const fn kind(&self) -> GitErrorKind {
        self.kind
    }

    /// Returns the concise user-facing failure description.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns a suggested recovery action.
    #[must_use]
    pub fn action(&self) -> &str {
        &self.action
    }

    /// Returns the relevant local path, when one is safe and useful to expose.
    #[must_use]
    pub fn path(&self) -> Option<&std::path::Path> {
        self.path.as_deref()
    }

    /// Returns the Git exit status, when the process exited normally.
    #[must_use]
    pub const fn exit_status(&self) -> Option<i32> {
        self.exit_status
    }
}

impl fmt::Display for GitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}. Action: {}", self.message, self.action)
    }
}

impl Error for GitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_ref().map(|error| error as _)
    }
}
