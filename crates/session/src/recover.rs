use cli_master_core::WorktreeId;
use cli_master_storage::{StoredWorktree, WorktreeState};

use crate::error::SagaError;
use crate::saga::{SessionWorktreeSaga, require_project};
use crate::spawn::SessionSpawner;
use crate::token::now_ms;

/// Outcome of reconciling durable worktree rows after a daemon restart.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RecoveryReport {
    /// `creating` rows deleted because Git proved the worktree was absent.
    pub dropped_creating: Vec<WorktreeId>,
    /// Rows preserved as `orphaned` because cleanup or completion could not be proven.
    pub orphaned: Vec<WorktreeId>,
    /// `remove_pending` rows restored to `active` because the confirmation token is gone.
    pub restored_active: Vec<WorktreeId>,
}

pub(crate) fn recover<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
) -> Result<RecoveryReport, SagaError> {
    let now = now_ms();
    let worktrees = saga.storage().list_worktrees()?;
    let mut report = RecoveryReport::default();
    for worktree in worktrees {
        match worktree.state {
            WorktreeState::Creating => recover_creating(saga, &worktree, now, &mut report)?,
            WorktreeState::RemovePending => {
                recover_remove_pending(saga, &worktree, now, &mut report)?;
            }
            WorktreeState::Active | WorktreeState::Orphaned => {}
        }
    }
    Ok(report)
}

fn recover_creating<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    worktree: &StoredWorktree,
    now: i64,
    report: &mut RecoveryReport,
) -> Result<(), SagaError> {
    let project = require_project(saga, worktree.project_id)?;
    if matches!(git_proves_absence(saga, &project.path, worktree), Ok(true)) {
        saga.storage().remove_worktree_metadata(worktree.id)?;
        report.dropped_creating.push(worktree.id);
    } else {
        saga.storage().update_worktree_state(
            worktree.id,
            WorktreeState::Orphaned,
            worktree.path.exists(),
            worktree.session_id,
            now,
        )?;
        report.orphaned.push(worktree.id);
    }
    Ok(())
}

fn recover_remove_pending<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    worktree: &StoredWorktree,
    now: i64,
    report: &mut RecoveryReport,
) -> Result<(), SagaError> {
    saga.tokens.discard_for(worktree.id);
    let project = require_project(saga, worktree.project_id)?;
    match git_proves_absence(saga, &project.path, worktree) {
        Ok(true) => {
            saga.storage().update_worktree_state(
                worktree.id,
                WorktreeState::Orphaned,
                false,
                worktree.session_id,
                now,
            )?;
            report.orphaned.push(worktree.id);
        }
        Ok(false) => {
            saga.storage().update_worktree_state(
                worktree.id,
                WorktreeState::Active,
                false,
                worktree.session_id,
                now,
            )?;
            report.restored_active.push(worktree.id);
        }
        Err(_) => {
            saga.storage().update_worktree_state(
                worktree.id,
                WorktreeState::Orphaned,
                true,
                worktree.session_id,
                now,
            )?;
            report.orphaned.push(worktree.id);
        }
    }
    Ok(())
}

fn git_proves_absence<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    repository: &std::path::Path,
    worktree: &StoredWorktree,
) -> Result<bool, SagaError> {
    if worktree.path.exists() {
        return Ok(false);
    }
    let registered = saga.git.list_worktrees(repository)?;
    let still_listed = registered.iter().any(|item| item.path == worktree.path);
    Ok(!still_listed)
}
