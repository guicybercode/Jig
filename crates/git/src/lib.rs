//! Safe Git repository detection, status, diff, and worktree isolation.
//!
//! Every Git invocation uses a resolved executable plus a separate argument
//! array. This crate never interpolates a shell string, never runs
//! `git reset --hard` or `git clean`, and never deletes a dirty worktree.

#![warn(missing_docs)]

mod command;
mod detect;
mod diff;
mod error;
mod naming;
mod paths;
mod status;
mod worktree;

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::command::resolve_git_executable;

pub use detect::{BranchState, RepositoryInfo, RepositoryKind};
pub use diff::{DiffOptions, DiffScope, GitDiff};
pub use error::{GitError, code};
pub use naming::{AllocatedNames, allocate_names, session_suffix, slugify};
pub use status::{ChangeKind, GitStatus, StatusEntry};
pub use worktree::{
    CreateWorktreeRequest, CreatedWorktree, ExistingBranchBehavior, InspectOptions,
    PrepareRemoveRequest, PrepareRemoveResult, RemoveBlocker, RemoveScope, RemoveWorktreeRequest,
    RemovedWorktree, WorktreeInfo,
};

/// Typed Git operations for CLI Master. Frontend code must not invoke Git.
pub struct GitService {
    git_executable: PathBuf,
    managed_worktree_root: PathBuf,
    removals: worktree::RemovalTokens,
}

impl std::fmt::Debug for GitService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GitService")
            .field("git_executable", &self.git_executable)
            .field("managed_worktree_root", &self.managed_worktree_root)
            .finish_non_exhaustive()
    }
}

impl GitService {
    /// Resolves `git` from `PATH` and stores worktrees under `managed_worktree_root`.
    ///
    /// # Errors
    ///
    /// Returns [`GitError::ExecutableNotFound`] when Git is missing, or an I/O
    /// error if the managed root cannot be created.
    pub fn new(managed_worktree_root: impl Into<PathBuf>) -> Result<Self, GitError> {
        Self::with_executable(resolve_git_executable()?, managed_worktree_root)
    }

    /// Builds a service around an already resolved Git executable.
    ///
    /// # Errors
    ///
    /// Returns an error if the executable is not usable or the managed root
    /// cannot be created.
    pub fn with_executable(
        git_executable: impl Into<PathBuf>,
        managed_worktree_root: impl Into<PathBuf>,
    ) -> Result<Self, GitError> {
        let git_executable = git_executable.into();
        if !command::is_executable_file(&git_executable) {
            return Err(GitError::ExecutableNotFound);
        }
        let managed_worktree_root = paths::normalize_absolute(&managed_worktree_root.into())?;
        fs::create_dir_all(&managed_worktree_root).map_err(|error| GitError::SpawnFailed {
            message: format!(
                "could not create managed worktree root {}: {error}",
                managed_worktree_root.display()
            ),
        })?;
        Ok(Self {
            git_executable,
            managed_worktree_root,
            removals: worktree::RemovalTokens::new(),
        })
    }

    /// Returns the resolved Git executable.
    #[must_use]
    pub fn git_executable(&self) -> &Path {
        &self.git_executable
    }

    /// Returns the directory that must contain every created worktree.
    #[must_use]
    pub fn managed_worktree_root(&self) -> &Path {
        &self.managed_worktree_root
    }

    /// Detects whether `path` is a repository root, subdirectory, or worktree.
    ///
    /// # Errors
    ///
    /// Returns a typed error for a missing path, a non-Git directory, or a bare
    /// repository.
    pub fn detect_repository(&self, path: impl AsRef<Path>) -> Result<RepositoryInfo, GitError> {
        detect::detect_repository(&self.git_executable, path.as_ref())
    }

    /// Returns the worktree root that owns `path`.
    ///
    /// # Errors
    ///
    /// Returns the same class of errors as [`Self::detect_repository`].
    pub fn get_repository_root(&self, path: impl AsRef<Path>) -> Result<PathBuf, GitError> {
        Ok(self.detect_repository(path)?.root)
    }

    /// Returns the current branch, detached HEAD, or unborn branch at `path`.
    ///
    /// # Errors
    ///
    /// Returns a typed error when Git cannot describe `HEAD`.
    pub fn current_branch(&self, path: impl AsRef<Path>) -> Result<BranchState, GitError> {
        detect::current_branch(&self.git_executable, path.as_ref())
    }

    /// Returns porcelain v2 status for `path`.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the path is not a worktree or Git output
    /// cannot be parsed.
    pub fn status(&self, path: impl AsRef<Path>) -> Result<GitStatus, GitError> {
        status::status(&self.git_executable, path.as_ref())
    }

    /// Returns a size-capped textual diff for `path`.
    ///
    /// # Errors
    ///
    /// Returns a typed error when Git fails. Invalid UTF-8 is reported in
    /// [`GitDiff::invalid_output`] rather than panicking.
    pub fn diff(&self, path: impl AsRef<Path>, options: DiffOptions) -> Result<GitDiff, GitError> {
        diff::diff(&self.git_executable, path.as_ref(), &options)
    }

    /// Lists Git worktrees for the repository that owns `repository`.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the path is not a repository or porcelain
    /// output cannot be parsed.
    pub fn list_worktrees(
        &self,
        repository: impl AsRef<Path>,
    ) -> Result<Vec<WorktreeInfo>, GitError> {
        worktree::list_worktrees(&self.git_executable, repository.as_ref())
    }

    /// Creates an isolated branch and worktree for a session.
    ///
    /// Persistence is the caller's job. This method only talks to Git and the
    /// filesystem, and only after Git confirms the worktree.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the repository, base ref, generated names,
    /// or `git worktree add` fail. A partial failure attempts compensating
    /// cleanup.
    #[allow(clippy::needless_pass_by_value)]
    pub fn create_worktree(
        &self,
        request: CreateWorktreeRequest,
    ) -> Result<CreatedWorktree, GitError> {
        worktree::create_worktree(&self.git_executable, &self.managed_worktree_root, &request)
    }

    /// Inspects one worktree, optionally including dirty state.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the path is not a known worktree.
    pub fn inspect_worktree(
        &self,
        path: impl AsRef<Path>,
        options: InspectOptions,
    ) -> Result<WorktreeInfo, GitError> {
        worktree::inspect_worktree(&self.git_executable, path.as_ref(), options)
    }

    /// Inspects a worktree and, if removal is allowed, issues a confirmation token.
    ///
    /// Dirty directory removal never receives a token. There is no hidden force
    /// flag.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the path cannot be inspected.
    #[allow(clippy::needless_pass_by_value)]
    pub fn prepare_remove_worktree(
        &self,
        request: PrepareRemoveRequest,
    ) -> Result<PrepareRemoveResult, GitError> {
        worktree::prepare_remove(
            &self.git_executable,
            &self.managed_worktree_root,
            &self.removals,
            &request,
        )
    }

    /// Removes a worktree after a matching prepare-remove token.
    ///
    /// [`RemoveScope::MetadataOnly`] does not delete Git files. Directory
    /// removal uses `git worktree remove` without `--force`.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the token is missing, expired, or the
    /// worktree state changed, including when it became dirty.
    #[allow(clippy::needless_pass_by_value)]
    pub fn remove_worktree(
        &self,
        request: RemoveWorktreeRequest,
    ) -> Result<RemovedWorktree, GitError> {
        worktree::remove_worktree(
            &self.git_executable,
            &self.managed_worktree_root,
            &self.removals,
            &request,
        )
    }
}
