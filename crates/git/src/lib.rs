//! Safe, bounded access to the system Git executable.
//!
//! This crate invokes Git directly with structured argument lists. It never
//! starts a shell and deliberately exposes no reset, branch deletion, or forced
//! worktree removal operation.

#![warn(missing_docs)]

mod command;
mod diff;
mod error;
mod naming;
mod repository;
mod status;
mod worktree;

pub use diff::Diff;
pub use error::{GitError, GitErrorKind};
pub use naming::slugify;
pub use repository::RepositoryInspection;
pub use status::{ChangeKind, ChangedFile, RepositoryStatus, StatusCounts};
pub use worktree::{RemovalBlocker, RemovalPreparation, WorktreeInfo, WorktreeUse};

use std::{
    env,
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    time::Duration,
};

use command::{CommandOutput, run};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);
const DEFAULT_OUTPUT_LIMIT: usize = 8 * 1024 * 1024;

/// A validated system Git executable with bounded command execution.
#[derive(Clone, Debug)]
pub struct Git {
    executable: PathBuf,
    timeout: Duration,
}

impl Git {
    /// Locates Git on `PATH` and verifies that it can execute.
    ///
    /// # Errors
    ///
    /// Returns an actionable error when `PATH` is missing, no executable Git is
    /// found, or the discovered executable cannot report its version.
    pub fn discover() -> Result<Self, GitError> {
        let executable = find_on_path(OsStr::new("git")).ok_or_else(|| {
            GitError::new(
                GitErrorKind::NotFound,
                "Git was not found on PATH",
                "Install Git and restart CLI Master so the desktop process inherits the updated PATH",
            )
        })?;
        Self::with_executable(executable)
    }

    /// Validates an explicit Git executable path.
    ///
    /// # Errors
    ///
    /// Returns an error when the path is not a file or does not successfully
    /// execute `git --version` within the default timeout.
    pub fn with_executable(executable: impl Into<PathBuf>) -> Result<Self, GitError> {
        let executable = executable.into();
        if !executable.is_file() {
            return Err(GitError::new(
                GitErrorKind::NotFound,
                format!("Git executable does not exist: {}", executable.display()),
                "Choose an existing Git executable",
            ));
        }
        let git = Self {
            executable,
            timeout: DEFAULT_TIMEOUT,
        };
        let output = git.execute(None, [OsStr::new("--version")], 16 * 1024)?;
        if !output.success() {
            return Err(git.command_error("validate Git", &output));
        }
        if output.stdout_truncated {
            return Err(GitError::new(
                GitErrorKind::InvalidOutput,
                "The selected executable returned an unexpectedly large version response",
                "Choose the system Git executable",
            ));
        }
        if !String::from_utf8_lossy(&output.stdout).starts_with("git version ") {
            return Err(GitError::new(
                GitErrorKind::InvalidOutput,
                "The selected executable did not identify itself as Git",
                "Choose the system Git executable",
            ));
        }
        Ok(git)
    }

    /// Overrides the command timeout for this handle.
    ///
    /// A zero timeout is rejected because it would make every invocation race
    /// process startup.
    ///
    /// # Errors
    ///
    /// Returns an error when `timeout` is zero.
    pub fn with_timeout(mut self, timeout: Duration) -> Result<Self, GitError> {
        if timeout.is_zero() {
            return Err(GitError::new(
                GitErrorKind::InvalidInput,
                "Git command timeout must be greater than zero",
                "Use a positive timeout",
            ));
        }
        self.timeout = timeout;
        Ok(self)
    }

    /// Returns the validated executable path.
    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// Inspects a directory without requiring it to be a Git repository.
    ///
    /// # Errors
    ///
    /// Returns an error if the path does not exist, is not a directory, cannot
    /// be canonicalized, or Git cannot inspect it for a reason other than it not
    /// being a repository.
    pub fn inspect_repository(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<RepositoryInspection, GitError> {
        repository::inspect(self, path.as_ref())
    }

    /// Generates an ASCII `agent/<slug>-<shortid>` branch name.
    ///
    /// Existing local branches are avoided with deterministic numeric suffixes.
    ///
    /// # Errors
    ///
    /// Returns an error if `repository` is not a repository or Git cannot query
    /// its refs.
    pub fn generate_branch_name(
        &self,
        repository: impl AsRef<Path>,
        task_name: &str,
        short_id: &str,
    ) -> Result<String, GitError> {
        naming::generate_branch_name(self, repository.as_ref(), task_name, short_id)
    }

    /// Reads branch metadata and changed paths using porcelain v2 `-z` output.
    ///
    /// # Errors
    ///
    /// Returns an error when `path` is not in a repository, Git times out, or
    /// the porcelain response is malformed.
    pub fn status(&self, path: impl AsRef<Path>) -> Result<RepositoryStatus, GitError> {
        status::read(self, path.as_ref())
    }

    /// Returns the combined staged and unstaged tracked-file diff against `HEAD`.
    ///
    /// Output is always generated without color or external diff drivers and is
    /// capped at `max_bytes`. The returned [`Diff::truncated`] flag reports when
    /// bytes were omitted.
    ///
    /// # Errors
    ///
    /// Returns an error for a zero limit, invalid repository, timeout, or failed
    /// Git invocation.
    pub fn diff(&self, path: impl AsRef<Path>, max_bytes: usize) -> Result<Diff, GitError> {
        diff::read(self, path.as_ref(), max_bytes)
    }

    /// Lists all worktrees registered by a repository.
    ///
    /// # Errors
    ///
    /// Returns an error when Git cannot list or parse worktree metadata.
    pub fn list_worktrees(
        &self,
        repository: impl AsRef<Path>,
    ) -> Result<Vec<WorktreeInfo>, GitError> {
        worktree::list(self, repository.as_ref())
    }

    /// Creates a branch and linked worktree below a managed root.
    ///
    /// Both branch and directory names use collision-safe ASCII names. The
    /// destination is validated to remain below `managed_root`.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid repository/root, unsafe destination, or
    /// failed Git worktree creation.
    pub fn create_worktree(
        &self,
        repository: impl AsRef<Path>,
        managed_root: impl AsRef<Path>,
        task_name: &str,
        short_id: &str,
    ) -> Result<WorktreeInfo, GitError> {
        worktree::create(
            self,
            repository.as_ref(),
            managed_root.as_ref(),
            task_name,
            short_id,
        )
    }

    /// Checks whether a managed worktree can be removed safely.
    ///
    /// The result distinguishes staged, tracked, and untracked dirtiness and
    /// combines that state with caller-supplied runtime usage information.
    ///
    /// # Errors
    ///
    /// Returns an error if paths are invalid, the target is outside the managed
    /// root, or Git cannot inspect the target.
    pub fn prepare_remove(
        &self,
        repository: impl AsRef<Path>,
        managed_root: impl AsRef<Path>,
        worktree_path: impl AsRef<Path>,
        usage: WorktreeUse,
    ) -> Result<RemovalPreparation, GitError> {
        worktree::prepare_remove(
            self,
            repository.as_ref(),
            managed_root.as_ref(),
            worktree_path.as_ref(),
            usage,
        )
    }

    /// Removes a clean, unused managed worktree without using `--force`.
    ///
    /// This method never deletes a branch. Dirty, running, or otherwise in-use
    /// worktrees are rejected. Safety state is rechecked immediately before
    /// Git is invoked, and `--force` is omitted so Git still rejects a change
    /// that races with that final check.
    ///
    /// # Errors
    ///
    /// Returns an error if safety checks fail or `git worktree remove` fails.
    pub fn remove_worktree(
        &self,
        repository: impl AsRef<Path>,
        managed_root: impl AsRef<Path>,
        worktree_path: impl AsRef<Path>,
        usage: WorktreeUse,
    ) -> Result<(), GitError> {
        worktree::remove(
            self,
            repository.as_ref(),
            managed_root.as_ref(),
            worktree_path.as_ref(),
            usage,
        )
    }

    pub(crate) fn execute<I, S>(
        &self,
        cwd: Option<&Path>,
        args: I,
        max_stdout: usize,
    ) -> Result<CommandOutput, GitError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args = args
            .into_iter()
            .map(|arg| arg.as_ref().to_os_string())
            .collect();
        run(&self.executable, cwd, args, self.timeout, max_stdout)
    }

    pub(crate) fn checked<I, S>(
        &self,
        cwd: Option<&Path>,
        args: I,
        operation: &'static str,
    ) -> Result<CommandOutput, GitError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let output = self.execute(cwd, args, DEFAULT_OUTPUT_LIMIT)?;
        if !output.success() {
            return Err(self.command_error(operation, &output));
        }
        if output.stdout_truncated {
            return Err(GitError::new(
                GitErrorKind::InvalidOutput,
                format!("Git returned too much data while attempting to {operation}"),
                "Reduce the repository's changed-file count or stale worktree entries and try again",
            ));
        }
        Ok(output)
    }

    pub(crate) fn command_error(
        &self,
        operation: &'static str,
        output: &CommandOutput,
    ) -> GitError {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let message = if stderr.is_empty() {
            format!("Git could not {operation} (exit status {})", output.status)
        } else {
            format!("Git could not {operation}: {stderr}")
        };
        GitError::new(
            GitErrorKind::CommandFailed,
            message,
            "Resolve the Git error and try again",
        )
        .with_path(&self.executable)
        .with_exit_status(output.status.code())
    }
}

fn find_on_path(name: &OsStr) -> Option<PathBuf> {
    let candidate = Path::new(name);
    if candidate.components().count() > 1 {
        return candidate.is_file().then(|| candidate.to_path_buf());
    }
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

pub(crate) fn os(value: impl Into<OsString>) -> OsString {
    value.into()
}
