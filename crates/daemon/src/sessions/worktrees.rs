use cli_master_core::wire::{
    SessionCreateRequest, WorktreeListRequest, WorktreeListResponse, WorktreePrepareRemoveRequest,
    WorktreePrepareRemoveResponse, WorktreeRemovalBlocker, WorktreeRemoveRequest,
};
use cli_master_core::{ApiError, Session, Worktree, WorktreeState};
use cli_master_session::{CreateSession, SagaError, SessionManager, SessionWorktreeSaga};
use cli_master_storage::{Storage, StoredSession, StoredWorktree};

use super::{SessionRegistry, not_found, storage_error, stored_session};

impl SessionRegistry {
    pub(super) fn prepare_with_saga(
        &self,
        saga: &SessionWorktreeSaga<SessionManager>,
        request: SessionCreateRequest,
    ) -> Result<Session, ApiError> {
        // The root belongs to the daemon; callers cannot supply paths or branches.
        let creation = CreateSession {
            project_id: request.project_id,
            agent_id: request.agent_id,
            name: request.name.into_inner(),
            isolation: request.isolation,
            managed_root: self.managed_root.join(request.project_id.to_string()),
            short_id: None,
        };
        saga.prepare_session(&creation, request.relative_directory.as_ref())
            .map(|created| created.session)
            .map_err(saga_error)
    }

    pub(crate) fn worktrees(&self) -> Result<Vec<Worktree>, ApiError> {
        self.list_worktrees(WorktreeListRequest::default())
            .map(|response| response.worktrees)
    }

    pub(crate) fn list_worktrees(
        &self,
        request: WorktreeListRequest,
    ) -> Result<WorktreeListResponse, ApiError> {
        let storage = self.storage()?;
        let worktrees = match request.project_id {
            Some(project_id) => storage.list_worktrees_for_project(project_id),
            None => storage.list_worktrees(),
        }
        .map_err(storage_error)?;
        Ok(WorktreeListResponse {
            worktrees: worktrees.into_iter().map(worktree_dto).collect(),
        })
    }

    pub(crate) fn prepare_worktree_removal(
        &self,
        request: WorktreePrepareRemoveRequest,
    ) -> Result<WorktreePrepareRemoveResponse, ApiError> {
        let _lifecycle = self.lifecycle()?;
        let saga = self.worktree_saga.as_ref().ok_or_else(git_unavailable)?;
        if self.has_live_worktree_user(request.worktree_id)? {
            return Ok(WorktreePrepareRemoveResponse::Blocked {
                worktree_id: request.worktree_id,
                is_dirty: self
                    .storage()?
                    .get_worktree(request.worktree_id)
                    .map_err(storage_error)?
                    .is_some_and(|worktree| worktree.is_dirty),
                blockers: vec![
                    WorktreeRemovalBlocker::Running,
                    WorktreeRemovalBlocker::InUse,
                ],
            });
        }
        saga.prepare_remove(request.worktree_id).map_err(saga_error)
    }

    pub(crate) fn remove_worktree(
        &self,
        request: &WorktreeRemoveRequest,
    ) -> Result<cli_master_core::wire::EmptyResponse, ApiError> {
        let _lifecycle = self.lifecycle()?;
        let saga = self.worktree_saga.as_ref().ok_or_else(git_unavailable)?;
        if self.has_live_worktree_user(request.worktree_id)? {
            return Err(ApiError::new(
                "worktree_confirmation_invalid",
                "A live session now uses this worktree.",
            )
            .with_action("Stop its sessions and prepare removal again."));
        }
        saga.remove_worktree(request.worktree_id, &request.confirmation_token)
            .map_err(saga_error)?;
        Ok(cli_master_core::wire::EmptyResponse::default())
    }

    fn has_live_worktree_user(
        &self,
        worktree_id: cli_master_core::WorktreeId,
    ) -> Result<bool, ApiError> {
        let (worktree, sessions) = {
            let storage = self.storage()?;
            let worktree = storage
                .get_worktree(worktree_id)
                .map_err(storage_error)?
                .ok_or_else(|| not_found("worktree", worktree_id))?;
            let sessions = storage.list_sessions().map_err(storage_error)?;
            (worktree, sessions)
        };
        let root = worktree.path.canonicalize().unwrap_or(worktree.path);
        let mut live = false;
        for session in sessions {
            // Also cover sessions registered directly at this worktree or a child folder.
            let cwd = session.cwd.canonicalize().unwrap_or(session.cwd);
            if worktree.session_id == Some(session.id) || cwd.starts_with(&root) {
                if let Ok(snapshot) = self.manager.snapshot(session.id) {
                    self.persist_snapshot(&snapshot)?;
                    live |= snapshot.status.is_live();
                } else {
                    live |= session.status.is_live();
                }
            }
        }
        Ok(live)
    }
}

pub(super) fn session_with_worktree(
    storage: &Storage,
    stored: StoredSession,
) -> Result<Session, ApiError> {
    let worktree = storage
        .list_worktrees_for_project(stored.project_id)
        .map_err(storage_error)?
        .into_iter()
        .find(|worktree| worktree.session_id == Some(stored.id));
    let mut session = stored_session(stored);
    if let Some(worktree) = worktree {
        session.branch = Some(worktree.branch);
        session.worktree_id = Some(worktree.id);
        session.worktree_path = Some(worktree.path);
    }
    Ok(session)
}

impl SessionRegistry {
    pub(super) fn validate_start_directory(
        &self,
        stored: &StoredSession,
    ) -> Result<Option<cli_master_core::WorktreeId>, ApiError> {
        let (project, worktree) = {
            let storage = self.storage()?;
            let project = storage
                .get_project(stored.project_id)
                .map_err(storage_error)?
                .ok_or_else(|| not_found("project", stored.project_id))?;
            let worktree = storage
                .list_worktrees_for_project(stored.project_id)
                .map_err(storage_error)?
                .into_iter()
                .find(|worktree| worktree.session_id == Some(stored.id));
            (project, worktree)
        };
        let worktree_id = worktree.as_ref().map(|worktree| worktree.id);
        let root = if let Some(worktree) = worktree {
            if !matches!(
                worktree.state,
                cli_master_storage::WorktreeState::Active
                    | cli_master_storage::WorktreeState::RemovePending
            ) {
                return Err(
                    ApiError::new("worktree_not_active", "This worktree needs recovery.")
                        .with_action("Resolve the incomplete operation before starting."),
                );
            }
            if worktree
                .path
                .canonicalize()
                .map_err(|_| directory_unavailable())?
                != worktree.path
            {
                return Err(directory_unavailable());
            }
            let git = self.git.as_ref().ok_or_else(git_unavailable)?;
            let registered = git
                .list_worktrees(&project.path)
                .map_err(|error| saga_error(error.into()))?;
            let inspection = git
                .inspect_repository(&worktree.path)
                .map_err(|error| saga_error(error.into()))?;
            if inspection.repository_root.as_ref() != Some(&worktree.path)
                || inspection.branch.as_deref() != Some(worktree.branch.as_str())
                || !registered.iter().any(|entry| {
                    entry.path == worktree.path
                        && entry.branch.as_deref() == Some(worktree.branch.as_str())
                        && !entry.prunable
                })
            {
                return Err(ApiError::new(
                    "worktree_identity_changed",
                    "The managed Git worktree identity changed.",
                )
                .with_action("Restore the original worktree or create a new isolated session."));
            }
            worktree.path
        } else {
            project.path
        };
        let canonical_root = root.canonicalize().map_err(|_| directory_unavailable())?;
        let cwd = stored
            .cwd
            .canonicalize()
            .map_err(|_| directory_unavailable())?;
        if canonical_root != root || cwd != stored.cwd || !cwd.is_dir() || !cwd.starts_with(root) {
            return Err(directory_unavailable());
        }
        Ok(worktree_id)
    }
}

fn directory_unavailable() -> ApiError {
    ApiError::new(
        "session_directory_unavailable",
        "The session directory is missing or outside its project/worktree.",
    )
    .with_action("Restore the directory or create a session in an existing project folder.")
}

fn worktree_dto(stored: StoredWorktree) -> Worktree {
    Worktree {
        id: stored.id,
        project_id: stored.project_id,
        session_id: stored.session_id,
        path: stored.path,
        branch: stored.branch,
        is_dirty: stored.is_dirty,
        state: match stored.state {
            cli_master_storage::WorktreeState::Creating => WorktreeState::Creating,
            cli_master_storage::WorktreeState::Active => WorktreeState::Active,
            cli_master_storage::WorktreeState::RemovePending => WorktreeState::RemovePending,
            cli_master_storage::WorktreeState::Orphaned => WorktreeState::Orphaned,
        },
        created_at_ms: stored.created_at_ms,
        updated_at_ms: stored.updated_at_ms,
    }
}

pub(super) fn git_unavailable() -> ApiError {
    ApiError::new(
        "git_unavailable",
        "Git is required to manage isolated worktrees.",
    )
    .with_action("Install Git and restart the daemon.")
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "owned adapter for Result::map_err"
)]
pub(super) fn saga_error(error: SagaError) -> ApiError {
    let mut api = ApiError::new(error.code(), error.message()).with_action(error.action());
    if let Some(path) = error.path() {
        api = api.with_detail("path", path.to_string_lossy().into_owned());
    }
    if let Some(id) = error.worktree_id() {
        api = api.with_detail("worktreeId", id.to_string());
    }
    if let Some(id) = error.session_id() {
        api = api.with_detail("sessionId", id.to_string());
    }
    api
}
