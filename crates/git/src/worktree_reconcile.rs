use crate::{
    Git, GitError, GitErrorKind, naming, os, path_safety, worktree,
    worktree::{WorktreeInfo, WorktreeUse},
    worktree_creation::WorktreePlan,
    worktree_removal,
};

struct CreationObservation {
    branch_exists: bool,
    path_exists: bool,
    registered: Option<WorktreeInfo>,
}

impl CreationObservation {
    fn proves_absence(&self) -> bool {
        !self.branch_exists && !self.path_exists && self.registered.is_none()
    }

    fn supports_exact_cleanup(&self, plan: &WorktreePlan) -> bool {
        self.branch_exists
            && self.path_exists
            && self.registered.as_ref().is_some_and(|registered| {
                registered.branch.as_deref() == Some(plan.branch())
                    && registered.head.as_deref() == Some(plan.initial_oid())
                    && !registered.detached
                    && !registered.prunable
            })
    }

    fn summary(&self) -> String {
        let registration = self.registered.as_ref().map_or("absent", |worktree| {
            if worktree.detached {
                "detached"
            } else {
                "present"
            }
        });
        format!(
            "branch={}, path={}, registration={registration}",
            self.branch_exists, self.path_exists
        )
    }
}

pub(crate) fn after_add(
    git: &Git,
    plan: &WorktreePlan,
    original: GitError,
) -> Result<WorktreeInfo, GitError> {
    let observation = match observe_creation(git, plan) {
        Ok(observation) => observation,
        Err(observation_error) => {
            return Err(partial_creation_error(
                plan,
                &original,
                &format!(
                    "creation effects could not be inspected: {}",
                    observation_error.message()
                ),
            ));
        }
    };
    if observation.proves_absence() {
        return Err(original);
    }
    if !observation.supports_exact_cleanup(plan) {
        return Err(partial_creation_error(
            plan,
            &original,
            &format!("observed partial state: {}", observation.summary()),
        ));
    }
    match compensate_failed_creation(git, plan) {
        Ok(()) => Err(rolled_back_error(plan, &original)),
        Err(cleanup) => Err(partial_creation_error(
            plan,
            &original,
            &format!(
                "conservative cleanup could not be proven: {}",
                cleanup.message()
            ),
        )),
    }
}

fn observe_creation(git: &Git, plan: &WorktreePlan) -> Result<CreationObservation, GitError> {
    let branch_exists = naming::branch_exists(git, plan.repository_root(), plan.branch())?;
    let path_exists = path_safety::path_occupied(plan.destination())?;
    let registered = worktree::list(git, plan.repository_root())?
        .into_iter()
        .find(|worktree| path_safety::paths_match(&worktree.path, plan.destination()));
    Ok(CreationObservation {
        branch_exists,
        path_exists,
        registered,
    })
}

fn compensate_failed_creation(git: &Git, plan: &WorktreePlan) -> Result<(), GitError> {
    let canonical =
        path_safety::canonical_existing_directory(plan.destination(), "partial worktree")?;
    if canonical != plan.destination() {
        return Err(crate::worktree_creation::stale_plan_error(
            "partial worktree path resolves outside its plan",
            plan.destination(),
        ));
    }
    path_safety::validate_descendant(plan.managed_root(), &canonical, true)?;
    let preparation = worktree_removal::prepare_remove(
        git,
        plan.repository_root(),
        plan.managed_root(),
        &canonical,
        WorktreeUse::default(),
    )?;
    worktree_removal::ensure_removable(&preparation, &canonical)?;
    ensure_planned_identity(&preparation.worktree, plan)?;
    let final_preparation = worktree_removal::prepare_remove(
        git,
        plan.repository_root(),
        plan.managed_root(),
        &canonical,
        WorktreeUse::default(),
    )?;
    worktree_removal::ensure_removable(&final_preparation, &canonical)?;
    worktree_removal::ensure_snapshot_unchanged(&preparation, &final_preparation, &canonical)?;
    ensure_planned_identity(&final_preparation.worktree, plan)?;
    git.checked(
        Some(plan.repository_root()),
        [
            os("worktree"),
            os("remove"),
            canonical.as_os_str().to_os_string(),
        ],
        "roll back the partial worktree",
    )?;
    verify_cleanup(git, plan)
}

fn ensure_planned_identity(worktree: &WorktreeInfo, plan: &WorktreePlan) -> Result<(), GitError> {
    if worktree.branch.as_deref() == Some(plan.branch())
        && worktree.head.as_deref() == Some(plan.initial_oid())
        && !worktree.detached
        && !worktree.prunable
    {
        return Ok(());
    }
    Err(worktree::invalid_worktree_output(
        "partial worktree identity no longer matches its plan",
    ))
}

fn verify_cleanup(git: &Git, plan: &WorktreePlan) -> Result<(), GitError> {
    if path_safety::path_occupied(plan.destination())? {
        return Err(GitError::new(
            GitErrorKind::PartialWorktree,
            "Git reported cleanup success but the worktree path still exists",
            "Inspect the path and preserve its contents before resolving the worktree manually",
        )
        .with_path(plan.destination()));
    }
    if worktree::list(git, plan.repository_root())?
        .iter()
        .any(|worktree| path_safety::paths_match(&worktree.path, plan.destination()))
    {
        return Err(GitError::new(
            GitErrorKind::PartialWorktree,
            "Git reported cleanup success but the worktree remains registered",
            "Inspect `git worktree list` and resolve the stale registration manually",
        )
        .with_path(plan.destination()));
    }
    Ok(())
}

fn rolled_back_error(plan: &WorktreePlan, original: &GitError) -> GitError {
    GitError::new(
        original.kind(),
        format!(
            "Worktree creation did not complete and was safely rolled back: {}",
            original.message()
        ),
        format!(
            "Generate a fresh plan before retrying; branch {} was preserved",
            plan.branch()
        ),
    )
    .with_path(plan.destination())
}

fn partial_creation_error(plan: &WorktreePlan, original: &GitError, detail: &str) -> GitError {
    GitError::new(
        GitErrorKind::PartialWorktree,
        format!(
            "Worktree creation may be partial after {}: {detail}",
            original.message()
        ),
        format!(
            "Do not retry automatically; inspect path {} and branch {} with `git worktree list` and preserve any user data",
            plan.destination().display(),
            plan.branch()
        ),
    )
    .with_path(plan.destination())
}
