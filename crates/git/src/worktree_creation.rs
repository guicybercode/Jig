use std::{
    fs,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
#[cfg(not(unix))]
use std::time::SystemTime;

use crate::{
    Git, GitError, GitErrorKind, naming, os, path_safety, repository, worktree,
    worktree::WorktreeInfo, worktree_reconcile,
};

/// A side-effect-free, durable description of a future managed worktree.
///
/// Paths are resolved through every existing ancestor. The plan is bound to
/// the physical Git common directory and an initial commit object ID, so later
/// creation cannot silently target a recreated repository or a different base.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorktreePlan {
    repository_root: PathBuf,
    git_common_dir: PathBuf,
    common_dir_identity: DirectoryIdentity,
    initial_oid: String,
    managed_root: PathBuf,
    branch: String,
    destination: PathBuf,
}

impl WorktreePlan {
    /// Returns the canonical repository root the plan is bound to.
    #[must_use]
    pub fn repository_root(&self) -> &Path {
        &self.repository_root
    }

    /// Returns the canonical Git common directory the plan is bound to.
    #[must_use]
    pub fn git_common_dir(&self) -> &Path {
        &self.git_common_dir
    }

    /// Returns the full commit object ID used as the worktree starting point.
    #[must_use]
    pub fn initial_oid(&self) -> &str {
        &self.initial_oid
    }

    /// Returns the canonical or canonically intended managed root.
    #[must_use]
    pub fn managed_root(&self) -> &Path {
        &self.managed_root
    }

    /// Returns the validated local branch that creation will preserve.
    #[must_use]
    pub fn branch(&self) -> &str {
        &self.branch
    }

    /// Returns the canonically safe worktree destination.
    #[must_use]
    pub fn destination(&self) -> &Path {
        &self.destination
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DirectoryIdentity {
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(not(unix))]
    modified: SystemTime,
    #[cfg(not(unix))]
    length: u64,
}

pub(crate) fn plan(
    git: &Git,
    repository: &Path,
    managed_root: &Path,
    task_name: &str,
    short_id: &str,
) -> Result<WorktreePlan, GitError> {
    let root = repository::require_root(git, repository)?;
    let git_common_dir = repository::require_common_dir(git, &root)?;
    let common_dir_identity = directory_identity(&git_common_dir)?;
    let initial_oid = repository::resolve_head_commit(git, &root)?;
    let managed_root =
        path_safety::canonical_intended_directory(managed_root, "managed worktree root")?;
    let branch = naming::generate_branch_name(git, &root, task_name, short_id)?;
    let directory_base = branch
        .strip_prefix("agent/")
        .ok_or_else(|| worktree::invalid_worktree_output("generated branch prefix was invalid"))?;
    let registered = worktree::list(git, &root)?;
    let destination = unique_destination(&managed_root, directory_base, &registered)?;
    path_safety::validate_descendant(&managed_root, &destination, false)?;
    Ok(WorktreePlan {
        repository_root: root,
        git_common_dir,
        common_dir_identity,
        initial_oid,
        managed_root,
        branch,
        destination,
    })
}

pub(crate) fn create(
    git: &Git,
    repository: &Path,
    managed_root: &Path,
    task_name: &str,
    short_id: &str,
) -> Result<WorktreeInfo, GitError> {
    let plan = plan(git, repository, managed_root, task_name, short_id)?;
    create_from_plan(git, &plan)
}

pub(crate) fn create_from_plan(git: &Git, plan: &WorktreePlan) -> Result<WorktreeInfo, GitError> {
    revalidate_plan(git, plan)?;
    let add_result = git.execute(
        Some(&plan.repository_root),
        [
            os("worktree"),
            os("add"),
            os("-b"),
            os(plan.branch.clone()),
            plan.destination.as_os_str().to_os_string(),
            os(plan.initial_oid.clone()),
        ],
        8 * 1024 * 1024,
    );
    match add_result {
        Err(error) => worktree_reconcile::after_add(git, plan, error),
        Ok(output) if !output.success() => {
            let error = git.command_error("create the worktree", &output);
            worktree_reconcile::after_add(git, plan, error)
        }
        Ok(_) => match confirm_created(git, plan) {
            Ok(worktree) => Ok(worktree),
            Err(error) => worktree_reconcile::after_add(git, plan, error),
        },
    }
}

fn revalidate_plan(git: &Git, plan: &WorktreePlan) -> Result<(), GitError> {
    validate_plan_shape(plan)?;
    let current_root = repository::require_root(git, &plan.repository_root)?;
    if current_root != plan.repository_root {
        return Err(stale_plan_error(
            "repository root identity changed",
            &plan.repository_root,
        ));
    }
    let current_common_dir = repository::require_common_dir(git, &current_root)?;
    let current_identity = directory_identity(&current_common_dir)?;
    if current_common_dir != plan.git_common_dir || current_identity != plan.common_dir_identity {
        return Err(stale_plan_error(
            "Git common directory identity changed",
            &plan.git_common_dir,
        ));
    }
    repository::verify_commit(git, &current_root, &plan.initial_oid)?;

    let intended_root =
        path_safety::canonical_intended_directory(&plan.managed_root, "managed worktree root")?;
    if intended_root != plan.managed_root {
        return Err(stale_plan_error(
            "managed root now resolves to a different location",
            &plan.managed_root,
        ));
    }
    fs::create_dir_all(&plan.managed_root).map_err(|error| {
        GitError::io("create managed worktree root", error).with_path(&plan.managed_root)
    })?;
    let current_managed =
        path_safety::canonical_existing_directory(&plan.managed_root, "managed worktree root")?;
    if current_managed != plan.managed_root {
        return Err(stale_plan_error(
            "managed root changed while it was being created",
            &plan.managed_root,
        ));
    }
    path_safety::validate_descendant(&current_managed, &plan.destination, false)?;
    naming::validate_branch(git, &current_root, &plan.branch)?;
    if naming::branch_exists(git, &current_root, &plan.branch)? {
        return Err(GitError::new(
            GitErrorKind::InvalidInput,
            format!("Planned worktree branch now exists: {}", plan.branch),
            "Reconcile the stored plan before generating a replacement",
        ));
    }
    if worktree::list(git, &current_root)?
        .iter()
        .any(|worktree| path_safety::paths_match(&worktree.path, &plan.destination))
    {
        return Err(GitError::new(
            GitErrorKind::InvalidInput,
            format!(
                "Planned worktree destination is already registered: {}",
                plan.destination.display()
            ),
            "Reconcile the stored plan before generating a replacement",
        )
        .with_path(&plan.destination));
    }
    Ok(())
}

fn validate_plan_shape(plan: &WorktreePlan) -> Result<(), GitError> {
    if plan.destination.parent() != Some(plan.managed_root.as_path())
        || plan.destination.file_name().is_none()
        || !plan.branch.starts_with("agent/")
        || plan.initial_oid.is_empty()
    {
        return Err(GitError::new(
            GitErrorKind::UnsafePath,
            "Worktree plan invariants are invalid",
            "Discard the stale plan and generate a new one",
        )
        .with_path(&plan.destination));
    }
    Ok(())
}

fn confirm_created(git: &Git, plan: &WorktreePlan) -> Result<WorktreeInfo, GitError> {
    let canonical =
        path_safety::canonical_existing_directory(&plan.destination, "created worktree")?;
    if canonical != plan.destination {
        return Err(stale_plan_error(
            "created worktree resolved to an unexpected location",
            &plan.destination,
        ));
    }
    path_safety::validate_descendant(&plan.managed_root, &canonical, true)?;
    let mut worktree = worktree::list(git, &plan.repository_root)?
        .into_iter()
        .find(|worktree| path_safety::paths_match(&worktree.path, &canonical))
        .ok_or_else(|| {
            worktree::invalid_worktree_output("created worktree was absent from Git's registry")
        })?;
    if worktree.branch.as_deref() != Some(plan.branch.as_str())
        || worktree.head.as_deref() != Some(plan.initial_oid.as_str())
        || worktree.detached
        || worktree.prunable
    {
        return Err(worktree::invalid_worktree_output(
            "created worktree registry identity did not match its plan",
        ));
    }
    worktree.path = canonical;
    Ok(worktree)
}

pub(crate) fn stale_plan_error(detail: &str, path: &Path) -> GitError {
    GitError::new(
        GitErrorKind::UnsafePath,
        format!("Worktree plan is no longer safe: {detail}"),
        "Discard the stale plan, inspect the configured paths, and generate a new plan",
    )
    .with_path(path)
}

fn unique_destination(
    managed_root: &Path,
    base: &str,
    registered: &[WorktreeInfo],
) -> Result<PathBuf, GitError> {
    for collision in 1..=10_000 {
        let name = if collision == 1 {
            base.to_owned()
        } else {
            format!("{base}-{collision}")
        };
        let candidate = managed_root.join(name);
        let registered_collision = registered
            .iter()
            .any(|worktree| path_safety::paths_match(&worktree.path, &candidate));
        if !path_safety::path_occupied(&candidate)? && !registered_collision {
            return Ok(candidate);
        }
    }
    Err(GitError::new(
        GitErrorKind::InvalidInput,
        "Could not allocate a unique managed worktree directory",
        "Use a different task name or remove stale worktree directories",
    ))
}

fn directory_identity(path: &Path) -> Result<DirectoryIdentity, GitError> {
    let metadata = fs::metadata(path)
        .map_err(|error| GitError::io("inspect the Git common directory", error).with_path(path))?;
    #[cfg(unix)]
    {
        Ok(DirectoryIdentity {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
    #[cfg(not(unix))]
    {
        Ok(DirectoryIdentity {
            modified: metadata.modified().map_err(|error| {
                GitError::io("read Git common directory identity", error).with_path(path)
            })?,
            length: metadata.len(),
        })
    }
}
