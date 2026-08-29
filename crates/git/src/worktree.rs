use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use crate::{Git, GitError, GitErrorKind, RepositoryStatus, naming, os, repository, status};

/// Caller-observed runtime usage that Git cannot determine itself.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorktreeUse {
    /// An agent process is currently running with this worktree as its cwd.
    pub running: bool,
    /// Session metadata or another live operation currently claims the worktree.
    pub in_use: bool,
}

/// One entry from Git's worktree registry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorktreeInfo {
    /// Canonical or absolute worktree path reported by Git.
    pub path: PathBuf,
    /// Commit checked out in the worktree.
    pub head: Option<String>,
    /// Local branch without the `refs/heads/` prefix.
    pub branch: Option<String>,
    /// Whether the worktree has a detached `HEAD`.
    pub detached: bool,
    /// Whether Git has marked the worktree as locked.
    pub locked: bool,
    /// Whether Git considers the worktree entry prunable.
    pub prunable: bool,
}

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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemovalPreparation {
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

pub(crate) fn list(git: &Git, repository: &Path) -> Result<Vec<WorktreeInfo>, GitError> {
    let root = repository::require_root(git, repository)?;
    let output = git.checked(
        Some(&root),
        [os("worktree"), os("list"), os("--porcelain"), os("-z")],
        "list worktrees",
    )?;
    parse_list(&output.stdout)
}

pub(crate) fn create(
    git: &Git,
    repository: &Path,
    managed_root: &Path,
    task_name: &str,
    short_id: &str,
) -> Result<WorktreeInfo, GitError> {
    let root = repository::require_root(git, repository)?;
    let managed_root = ensure_managed_root(managed_root)?;
    let branch = naming::generate_branch_name(git, &root, task_name, short_id)?;
    let directory_base = branch
        .strip_prefix("agent/")
        .ok_or_else(|| invalid_worktree_output("generated branch prefix was invalid"))?;
    let destination = unique_destination(&managed_root, directory_base)?;
    validate_descendant(&managed_root, &destination, false)?;

    let output = git.execute(
        Some(&root),
        [
            os("worktree"),
            os("add"),
            os("-b"),
            os(branch.clone()),
            destination.as_os_str().to_os_string(),
        ],
        8 * 1024 * 1024,
    )?;
    if !output.success() {
        return Err(git.command_error("create the worktree", &output));
    }
    let canonical = destination.canonicalize().map_err(|error| {
        GitError::io("resolve the created worktree", error).with_path(&destination)
    })?;
    validate_descendant(&managed_root, &canonical, true)?;
    list(git, &root)?
        .into_iter()
        .find(|worktree| canonicalize_if_possible(&worktree.path) == canonical)
        .ok_or_else(|| invalid_worktree_output("created worktree was absent from Git's registry"))
}

pub(crate) fn prepare_remove(
    git: &Git,
    repository: &Path,
    managed_root: &Path,
    worktree_path: &Path,
    usage: WorktreeUse,
) -> Result<RemovalPreparation, GitError> {
    let root = repository::require_root(git, repository)?;
    let managed_root = canonical_existing_directory(managed_root, "managed worktree root")?;
    let target = canonical_existing_directory(worktree_path, "worktree")?;
    validate_descendant(&managed_root, &target, true)?;
    if target == root {
        return Err(GitError::new(
            GitErrorKind::UnsafePath,
            "The primary repository checkout cannot be removed as a managed worktree",
            "Select a linked worktree below the managed worktree root",
        )
        .with_path(target));
    }
    let registered = list(git, &root)?
        .into_iter()
        .find(|worktree| canonicalize_if_possible(&worktree.path) == target);
    let Some(registered) = registered else {
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
    let (status, ignored_paths) = status::read_for_removal(git, &target)?;
    let (assume_unchanged_paths, skip_worktree_paths) = read_index_flags(git, &target)?;
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
    let can_remove = blockers.is_empty();
    Ok(RemovalPreparation {
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

pub(crate) fn remove(
    git: &Git,
    repository: &Path,
    managed_root: &Path,
    worktree_path: &Path,
    usage: WorktreeUse,
) -> Result<(), GitError> {
    let preparation = prepare_remove(git, repository, managed_root, worktree_path, usage)?;
    ensure_removable(&preparation, worktree_path)?;
    let root = repository::require_root(git, repository)?;
    let target = worktree_path.canonicalize().map_err(|error| {
        GitError::io("resolve worktree before removal", error).with_path(worktree_path)
    })?;
    // Re-read Git and caller-observed state immediately before invoking remove.
    // Another process can still race after this check; omitting `--force` leaves
    // Git's own clean-worktree guard as the final line of defense.
    let final_preparation = prepare_remove(git, repository, managed_root, &target, usage)?;
    ensure_removable(&final_preparation, &target)?;
    git.checked(
        Some(&root),
        [
            os("worktree"),
            os("remove"),
            target.as_os_str().to_os_string(),
        ],
        "remove the worktree",
    )?;
    Ok(())
}

fn ensure_removable(
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
            return Err(invalid_worktree_output(
                "git ls-files returned malformed index flags",
            ));
        }
        let tag = record[0];
        let path = bytes_to_path(&record[2..]);
        // `git ls-files -v` lowercases its status tag for assume-unchanged
        // entries. Blocking every lowercase tag is intentionally conservative.
        if tag.is_ascii_lowercase() {
            assume_unchanged.push(path.clone());
        }
        if matches!(tag, b'S' | b's') {
            skip_worktree.push(path);
        }
    }
    Ok((assume_unchanged, skip_worktree))
}

fn parse_list(bytes: &[u8]) -> Result<Vec<WorktreeInfo>, GitError> {
    let mut result = Vec::new();
    let mut current: Option<WorktreeInfo> = None;
    for record in bytes.split(|byte| *byte == 0) {
        if record.is_empty() {
            if let Some(worktree) = current.take() {
                result.push(worktree);
            }
            continue;
        }
        if let Some(path) = record.strip_prefix(b"worktree ") {
            if let Some(worktree) = current.take() {
                result.push(worktree);
            }
            current = Some(WorktreeInfo {
                path: bytes_to_path(path),
                head: None,
                branch: None,
                detached: false,
                locked: false,
                prunable: false,
            });
            continue;
        }
        let worktree = current
            .as_mut()
            .ok_or_else(|| invalid_worktree_output("metadata appeared before a worktree path"))?;
        if let Some(head) = record.strip_prefix(b"HEAD ") {
            worktree.head = Some(parse_utf8(head, "worktree HEAD")?.to_owned());
        } else if let Some(branch) = record.strip_prefix(b"branch refs/heads/") {
            worktree.branch = Some(parse_utf8(branch, "worktree branch")?.to_owned());
        } else if record == b"detached" {
            worktree.detached = true;
        } else if record == b"locked" || record.starts_with(b"locked ") {
            worktree.locked = true;
        } else if record == b"prunable" || record.starts_with(b"prunable ") {
            worktree.prunable = true;
        }
    }
    if let Some(worktree) = current {
        result.push(worktree);
    }
    if result.is_empty() {
        return Err(invalid_worktree_output("Git returned no worktrees"));
    }
    Ok(result)
}

fn ensure_managed_root(path: &Path) -> Result<PathBuf, GitError> {
    if path.as_os_str().is_empty() {
        return Err(GitError::new(
            GitErrorKind::InvalidInput,
            "Managed worktree root must not be empty",
            "Configure an absolute application worktree directory",
        ));
    }
    fs::create_dir_all(path)
        .map_err(|error| GitError::io("create managed worktree root", error).with_path(path))?;
    canonical_existing_directory(path, "managed worktree root")
}

fn canonical_existing_directory(path: &Path, label: &'static str) -> Result<PathBuf, GitError> {
    if !path.is_dir() {
        return Err(GitError::new(
            GitErrorKind::InvalidInput,
            format!("{label} is not an existing directory: {}", path.display()),
            "Choose an existing directory",
        )
        .with_path(path));
    }
    path.canonicalize()
        .map_err(|error| GitError::io("resolve directory", error).with_path(path))
}

fn validate_descendant(
    managed_root: &Path,
    target: &Path,
    must_exist: bool,
) -> Result<(), GitError> {
    let checked = if must_exist {
        target
            .canonicalize()
            .map_err(|error| GitError::io("resolve managed worktree", error).with_path(target))?
    } else {
        if target.exists() {
            return Err(GitError::new(
                GitErrorKind::InvalidInput,
                format!("Worktree destination already exists: {}", target.display()),
                "Choose a different session name or identifier",
            )
            .with_path(target));
        }
        let parent = target.parent().ok_or_else(|| {
            GitError::new(
                GitErrorKind::UnsafePath,
                "Worktree destination has no parent directory",
                "Use a destination below the managed worktree root",
            )
        })?;
        let parent = parent.canonicalize().map_err(|error| {
            GitError::io("resolve worktree destination parent", error).with_path(parent)
        })?;
        parent.join(target.file_name().unwrap_or_default())
    };
    if checked == managed_root || !checked.starts_with(managed_root) {
        return Err(GitError::new(
            GitErrorKind::UnsafePath,
            format!(
                "Worktree path is outside the managed root: {}",
                checked.display()
            ),
            "Choose a worktree created below the configured managed worktree root",
        )
        .with_path(checked));
    }
    Ok(())
}

fn unique_destination(managed_root: &Path, base: &str) -> Result<PathBuf, GitError> {
    for collision in 1..=10_000 {
        let name = if collision == 1 {
            base.to_owned()
        } else {
            format!("{base}-{collision}")
        };
        let candidate = managed_root.join(name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(GitError::new(
        GitErrorKind::InvalidInput,
        "Could not allocate a unique managed worktree directory",
        "Use a different task name or remove stale worktree directories",
    ))
}

fn parse_utf8<'a>(bytes: &'a [u8], field: &'static str) -> Result<&'a str, GitError> {
    std::str::from_utf8(bytes)
        .map_err(|_| invalid_worktree_output(&format!("Git returned a non-UTF-8 {field}")))
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

fn canonicalize_if_possible(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn invalid_worktree_output(detail: &str) -> GitError {
    GitError::new(
        GitErrorKind::InvalidOutput,
        format!("Could not parse Git worktree data: {detail}"),
        "Run `git worktree list` to inspect the repository and update Git if necessary",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_worktree_list() {
        let bytes = b"worktree /tmp/main\0HEAD abc123\0branch refs/heads/main\0\0\
            worktree /tmp/agent\0HEAD def456\0branch refs/heads/agent/task-123\0locked reason\0\0";
        let worktrees = parse_list(bytes).expect("fixture should parse");
        assert_eq!(worktrees.len(), 2);
        assert_eq!(worktrees[1].branch.as_deref(), Some("agent/task-123"));
        assert!(worktrees[1].locked);
    }
}
