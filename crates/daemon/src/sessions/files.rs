use std::path::Path;

use cli_master_core::ApiError;
use cli_master_core::wire::FileTarget;
use cli_master_storage::{StoredWorktree, WorktreeState};

use crate::files::{FileTargetAccess, ResolvedFileTarget};

use super::{SessionRegistry, storage_error};

impl FileTargetAccess for SessionRegistry {
    fn resolve_target(
        &self,
        target: &FileTarget,
        write: bool,
    ) -> Result<ResolvedFileTarget, ApiError> {
        let (root, worktrees) = {
            let storage = self.storage()?;
            let root = match *target {
                FileTarget::Project { project_id } => storage
                    .get_project(project_id)
                    .map_err(storage_error)?
                    .map(|project| project.path),
                FileTarget::Session { session_id } => storage
                    .get_session(session_id)
                    .map_err(storage_error)?
                    .map(|session| session.cwd),
                FileTarget::Worktree { worktree_id } => storage
                    .get_worktree(worktree_id)
                    .map_err(storage_error)?
                    .map(|worktree| worktree.path),
            }
            .ok_or_else(|| {
                ApiError::new(
                    "file_target_not_found",
                    "The selected file target is not registered.",
                )
                .with_action("Refresh projects and sessions before opening files.")
            })?;
            (root, storage.list_worktrees().map_err(storage_error)?)
        };
        if root.canonicalize().map_err(|_| target_changed())? != root || !root.is_dir() {
            return Err(target_changed());
        }
        // Projects may be registered directly at a managed checkout. Enforce the
        // same identity and write policy even through such an alternate target ID.
        for worktree in worktrees
            .iter()
            .filter(|worktree| root.starts_with(&worktree.path))
        {
            self.validate_file_worktree(worktree, write)?;
        }
        Ok(ResolvedFileTarget { root })
    }

    fn with_write_lease<T>(
        &self,
        target: &FileTarget,
        expected_root: &Path,
        operation: impl FnOnce() -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        let _lifecycle = self.lifecycle()?;
        if self.resolve_target(target, true)?.root != expected_root {
            return Err(target_changed());
        }
        operation()
    }
}

impl SessionRegistry {
    fn validate_file_worktree(
        &self,
        worktree: &StoredWorktree,
        write: bool,
    ) -> Result<(), ApiError> {
        if matches!(
            worktree.state,
            WorktreeState::Creating | WorktreeState::Orphaned
        ) || (write && worktree.state != WorktreeState::Active)
            || worktree.path.canonicalize().map_err(|_| target_changed())? != worktree.path
        {
            return Err(target_changed());
        }
        let project = self
            .storage()?
            .get_project(worktree.project_id)
            .map_err(storage_error)?
            .ok_or_else(target_changed)?;
        let git = self.git.as_ref().ok_or_else(target_changed)?;
        let registered = git
            .list_worktrees(&project.path)
            .map_err(|_| target_changed())?;
        let inspection = git
            .inspect_repository(&worktree.path)
            .map_err(|_| target_changed())?;
        if inspection.repository_root.as_ref() != Some(&worktree.path)
            || inspection.branch.as_deref() != Some(worktree.branch.as_str())
            || !registered.iter().any(|entry| {
                entry.path == worktree.path
                    && entry.branch.as_deref() == Some(worktree.branch.as_str())
                    && !entry.prunable
            })
        {
            return Err(target_changed());
        }
        Ok(())
    }

    /// Metadata removal and the final file publish share the worktree mutation lease.
    pub(crate) fn with_metadata_mutation<T>(
        &self,
        operation: impl FnOnce() -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        let _lifecycle = self.lifecycle()?;
        operation()
    }
}

fn target_changed() -> ApiError {
    ApiError::new(
        "file_target_changed",
        "The registered file target changed or needs recovery.",
    )
    .with_action("Refresh the target and reopen the file before retrying.")
}
