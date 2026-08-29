use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use crate::{
    Git, GitError, GitErrorKind, RepositoryStatus, os, path_safety, repository, status, worktree,
    worktree::{WorktreeInfo, WorktreeUse},
};

/// A conservative reason why a managed worktree cannot be removed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemovalBlocker {
    /// The index differs from `HEAD`.
    StagedChanges,
    /// Tracked working-tree content differs from the index.
    TrackedChanges,
    /// Ordinary untracked files are present.
    UntrackedFiles,
    /// Ignored files are present and would otherwise be silently deleted.
    IgnoredFiles,
    /// At least one index entry is marked `assume-unchanged`.
    AssumeUnchanged,
    /// At least one index entry is marked `skip-worktree`, including sparse entries.
    SkipWorktree,
    /// Git has locked the worktree.
    Locked,
    /// An agent process is currently running in the worktree.
    Running,
    /// Another session or live operation currently claims the worktree.
    InUse,
}

/// Result of the non-destructive worktree removal safety check.
///
/// Equality is an exact state-bound comparison: repository and managed roots,
/// registered worktree identity, content state, hidden index protections,
/// runtime usage, blockers, and the final decision must all match.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemovalPreparation {
    /// Canonical primary repository root used to obtain this snapshot.
    pub repository_root: PathBuf,
    /// Canonical managed root that bounded this snapshot.
    pub managed_root: PathBuf,
    /// Exact registered identity, including path, `HEAD`, branch, and lock state.
    pub worktree: WorktreeInfo,
    /// Current staged, tracked, and untracked state.
    pub status: RepositoryStatus,
    /// Ignored repository-relative paths that removal could silently delete.
    pub ignored_paths: Vec<PathBuf>,
    /// Paths whose index entries are marked `assume-unchanged`.
    pub assume_unchanged_paths: Vec<PathBuf>,
    /// Paths whose index entries are marked `skip-worktree`.
    pub skip_worktree_paths: Vec<PathBuf>,
    /// Whether an agent is currently running in the worktree.
    pub running: bool,
    /// Whether another live object currently claims the worktree.
    pub in_use: bool,
    /// Every currently observed reason that prevents safe removal.
    pub blockers: Vec<RemovalBlocker>,
    /// Whether default, non-forced removal is currently safe.
    pub can_remove: bool,
}

pub(crate) fn prepare_remove(
    git: &Git,
    repository: &Path,
    managed_root: &Path,
    worktree_path: &Path,
    usage: WorktreeUse,
) -> Result<RemovalPreparation, GitError> {
    let root = repository::require_root(git, repository)?;
    let managed_root =
        path_safety::canonical_existing_directory(managed_root, "managed worktree root")?;
    let target = path_safety::canonical_existing_directory(worktree_path, "worktree")?;
    path_safety::validate_descendant(&managed_root, &target, true)?;
    if target == root {
        return Err(GitError::new(
            GitErrorKind::UnsafePath,
            "The primary repository checkout cannot be removed as a managed worktree",
            "Select a linked worktree below the managed worktree root",
        )
        .with_path(target));
    }
    let registered = worktree::list(git, &root)?
        .into_iter()
        .find(|worktree| path_safety::paths_match(&worktree.path, &target));
    let Some(mut registered) = registered else {
        return Err(GitError::new(
            GitErrorKind::InvalidInput,
            format!(
                "Git does not register this path as a worktree: {}",
                target.display()
            ),
            "Refresh worktrees and select a registered managed worktree",
        )
        .with_path(target));
    };
    registered.path.clone_from(&target);
    let (status, ignored_paths) = status::read_for_removal(git, &target)?;
    let (assume_unchanged_paths, skip_worktree_paths) = read_index_flags(git, &target)?;
    let blockers = collect_blockers(
        &status,
        &ignored_paths,
        &assume_unchanged_paths,
        &skip_worktree_paths,
        &registered,
        usage,
    );
    let can_remove = blockers.is_empty();
    Ok(RemovalPreparation {
        repository_root: root,
        managed_root,
        worktree: registered,
        status,
        ignored_paths,
        assume_unchanged_paths,
        skip_worktree_paths,
        running: usage.running,
        in_use: usage.in_use,
        blockers,
        can_remove,
    })
}

pub(crate) fn remove<F>(
    git: &Git,
    repository: &Path,
    managed_root: &Path,
    worktree_path: &Path,
    mut read_usage: F,
) -> Result<(), GitError>
where
    F: FnMut() -> WorktreeUse,
{
    let preparation = prepare_remove(git, repository, managed_root, worktree_path, read_usage())?;
    ensure_removable(&preparation, worktree_path)?;
    let target = preparation.worktree.path.clone();

    let final_preparation = prepare_remove(git, repository, managed_root, &target, read_usage())?;
    ensure_removable(&final_preparation, &target)?;
    ensure_snapshot_unchanged(&preparation, &final_preparation, &target)?;
    git.checked(
        Some(&final_preparation.repository_root),
        [
            os("worktree"),
            os("remove"),
            target.as_os_str().to_os_string(),
        ],
        "remove the worktree",
    )?;
    Ok(())
}

pub(crate) fn ensure_snapshot_unchanged(
    expected: &RemovalPreparation,
    actual: &RemovalPreparation,
    worktree_path: &Path,
) -> Result<(), GitError> {
    if expected == actual {
        return Ok(());
    }
    Err(GitError::new(
        GitErrorKind::WorktreeInUse,
        "Worktree state changed while removal safety was being checked",
        "Refresh the worktree state and retry removal only if the new snapshot is still safe",
    )
    .with_path(worktree_path))
}

pub(crate) fn ensure_removable(
    preparation: &RemovalPreparation,
    worktree_path: &Path,
) -> Result<(), GitError> {
    if preparation.can_remove {
        return Ok(());
    }
    let hidden_or_dirty = preparation.status.is_dirty()
        || !preparation.ignored_paths.is_empty()
        || !preparation.assume_unchanged_paths.is_empty()
        || !preparation.skip_worktree_paths.is_empty();
    if hidden_or_dirty {
        return Err(GitError::new(
            GitErrorKind::DirtyWorktree,
            format!(
                "Worktree removal is blocked by repository content or index flags: {:?}",
                preparation.blockers
            ),
            "Commit or stash changes, move ignored files, and clear assume-unchanged/skip-worktree flags before removing the worktree",
        )
        .with_path(worktree_path));
    }
    Err(GitError::new(
        GitErrorKind::WorktreeInUse,
        format!(
            "Worktree removal is blocked by runtime or lock state: {:?}",
            preparation.blockers
        ),
        "Unlock the worktree, stop its session, and close active operations before trying again",
    )
    .with_path(worktree_path))
}

fn collect_blockers(
    status: &RepositoryStatus,
    ignored_paths: &[PathBuf],
    assume_unchanged_paths: &[PathBuf],
    skip_worktree_paths: &[PathBuf],
    registered: &WorktreeInfo,
    usage: WorktreeUse,
) -> Vec<RemovalBlocker> {
    let mut blockers = Vec::new();
    if status.has_staged {
        blockers.push(RemovalBlocker::StagedChanges);
    }
    if status.has_tracked_changes {
        blockers.push(RemovalBlocker::TrackedChanges);
    }
    if status.has_untracked {
        blockers.push(RemovalBlocker::UntrackedFiles);
    }
    if !ignored_paths.is_empty() {
        blockers.push(RemovalBlocker::IgnoredFiles);
    }
    if !assume_unchanged_paths.is_empty() {
        blockers.push(RemovalBlocker::AssumeUnchanged);
    }
    if !skip_worktree_paths.is_empty() {
        blockers.push(RemovalBlocker::SkipWorktree);
    }
    if registered.locked {
        blockers.push(RemovalBlocker::Locked);
    }
    if usage.running {
        blockers.push(RemovalBlocker::Running);
    }
    if usage.in_use {
        blockers.push(RemovalBlocker::InUse);
    }
    blockers
}

fn read_index_flags(git: &Git, worktree: &Path) -> Result<(Vec<PathBuf>, Vec<PathBuf>), GitError> {
    let output = git.checked(
        Some(worktree),
        [os("ls-files"), os("-v"), os("-z")],
        "inspect worktree index flags",
    )?;
    let mut assume_unchanged = Vec::new();
    let mut skip_worktree = Vec::new();
    for record in output.stdout.split(|byte| *byte == 0) {
        if record.is_empty() {
            continue;
        }
        if record.len() < 3 || record[1] != b' ' {
            return Err(worktree::invalid_worktree_output(
                "git ls-files returned malformed index flags",
            ));
        }
        let tag = record[0];
        let path = bytes_to_path(&record[2..]);
        if tag.is_ascii_lowercase() {
            assume_unchanged.push(path.clone());
        }
        if matches!(tag, b'S' | b's') {
            skip_worktree.push(path);
        }
    }
    Ok((assume_unchanged, skip_worktree))
}

fn bytes_to_path(bytes: &[u8]) -> PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        PathBuf::from(OsString::from_vec(bytes.to_vec()))
    }
    #[cfg(not(unix))]
    {
        PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
    }
}
