//! Git inspection handlers for registered daemon targets.

use std::path::PathBuf;

use cli_master_core::wire::{
    GitChangeKind, GitChangedFile, GitDiffRequest, GitDiffResponse, GitStatusCounts,
    GitStatusRequest, GitStatusResponse, GitTarget,
};
use cli_master_core::{ApiError, ApplicationError};
use cli_master_git::{
    ChangeKind, ChangedFile, Diff, Git, GitError, GitErrorKind, RepositoryStatus, display_path,
};
use cli_master_storage::{Storage, StorageError};

use tracing::warn;

use crate::MAX_GIT_DIFF_BYTES;

pub(crate) fn discover_git() -> Option<Git> {
    if let Ok(git) = Git::discover() {
        Some(git)
    } else {
        warn!("Git executable is unavailable; git inspection methods will fail");
        None
    }
}

pub(crate) async fn status(
    storage: &Storage,
    git: Option<&Git>,
    request: GitStatusRequest,
) -> Result<GitStatusResponse, ApiError> {
    let cwd = resolve_target(storage, request.target)?;
    let git = require_git(git)?;
    let status = spawn_git(move || git.status(&cwd)).await?;
    Ok(wire_status(&status))
}

pub(crate) async fn diff(
    storage: &Storage,
    git: Option<&Git>,
    request: GitDiffRequest,
) -> Result<GitDiffResponse, ApiError> {
    let cwd = resolve_target(storage, request.target)?;
    let git = require_git(git)?;
    let pathspec = request.path.map(|path| path.as_path().to_path_buf());
    let diff = spawn_git(move || match pathspec {
        Some(path) => git.diff_path(&cwd, &path, MAX_GIT_DIFF_BYTES),
        None => git.diff(&cwd, MAX_GIT_DIFF_BYTES),
    })
    .await?;
    Ok(wire_diff(diff))
}

fn resolve_target(storage: &Storage, target: GitTarget) -> Result<PathBuf, ApiError> {
    let path = match target {
        GitTarget::Project { project_id } => storage
            .get_project(project_id)
            .map_err(|error| map_storage_error(&error))?
            .map(|project| project.path),
        GitTarget::Session { session_id } => storage
            .get_session(session_id)
            .map_err(|error| map_storage_error(&error))?
            .map(|session| session.cwd),
        GitTarget::Worktree { worktree_id } => storage
            .get_worktree(worktree_id)
            .map_err(|error| map_storage_error(&error))?
            .map(|worktree| worktree.path),
    };
    path.ok_or_else(|| unregistered_target(&target))
}

fn unregistered_target(target: &GitTarget) -> ApiError {
    let kind = match target {
        GitTarget::Project { .. } => "project",
        GitTarget::Session { .. } => "session",
        GitTarget::Worktree { .. } => "worktree",
    };
    ApiError::new(
        "unregistered_git_target",
        "The requested Git target is not registered with this daemon",
    )
    .with_action("Register the project or worktree before inspecting Git state")
    .with_detail("targetKind", kind)
}

fn require_git(git: Option<&Git>) -> Result<Git, ApiError> {
    git.cloned().ok_or_else(|| {
        ApiError::new("git_not_found", "Git is not available to this daemon")
            .with_action("Install Git and restart CLI Master so the daemon inherits PATH")
    })
}

async fn spawn_git<T, F>(work: F) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, GitError> + Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| {
            record_application_error(
                ApplicationError::new(
                    "internal_error",
                    "Git inspection failed",
                    "The daemon could not complete a Git inspection",
                )
                .with_technical("the blocking Git task did not complete")
                .with_source(&error)
                .not_recoverable(),
            )
        })?
        .map_err(|error| map_git_error(&error))
}

fn map_git_error(error: &GitError) -> ApiError {
    let application = match error.kind() {
        GitErrorKind::NotFound => ApplicationError::new(
            "git_not_found",
            "Git unavailable",
            "Git could not inspect the registered target",
        )
        .with_action("Confirm Git is installed and the registered directory still exists"),
        GitErrorKind::InvalidInput | GitErrorKind::UnsafePath => ApplicationError::new(
            "invalid_git_path",
            "Invalid Git path",
            "The requested Git path is not a safe repository-relative pathspec",
        )
        .with_action("Choose a file path reported by git.status"),
        GitErrorKind::NotRepository => ApplicationError::new(
            "not_repository",
            "Not a Git repository",
            "The registered target is not inside a Git repository",
        )
        .with_action("Register a Git repository or initialize one in the project directory"),
        GitErrorKind::Timeout => ApplicationError::new(
            "git_timeout",
            "Git inspection timed out",
            "Git inspection exceeded the daemon time limit",
        )
        .with_action("Retry the inspection; if it keeps timing out, check repository locks"),
        GitErrorKind::CommandFailed
        | GitErrorKind::InvalidOutput
        | GitErrorKind::Io
        | GitErrorKind::DirtyWorktree
        | GitErrorKind::WorktreeInUse
        | GitErrorKind::PartialWorktree => ApplicationError::new(
            "git_inspection_failed",
            "Git inspection failed",
            "Git could not complete the requested inspection",
        )
        .with_action("Retry the inspection and verify the repository with the Git command line"),
    }
    .with_technical(error.to_string())
    .with_source(error);
    record_application_error(application)
}

fn map_storage_error(error: &StorageError) -> ApiError {
    record_application_error(
        ApplicationError::new(
            "storage_unavailable",
            "Storage unavailable",
            "The daemon could not look up the registered Git target",
        )
        .with_action("Restart the daemon and try again")
        .with_technical(error.to_string())
        .with_source(&error),
    )
}

fn record_application_error(error: ApplicationError) -> ApiError {
    warn!(
        error_code = error.code(),
        technical = error.technical_message(),
        source_chain = error.source_chain(),
        "application error"
    );
    error.into()
}

fn wire_status(status: &RepositoryStatus) -> GitStatusResponse {
    GitStatusResponse {
        branch: status.branch.clone(),
        files: status.files.iter().map(wire_file).collect(),
        counts: GitStatusCounts {
            modified: count(status.counts.modified),
            added: count(status.counts.added),
            deleted: count(status.counts.deleted),
            untracked: count(status.counts.untracked),
            renamed: count(status.counts.renamed),
            ignored: count(status.counts.ignored),
        },
        has_staged: status.has_staged,
        has_tracked_changes: status.has_tracked_changes,
        has_untracked: status.has_untracked,
        is_dirty: status.is_dirty(),
    }
}

fn wire_file(file: &ChangedFile) -> GitChangedFile {
    GitChangedFile {
        path: display_path(&file.path),
        original_path: file.original_path.as_deref().map(display_path),
        kind: wire_kind(file.kind),
        staged: file.staged,
        unstaged: file.unstaged,
    }
}

const fn wire_kind(kind: ChangeKind) -> GitChangeKind {
    match kind {
        ChangeKind::Modified => GitChangeKind::Modified,
        ChangeKind::Added => GitChangeKind::Added,
        ChangeKind::Deleted => GitChangeKind::Deleted,
        ChangeKind::Untracked => GitChangeKind::Untracked,
        ChangeKind::Renamed => GitChangeKind::Renamed,
        ChangeKind::Ignored => GitChangeKind::Ignored,
    }
}

fn wire_diff(diff: Diff) -> GitDiffResponse {
    GitDiffResponse {
        text: diff.text,
        truncated: diff.truncated,
        binary: diff.binary,
    }
}

fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
