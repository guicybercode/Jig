use cli_master_core::{Session, SessionStatus, Worktree, WorktreeState};
use cli_master_storage::{StoredSession, StoredWorktree, WorktreeState as StoredWorktreeState};

pub(crate) fn core_worktree_state(state: StoredWorktreeState) -> WorktreeState {
    match state {
        StoredWorktreeState::Creating => WorktreeState::Creating,
        StoredWorktreeState::Active => WorktreeState::Active,
        StoredWorktreeState::RemovePending => WorktreeState::RemovePending,
        StoredWorktreeState::Orphaned => WorktreeState::Orphaned,
    }
}

pub(crate) fn session_dto(
    stored: StoredSession,
    worktree: Option<&StoredWorktree>,
    pty_id: Option<String>,
) -> Session {
    Session {
        id: stored.id,
        project_id: stored.project_id,
        name: stored.name,
        agent_id: stored.agent_id,
        cwd: stored.cwd,
        pid: stored.runtime_pid,
        pty_id,
        branch: worktree.map(|item| item.branch.clone()),
        worktree_id: worktree.map(|item| item.id),
        worktree_path: worktree.map(|item| item.path.clone()),
        status: stored.status,
        exit_code: stored.exit_code,
        created_at_ms: stored.created_at_ms,
        updated_at_ms: stored.updated_at_ms,
        last_activity_at_ms: stored.last_activity_at_ms,
        error_code: stored.error_code,
    }
}

pub(crate) fn worktree_dto(stored: StoredWorktree) -> Worktree {
    Worktree {
        id: stored.id,
        project_id: stored.project_id,
        session_id: stored.session_id,
        path: stored.path,
        branch: stored.branch,
        is_dirty: stored.is_dirty,
        state: core_worktree_state(stored.state),
        created_at_ms: stored.created_at_ms,
        updated_at_ms: stored.updated_at_ms,
    }
}

pub(crate) const fn is_live(status: SessionStatus) -> bool {
    matches!(
        status,
        SessionStatus::Starting | SessionStatus::Running | SessionStatus::Idle
    )
}
