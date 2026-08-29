use std::{error::Error, fmt, path::PathBuf};

use cli_master_core::ApiError;

/// Stable machine-readable codes for Git operations.
pub mod code {
    /// No `git` executable was found on `PATH`.
    pub const EXECUTABLE_NOT_FOUND: &str = "GIT_EXECUTABLE_NOT_FOUND";
    /// The requested path does not exist.
    pub const PATH_NOT_FOUND: &str = "GIT_PATH_NOT_FOUND";
    /// The requested path exists but is not a directory.
    pub const NOT_A_DIRECTORY: &str = "GIT_NOT_A_DIRECTORY";
    /// The path is not inside a Git repository.
    pub const NOT_A_REPOSITORY: &str = "GIT_NOT_A_REPOSITORY";
    /// The repository is bare and cannot host a worktree checkout.
    pub const BARE_REPOSITORY: &str = "GIT_BARE_REPOSITORY";
    /// `HEAD` has no commits, so a worktree cannot be created from it.
    pub const UNBORN_HEAD: &str = "GIT_UNBORN_HEAD";
    /// The requested base ref does not resolve to a commit.
    pub const INVALID_REF: &str = "GIT_INVALID_REF";
    /// A generated or requested branch already exists.
    pub const BRANCH_EXISTS: &str = "GIT_BRANCH_EXISTS";
    /// A generated worktree path already exists.
    pub const WORKTREE_EXISTS: &str = "GIT_WORKTREE_EXISTS";
    /// The path is not a worktree known to this repository.
    pub const WORKTREE_UNKNOWN: &str = "GIT_WORKTREE_UNKNOWN";
    /// The worktree has uncommitted or untracked changes.
    pub const WORKTREE_DIRTY: &str = "GIT_WORKTREE_DIRTY";
    /// A live session is still using the worktree.
    pub const WORKTREE_IN_USE: &str = "GIT_WORKTREE_IN_USE";
    /// Git reports the worktree as locked.
    pub const WORKTREE_LOCKED: &str = "GIT_WORKTREE_LOCKED";
    /// Removal of the primary repository worktree is refused.
    pub const PRIMARY_WORKTREE: &str = "GIT_PRIMARY_WORKTREE_PROTECTED";
    /// The path escaped the managed worktree root.
    pub const PATH_OUTSIDE_ROOT: &str = "GIT_PATH_OUTSIDE_MANAGED_ROOT";
    /// The path is a symlink that would be followed to an unexpected location.
    pub const SYMLINK_REJECTED: &str = "GIT_SYMLINK_REJECTED";
    /// Removal was requested without a valid confirmation token.
    pub const CONFIRMATION_REQUIRED: &str = "GIT_CONFIRMATION_REQUIRED";
    /// The confirmation token does not match the current worktree state.
    pub const CONFIRMATION_MISMATCH: &str = "GIT_CONFIRMATION_MISMATCH";
    /// The confirmation token expired.
    pub const CONFIRMATION_EXPIRED: &str = "GIT_CONFIRMATION_EXPIRED";
    /// The session name could not be turned into a safe branch or directory.
    pub const NAME_INVALID: &str = "GIT_NAME_INVALID";
    /// Git produced output that could not be parsed.
    pub const INVALID_OUTPUT: &str = "GIT_INVALID_OUTPUT";
    /// The Git process could not be started.
    pub const SPAWN_FAILED: &str = "GIT_SPAWN_FAILED";
    /// Git exited unsuccessfully.
    pub const COMMAND_FAILED: &str = "GIT_COMMAND_FAILED";
    /// Compensating cleanup after a partial failure also failed.
    pub const ROLLBACK_FAILED: &str = "GIT_ROLLBACK_FAILED";
    /// An internal lock or invariant failed.
    pub const INTERNAL: &str = "GIT_INTERNAL";
}

/// Failure from a [`GitService`](crate::GitService) operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GitError {
    /// No Git executable was found.
    ExecutableNotFound,
    /// The path does not exist on disk.
    PathNotFound {
        /// Path that was requested.
        path: PathBuf,
    },
    /// The path exists but is not a directory.
    NotADirectory {
        /// Path that was requested.
        path: PathBuf,
    },
    /// The path is not inside a Git work tree or repository.
    NotARepository {
        /// Path that was inspected.
        path: PathBuf,
    },
    /// The repository is bare.
    BareRepository {
        /// Repository path.
        path: PathBuf,
    },
    /// The repository has no commits yet.
    UnbornHead {
        /// Repository path.
        path: PathBuf,
    },
    /// A ref name did not resolve to a commit.
    InvalidRef {
        /// Ref that was requested.
        reference: String,
    },
    /// The branch already exists.
    BranchExists {
        /// Conflicting branch name.
        branch: String,
    },
    /// The worktree directory already exists.
    WorktreeExists {
        /// Conflicting path.
        path: PathBuf,
    },
    /// The path is not a Git worktree of the repository.
    WorktreeUnknown {
        /// Path that was requested.
        path: PathBuf,
    },
    /// The worktree has uncommitted changes or untracked files.
    WorktreeDirty {
        /// Worktree path.
        path: PathBuf,
    },
    /// A session is still using the worktree.
    WorktreeInUse {
        /// Worktree path.
        path: PathBuf,
    },
    /// Git reports the worktree as locked.
    WorktreeLocked {
        /// Worktree path.
        path: PathBuf,
        /// Optional lock reason from Git.
        reason: Option<String>,
    },
    /// The path is the repository's primary worktree.
    PrimaryWorktree {
        /// Worktree path.
        path: PathBuf,
    },
    /// The path is outside the managed worktree root.
    PathOutsideManagedRoot {
        /// Requested or resolved path.
        path: PathBuf,
        /// Configured managed root.
        root: PathBuf,
    },
    /// A symlink would redirect the operation.
    SymlinkRejected {
        /// Path that is a symlink.
        path: PathBuf,
    },
    /// Removal requires an explicit confirmation token.
    ConfirmationRequired,
    /// The confirmation token does not match current state.
    ConfirmationMismatch,
    /// The confirmation token is no longer valid.
    ConfirmationExpired,
    /// A generated name was empty or reserved after sanitization.
    NameInvalid {
        /// Original session name.
        session_name: String,
    },
    /// Git printed output that this crate cannot parse.
    InvalidOutput {
        /// Short description of what failed to parse.
        reason: String,
    },
    /// The Git process could not be spawned.
    SpawnFailed {
        /// Operating-system error text.
        message: String,
    },
    /// Git returned a non-zero exit status.
    CommandFailed {
        /// Git arguments, without interpolating a shell string.
        args: Vec<String>,
        /// Process exit code, when available.
        exit_code: Option<i32>,
        /// Truncated stderr from Git.
        stderr: String,
    },
    /// Worktree creation failed and compensating cleanup also failed.
    RollbackFailed {
        /// Original creation failure.
        source: Box<GitError>,
        /// Cleanup failure.
        rollback: Box<GitError>,
    },
    /// An internal invariant failed.
    Internal {
        /// Safe diagnostic message.
        message: String,
    },
}

impl GitError {
    /// Returns the stable machine-readable code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::ExecutableNotFound => code::EXECUTABLE_NOT_FOUND,
            Self::PathNotFound { .. } => code::PATH_NOT_FOUND,
            Self::NotADirectory { .. } => code::NOT_A_DIRECTORY,
            Self::NotARepository { .. } => code::NOT_A_REPOSITORY,
            Self::BareRepository { .. } => code::BARE_REPOSITORY,
            Self::UnbornHead { .. } => code::UNBORN_HEAD,
            Self::InvalidRef { .. } => code::INVALID_REF,
            Self::BranchExists { .. } => code::BRANCH_EXISTS,
            Self::WorktreeExists { .. } => code::WORKTREE_EXISTS,
            Self::WorktreeUnknown { .. } => code::WORKTREE_UNKNOWN,
            Self::WorktreeDirty { .. } => code::WORKTREE_DIRTY,
            Self::WorktreeInUse { .. } => code::WORKTREE_IN_USE,
            Self::WorktreeLocked { .. } => code::WORKTREE_LOCKED,
            Self::PrimaryWorktree { .. } => code::PRIMARY_WORKTREE,
            Self::PathOutsideManagedRoot { .. } => code::PATH_OUTSIDE_ROOT,
            Self::SymlinkRejected { .. } => code::SYMLINK_REJECTED,
            Self::ConfirmationRequired => code::CONFIRMATION_REQUIRED,
            Self::ConfirmationMismatch => code::CONFIRMATION_MISMATCH,
            Self::ConfirmationExpired => code::CONFIRMATION_EXPIRED,
            Self::NameInvalid { .. } => code::NAME_INVALID,
            Self::InvalidOutput { .. } => code::INVALID_OUTPUT,
            Self::SpawnFailed { .. } => code::SPAWN_FAILED,
            Self::CommandFailed { .. } => code::COMMAND_FAILED,
            Self::RollbackFailed { .. } => code::ROLLBACK_FAILED,
            Self::Internal { .. } => code::INTERNAL,
        }
    }

    /// Converts this error into an IPC-facing [`ApiError`].
    #[must_use]
    pub fn to_api_error(&self) -> ApiError {
        let error = ApiError::new(self.code(), self.to_string()).with_action(self.action());
        match self {
            Self::PathNotFound { path }
            | Self::NotADirectory { path }
            | Self::NotARepository { path }
            | Self::BareRepository { path }
            | Self::UnbornHead { path }
            | Self::WorktreeExists { path }
            | Self::WorktreeUnknown { path }
            | Self::WorktreeDirty { path }
            | Self::WorktreeInUse { path }
            | Self::PrimaryWorktree { path }
            | Self::SymlinkRejected { path } => {
                error.with_detail("path", path.display().to_string())
            }
            Self::WorktreeLocked { path, reason } => {
                let error = error.with_detail("path", path.display().to_string());
                match reason {
                    Some(reason) => error.with_detail("reason", reason.clone()),
                    None => error,
                }
            }
            Self::PathOutsideManagedRoot { path, root } => error
                .with_detail("path", path.display().to_string())
                .with_detail("root", root.display().to_string()),
            Self::InvalidRef { reference } => error.with_detail("ref", reference.clone()),
            Self::BranchExists { branch } => error.with_detail("branch", branch.clone()),
            Self::NameInvalid { session_name } => {
                error.with_detail("sessionName", session_name.clone())
            }
            Self::InvalidOutput { reason } => error.with_detail("reason", reason.clone()),
            Self::CommandFailed {
                args,
                exit_code,
                stderr,
            } => {
                let mut error = error.with_detail("args", args.join(" "));
                if let Some(exit_code) = exit_code {
                    error = error.with_detail("exitCode", i64::from(*exit_code));
                }
                if !stderr.is_empty() {
                    error = error.with_detail("stderr", stderr.clone());
                }
                error
            }
            Self::RollbackFailed { source, rollback } => error
                .with_detail("sourceCode", source.code())
                .with_detail("source", source.to_string())
                .with_detail("rollbackCode", rollback.code())
                .with_detail("rollback", rollback.to_string()),
            Self::SpawnFailed { message } | Self::Internal { message } => {
                error.with_detail("message", message.clone())
            }
            Self::ExecutableNotFound
            | Self::ConfirmationRequired
            | Self::ConfirmationMismatch
            | Self::ConfirmationExpired => error,
        }
    }

    fn action(&self) -> &'static str {
        match self {
            Self::ExecutableNotFound => {
                "Install Git and ensure the `git` executable is on PATH, then retry."
            }
            Self::PathNotFound { .. } => "Choose an existing directory and retry.",
            Self::NotADirectory { .. } => "Choose a directory, not a file, and retry.",
            Self::NotARepository { .. } => {
                "Open a Git repository, or initialize one with `git init`, then retry."
            }
            Self::BareRepository { .. } => {
                "Use a non-bare checkout of the repository as the project path."
            }
            Self::UnbornHead { .. } => {
                "Create the first commit in the repository, then retry worktree creation."
            }
            Self::InvalidRef { .. } => {
                "Choose an existing branch or commit from the repository and retry."
            }
            Self::BranchExists { .. } => {
                "Rename the session or allow unique branch allocation, then retry."
            }
            Self::WorktreeExists { .. } => "Remove or rename the existing directory, then retry.",
            Self::WorktreeUnknown { .. } => {
                "Refresh the worktree list and select a managed worktree."
            }
            Self::WorktreeDirty { .. } => {
                "Commit or move the changes, then prepare removal again. Dirty worktrees are never deleted automatically."
            }
            Self::WorktreeInUse { .. } => "Stop the session that uses this worktree, then retry.",
            Self::WorktreeLocked { .. } => {
                "Unlock the worktree in Git after inspecting why it was locked, then retry."
            }
            Self::PrimaryWorktree { .. } => {
                "Choose a session worktree created by CLI Master. The main checkout is never removed."
            }
            Self::PathOutsideManagedRoot { .. } => {
                "Use a worktree created by CLI Master under its managed directory."
            }
            Self::SymlinkRejected { .. } => {
                "Replace the symlink with a regular directory inside the managed worktree area."
            }
            Self::ConfirmationRequired => {
                "Call worktree prepare-remove from the UI and confirm the result before deleting."
            }
            Self::ConfirmationMismatch => {
                "The worktree changed after prepare-remove. Inspect it and confirm again."
            }
            Self::ConfirmationExpired => {
                "Prepare removal again and confirm while the token is valid."
            }
            Self::NameInvalid { .. } => {
                "Choose a session name with letters or numbers so a branch can be generated."
            }
            Self::InvalidOutput { .. } => {
                "Retry the operation. If it keeps failing, inspect Git's version and the repository."
            }
            Self::SpawnFailed { .. } => "Confirm Git is installed and executable, then retry.",
            Self::CommandFailed { .. } => {
                "Inspect the Git error details, fix the repository state, and retry."
            }
            Self::RollbackFailed { .. } => {
                "Inspect the leftover branch or directory listed in the error and remove them only after confirming they are unused."
            }
            Self::Internal { .. } => "Retry the operation. If it persists, restart the daemon.",
        }
    }
}

impl fmt::Display for GitError {
    #[allow(clippy::too_many_lines)]
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExecutableNotFound => formatter.write_str("Git executable was not found on PATH"),
            Self::PathNotFound { path } => {
                write!(formatter, "path does not exist: {}", path.display())
            }
            Self::NotADirectory { path } => {
                write!(formatter, "path is not a directory: {}", path.display())
            }
            Self::NotARepository { path } => {
                write!(
                    formatter,
                    "path is not a Git repository: {}",
                    path.display()
                )
            }
            Self::BareRepository { path } => {
                write!(
                    formatter,
                    "repository is bare and cannot be used as a project checkout: {}",
                    path.display()
                )
            }
            Self::UnbornHead { path } => write!(
                formatter,
                "repository has no commits yet: {}",
                path.display()
            ),
            Self::InvalidRef { reference } => {
                write!(formatter, "ref does not resolve to a commit: {reference}")
            }
            Self::BranchExists { branch } => write!(formatter, "branch already exists: {branch}"),
            Self::WorktreeExists { path } => {
                write!(
                    formatter,
                    "worktree path already exists: {}",
                    path.display()
                )
            }
            Self::WorktreeUnknown { path } => write!(
                formatter,
                "path is not a known Git worktree: {}",
                path.display()
            ),
            Self::WorktreeDirty { path } => write!(
                formatter,
                "worktree has uncommitted changes: {}",
                path.display()
            ),
            Self::WorktreeInUse { path } => {
                write!(
                    formatter,
                    "worktree is in use by a session: {}",
                    path.display()
                )
            }
            Self::WorktreeLocked { path, reason } => match reason {
                Some(reason) => write!(
                    formatter,
                    "worktree is locked ({}): {}",
                    reason,
                    path.display()
                ),
                None => write!(formatter, "worktree is locked: {}", path.display()),
            },
            Self::PrimaryWorktree { path } => write!(
                formatter,
                "refusing to remove the primary repository worktree: {}",
                path.display()
            ),
            Self::PathOutsideManagedRoot { path, root } => write!(
                formatter,
                "path {} is outside the managed worktree root {}",
                path.display(),
                root.display()
            ),
            Self::SymlinkRejected { path } => {
                write!(formatter, "refusing to follow symlink: {}", path.display())
            }
            Self::ConfirmationRequired => {
                formatter.write_str("worktree removal requires an explicit confirmation token")
            }
            Self::ConfirmationMismatch => {
                formatter.write_str("worktree confirmation token does not match current state")
            }
            Self::ConfirmationExpired => {
                formatter.write_str("worktree confirmation token has expired")
            }
            Self::NameInvalid { session_name } => write!(
                formatter,
                "session name {session_name:?} does not produce a safe Git branch"
            ),
            Self::InvalidOutput { reason } => write!(formatter, "invalid Git output: {reason}"),
            Self::SpawnFailed { message } => write!(formatter, "failed to start Git: {message}"),
            Self::CommandFailed {
                args,
                exit_code,
                stderr,
            } => {
                write!(formatter, "git")?;
                for arg in args {
                    write!(formatter, " {arg}")?;
                }
                match exit_code {
                    Some(code) => write!(formatter, " failed with status {code}")?,
                    None => write!(formatter, " failed")?,
                }
                if !stderr.is_empty() {
                    write!(formatter, ": {stderr}")?;
                }
                Ok(())
            }
            Self::RollbackFailed { source, rollback } => write!(
                formatter,
                "worktree creation failed ({source}) and rollback failed ({rollback})"
            ),
            Self::Internal { message } => {
                write!(formatter, "internal Git service error: {message}")
            }
        }
    }
}

impl Error for GitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::RollbackFailed { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}
