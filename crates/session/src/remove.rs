use std::path::PathBuf;

use cli_master_core::wire::{
    ConfirmationToken, WorktreePrepareRemoveResponse, WorktreeRemovalBlocker,
};
use cli_master_core::{ProjectId, SessionId, WorktreeId};
use cli_master_git::{RemovalBlocker, RemovalPreparation, WorktreeUse};
use cli_master_storage::WorktreeState;

use crate::create::{require_session, require_worktree};
use crate::error::{SagaError, SagaErrorKind};
use crate::lock::{lock_destination, lock_mutation};
use crate::map::is_live;
use crate::spawn::SessionSpawner;
use crate::token::now_ms;
use crate::{SessionWorktreeSaga, require_project};

pub(crate) fn prepare_remove<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    worktree_id: WorktreeId,
) -> Result<WorktreePrepareRemoveResponse, SagaError> {
    let _mutation = lock_mutation(&saga.mutations, worktree_id)?;
    let stored = require_worktree(saga, worktree_id)?;
    let project = require_project(saga, stored.project_id)?;
    let _destination = lock_destination(&saga.destinations, stored.path.clone())?;
    let usage = read_usage(saga, stored.session_id)?;
    let preparation = inspect(saga, &project.path, &stored.path, usage)?;
    recheck_identity(&stored.path, stored.branch.as_str(), &preparation)?;
    let now = now_ms();
    if !preparation.can_remove {
        saga.tokens.discard_for(worktree_id);
        saga.storage().update_worktree_state(
            worktree_id,
            WorktreeState::Active,
            preparation.status.is_dirty() || !preparation.ignored_paths.is_empty(),
            stored.session_id,
            now,
        )?;
        return Ok(WorktreePrepareRemoveResponse::Blocked {
            worktree_id,
            is_dirty: preparation.status.is_dirty()
                || !preparation.ignored_paths.is_empty()
                || !preparation.assume_unchanged_paths.is_empty()
                || !preparation.skip_worktree_paths.is_empty(),
            blockers: preparation
                .blockers
                .iter()
                .copied()
                .map(to_wire_blocker)
                .collect(),
        });
    }
    saga.storage().update_worktree_state(
        worktree_id,
        WorktreeState::RemovePending,
        false,
        stored.session_id,
        now,
    )?;
    let (token, expires_at_ms) =
        saga.tokens
            .issue(worktree_id, stored.session_id, preparation, now)?;
    Ok(WorktreePrepareRemoveResponse::Ready {
        worktree_id,
        confirmation_token: token,
        expires_at_ms,
    })
}

pub(crate) fn remove<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    worktree_id: WorktreeId,
    token: &ConfirmationToken,
) -> Result<(), SagaError> {
    let _mutation = lock_mutation(&saga.mutations, worktree_id)?;
    let now = now_ms();
    let record = saga.tokens.take(token, worktree_id, now)?;
    let stored = match require_worktree(saga, worktree_id) {
        Ok(stored) => stored,
        Err(error) => {
            saga.tokens.discard_for(worktree_id);
            return Err(error);
        }
    };
    if stored.session_id != record.session_id {
        saga.storage().update_worktree_state(
            worktree_id,
            WorktreeState::Active,
            stored.is_dirty,
            stored.session_id,
            now,
        )?;
        return Err(SagaError::new(
            SagaErrorKind::InvalidToken,
            "Worktree session association changed after the confirmation token was issued",
            "Call worktree.prepare_remove again for the current session association",
        )
        .with_worktree_id(worktree_id)
        .with_path(stored.path));
    }
    let _destination = lock_destination(&saga.destinations, stored.path.clone())?;
    let project = require_project(saga, stored.project_id)?;
    let usage = read_usage(saga, stored.session_id)?;
    let current = inspect(saga, &project.path, &stored.path, usage)?;
    recheck_identity(&stored.path, stored.branch.as_str(), &current)?;
    if current != record.preparation {
        saga.storage().update_worktree_state(
            worktree_id,
            WorktreeState::Active,
            current.status.is_dirty(),
            stored.session_id,
            now,
        )?;
        return Err(SagaError::new(
            SagaErrorKind::InvalidToken,
            "Worktree state changed after the confirmation token was issued",
            "Call worktree.prepare_remove again; there is no dirty or in-use bypass",
        )
        .with_worktree_id(worktree_id)
        .with_path(stored.path));
    }
    if !current.can_remove {
        saga.storage().update_worktree_state(
            worktree_id,
            WorktreeState::Active,
            current.status.is_dirty(),
            stored.session_id,
            now,
        )?;
        return Err(blocked_error(&current, &stored.path).with_worktree_id(worktree_id));
    }
    let managed_root = current.managed_root.clone();
    let repository_root = current.repository_root.clone();
    let path = stored.path.clone();
    let session_id = stored.session_id;
    saga.git
        .remove_worktree(&repository_root, &managed_root, &path, || {
            read_usage(saga, session_id).unwrap_or(WorktreeUse {
                running: true,
                in_use: true,
            })
        })?;
    saga.storage().remove_worktree_metadata(worktree_id)?;
    Ok(())
}

pub(crate) fn record_session_exit<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    session_id: SessionId,
    exit_code: Option<i32>,
) -> Result<(), SagaError> {
    if saga.spawner.is_live(session_id) {
        return Err(SagaError::new(
            SagaErrorKind::SessionInUse,
            format!("Session {session_id} still has a daemon-owned process"),
            "Wait for the SessionManager exit event before recording durable exit state",
        )
        .with_session_id(session_id));
    }
    let now = now_ms();
    saga.storage()
        .update_session_runtime(
            session_id,
            &cli_master_storage::SessionRuntimeUpdate {
                status: cli_master_core::SessionStatus::Exited,
                runtime_pid: None,
                daemon_instance_id: None,
                exit_code,
                error_code: None,
                last_activity_at_ms: Some(now),
                updated_at_ms: now,
            },
        )
        .map_err(SagaError::from)
}

pub(crate) fn delete_session<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    session_id: SessionId,
) -> Result<(), SagaError> {
    let session = require_session(saga, session_id)?;
    if is_live(session.status) || saga.spawner.is_live(session_id) {
        return Err(SagaError::new(
            SagaErrorKind::SessionInUse,
            format!("Session {session_id} is still live and cannot have its metadata deleted"),
            "Stop the session first. Deleting metadata never removes a worktree directory",
        )
        .with_session_id(session_id));
    }
    saga.storage()
        .remove_session_metadata(session_id)
        .map_err(SagaError::from)
}

pub(crate) fn remove_project<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    project_id: ProjectId,
) -> Result<PathBuf, SagaError> {
    let project = require_project(saga, project_id)?;
    let repository = project.path.clone();
    saga.storage()
        .remove_project_metadata(project_id)
        .map_err(SagaError::from)?;
    Ok(repository)
}

fn inspect<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    repository: &std::path::Path,
    worktree_path: &std::path::Path,
    usage: WorktreeUse,
) -> Result<RemovalPreparation, SagaError> {
    let parent = worktree_path.parent().ok_or_else(|| {
        SagaError::new(
            SagaErrorKind::InvalidInput,
            format!(
                "Worktree path has no parent directory: {}",
                worktree_path.display()
            ),
            "Refresh worktree metadata and retry",
        )
        .with_path(worktree_path)
    })?;
    saga.git
        .prepare_remove(repository, parent, worktree_path, usage)
        .map_err(SagaError::from)
}

fn recheck_identity(
    stored_path: &std::path::Path,
    stored_branch: &str,
    preparation: &RemovalPreparation,
) -> Result<(), SagaError> {
    if preparation.worktree.path != stored_path {
        return Err(SagaError::new(
            SagaErrorKind::InvalidInput,
            "Stored worktree path no longer matches Git's registered identity",
            "Inspect the worktree and reconcile metadata before retrying removal",
        )
        .with_path(stored_path));
    }
    if preparation.worktree.branch.as_deref() != Some(stored_branch) {
        return Err(SagaError::new(
            SagaErrorKind::InvalidInput,
            "Stored worktree branch no longer matches Git's registered identity",
            "Inspect the worktree and reconcile metadata before retrying removal",
        )
        .with_path(stored_path));
    }
    Ok(())
}

fn read_usage<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    session_id: Option<SessionId>,
) -> Result<WorktreeUse, SagaError> {
    let Some(session_id) = session_id else {
        return Ok(WorktreeUse {
            running: false,
            in_use: false,
        });
    };
    let Some(session) = saga.storage().get_session(session_id)? else {
        return Ok(WorktreeUse {
            running: false,
            in_use: false,
        });
    };
    let live = is_live(session.status) || saga.spawner.is_live(session_id);
    Ok(WorktreeUse {
        running: live,
        in_use: live,
    })
}

fn blocked_error(preparation: &RemovalPreparation, path: &std::path::Path) -> SagaError {
    let hidden_or_dirty = preparation.status.is_dirty()
        || !preparation.ignored_paths.is_empty()
        || !preparation.assume_unchanged_paths.is_empty()
        || !preparation.skip_worktree_paths.is_empty();
    let kind = if hidden_or_dirty {
        SagaErrorKind::DirtyWorktree
    } else {
        SagaErrorKind::WorktreeInUse
    };
    SagaError::new(
        kind,
        format!(
            "Worktree removal is blocked: {:?}",
            preparation.blockers
        ),
        "There is no dirty-delete bypass. Commit or stash changes, stop the session, and call prepare_remove again",
    )
    .with_path(path)
}

const fn to_wire_blocker(blocker: RemovalBlocker) -> WorktreeRemovalBlocker {
    match blocker {
        RemovalBlocker::StagedChanges => WorktreeRemovalBlocker::StagedChanges,
        RemovalBlocker::TrackedChanges => WorktreeRemovalBlocker::TrackedChanges,
        RemovalBlocker::UntrackedFiles => WorktreeRemovalBlocker::UntrackedFiles,
        RemovalBlocker::IgnoredFiles => WorktreeRemovalBlocker::IgnoredFiles,
        RemovalBlocker::AssumeUnchanged => WorktreeRemovalBlocker::AssumeUnchanged,
        RemovalBlocker::SkipWorktree => WorktreeRemovalBlocker::SkipWorktree,
        RemovalBlocker::Locked => WorktreeRemovalBlocker::Locked,
        RemovalBlocker::Running => WorktreeRemovalBlocker::Running,
        RemovalBlocker::InUse => WorktreeRemovalBlocker::InUse,
    }
}
