use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::PathBuf;
use std::process::ExitStatus;

/// Failure while locating Git or performing a repository operation.
#[derive(Debug)]
pub enum GitError {
    /// No executable `git` was found in the supplied search path.
    GitNotFound,
    /// The Git executable could not be started.
    Spawn(std::io::Error),
    /// Git ran and returned a non-zero status.
    CommandFailed {
        /// Subcommand name for diagnostics.
        operation: String,
        /// Process status.
        status: ExitStatus,
        /// Sanitized stderr, truncated and without secrets.
        stderr: String,
    },
    /// The selected path is not inside a Git work tree.
    NotARepository(PathBuf),
    /// Output was not valid UTF-8.
    InvalidUtf8(&'static str),
    /// A worktree path escaped the managed root after normalization.
    PathOutsideManagedRoot {
        /// Requested worktree path.
        path: PathBuf,
        /// Configured managed root.
        root: PathBuf,
    },
    /// Removal was refused because the worktree has uncommitted or untracked files.
    DirtyWorktree {
        /// Worktree path that is dirty.
        path: PathBuf,
    },
    /// The confirmation token no longer matches the observed Git state.
    StaleRemovalToken,
    /// The path is the repository's primary worktree and cannot be removed.
    PrimaryWorktree(PathBuf),
    /// The path is not registered as a worktree of the repository.
    UnknownWorktree(PathBuf),
}

impl Display for GitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::GitNotFound => formatter.write_str("git executable was not found in PATH"),
            Self::Spawn(error) => write!(formatter, "failed to start git: {error}"),
            Self::CommandFailed {
                operation,
                status,
                stderr,
            } => write!(
                formatter,
                "git {operation} failed with status {status}: {stderr}"
            ),
            Self::NotARepository(path) => {
                write!(
                    formatter,
                    "path is not a Git repository: {}",
                    path.display()
                )
            }
            Self::InvalidUtf8(operation) => {
                write!(formatter, "git {operation} produced non-UTF-8 output")
            }
            Self::PathOutsideManagedRoot { path, root } => write!(
                formatter,
                "worktree path {} is outside managed root {}",
                path.display(),
                root.display()
            ),
            Self::DirtyWorktree { path } => write!(
                formatter,
                "worktree {} has uncommitted or untracked changes",
                path.display()
            ),
            Self::StaleRemovalToken => {
                formatter.write_str("worktree state changed since prepare_remove")
            }
            Self::PrimaryWorktree(path) => write!(
                formatter,
                "refusing to remove the primary worktree {}",
                path.display()
            ),
            Self::UnknownWorktree(path) => {
                write!(
                    formatter,
                    "path is not a managed worktree: {}",
                    path.display()
                )
            }
        }
    }
}

impl Error for GitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Spawn(error) => Some(error),
            _ => None,
        }
    }
}

pub(crate) fn truncate_stderr(stderr: &[u8]) -> String {
    const LIMIT: usize = 2_048;
    let text = String::from_utf8_lossy(stderr);
    if text.len() <= LIMIT {
        text.into_owned()
    } else {
        format!("{}…", &text[..LIMIT])
    }
}
