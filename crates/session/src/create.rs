use std::path::PathBuf;
use std::sync::Arc;

use cli_master_core::wire::SessionIsolation;
use cli_master_core::{
    AgentId, ProjectId, Session, SessionId, SessionStatus, Worktree, WorktreeId,
};
use cli_master_git::{WorktreePlan, WorktreeUse};
use cli_master_storage::{SessionRuntimeUpdate, StoredSession, StoredWorktree, WorktreeState};

use crate::error::{SagaError, SagaErrorKind};
use crate::lock::lock_destination;
use crate::map::{session_dto, worktree_dto};
use crate::spawn::{SessionSpawner, SpawnRequest};
use crate::token::now_ms;
use crate::{SessionWorktreeSaga, require_agent, require_project};

/// Named saga effect after which tests may inject a failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreateStep {
    /// `git.plan_worktree` completed with an exact OID and destination.
    Plan,
    /// The `creating` worktree row was persisted.
    PersistCreating,
    /// `git.create_worktree_from_plan` completed.
    GitAdd,
    /// Session `starting` and worktree `active` rows were persisted.
    PersistActive,
    /// The session spawner returned successfully.
    Spawn,
    /// Session runtime was persisted as `running`.
    PersistRunning,
}

/// Test-only hooks that fire after durable saga effects.
#[derive(Clone, Default)]
pub struct CreateFaults {
    /// Abort after this effect and run compensation if a worktree exists.
    pub fail_after: Option<CreateStep>,
    /// Called after a side-effect-free plan is produced.
    pub after_plan: Option<PlanHook>,
    /// Called after the destination lock is held.
    pub after_lock: Option<LockHook>,
    /// Called after Git has created the planned worktree.
    pub after_git_add: Option<PlanHook>,
}

/// Callback invoked with a planned worktree.
pub type PlanHook = Arc<dyn Fn(&WorktreePlan) + Send + Sync>;
/// Callback invoked while a destination lock is held.
pub type LockHook = Arc<dyn Fn() + Send + Sync>;

impl std::fmt::Debug for CreateFaults {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CreateFaults")
            .field("fail_after", &self.fail_after)
            .finish_non_exhaustive()
    }
}

/// Inputs for creating a session, optionally in an isolated worktree.
#[derive(Clone, Debug)]
pub struct CreateSession {
    /// Project whose repository root is the Git source of truth.
    pub project_id: ProjectId,
    /// Agent definition used to build `CommandSpec`.
    pub agent_id: AgentId,
    /// User-facing session name; also the Git branch slug input.
    pub name: String,
    /// Whether to isolate into a new managed worktree.
    pub isolation: SessionIsolation,
    /// Directory that must contain the generated worktree path.
    pub managed_root: PathBuf,
    /// Optional override so tests can force a colliding destination.
    pub short_id: Option<String>,
}

/// Durable result of a completed create saga.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreatedSession {
    /// Persisted session, including daemon-observed pid after spawn.
    pub session: Session,
    /// Managed worktree when isolation requested one.
    pub worktree: Option<Worktree>,
    /// Exact plan used for Git creation, when a worktree was created.
    pub plan: Option<WorktreePlan>,
}

pub(crate) fn create<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    request: &CreateSession,
    faults: &CreateFaults,
) -> Result<CreatedSession, SagaError> {
    match request.isolation {
        SessionIsolation::Current => create_current(saga, request, faults),
        SessionIsolation::NewWorktree => create_worktree(saga, request, faults),
    }
}

fn create_current<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    request: &CreateSession,
    faults: &CreateFaults,
) -> Result<CreatedSession, SagaError> {
    let now = now_ms();
    let project = require_project(saga, request.project_id)?;
    let agent = require_agent(saga, request.agent_id)?;
    maybe_fail(faults, CreateStep::Plan)?;
    maybe_fail(faults, CreateStep::PersistCreating)?;
    maybe_fail(faults, CreateStep::GitAdd)?;
    let session_id = SessionId::new();
    insert_starting_session(saga, session_id, request, project.path.clone(), now)?;
    maybe_fail(faults, CreateStep::PersistActive).inspect_err(|_| {
        discard_session(saga, session_id);
    })?;
    let command = agent
        .command_for_cwd(project.path.clone())
        .map_err(SagaError::from)?;
    let spawned = saga
        .spawner
        .spawn(SpawnRequest {
            session_id,
            command: &command,
        })
        .inspect_err(|_| discard_session(saga, session_id))?;
    maybe_fail(faults, CreateStep::Spawn).inspect_err(|_| {
        discard_session(saga, session_id);
    })?;
    persist_running(saga, session_id, spawned.pid, now)?;
    maybe_fail(faults, CreateStep::PersistRunning).inspect_err(|_| {
        discard_session(saga, session_id);
    })?;
    let stored = require_session(saga, session_id)?;
    Ok(CreatedSession {
        session: session_dto(stored, None),
        worktree: None,
        plan: None,
    })
}

fn create_worktree<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    request: &CreateSession,
    faults: &CreateFaults,
) -> Result<CreatedSession, SagaError> {
    let now = now_ms();
    let project = require_project(saga, request.project_id)?;
    let agent = require_agent(saga, request.agent_id)?;
    let worktree_id = WorktreeId::new();
    let session_id = SessionId::new();
    let short_id = request
        .short_id
        .clone()
        .unwrap_or_else(|| short_id_for(worktree_id));
    let plan = saga.git.plan_worktree(
        &project.path,
        &request.managed_root,
        &request.name,
        &short_id,
    )?;
    if let Some(hook) = &faults.after_plan {
        hook(&plan);
    }
    maybe_fail(faults, CreateStep::Plan)?;

    let _destination = lock_destination(&saga.destinations, plan.destination().to_path_buf())?;
    if let Some(hook) = &faults.after_lock {
        hook();
    }

    persist_creating(saga, worktree_id, request, &plan, now)?;
    if let Err(error) = maybe_fail(faults, CreateStep::PersistCreating) {
        discard_worktree(saga, worktree_id);
        return Err(error);
    }

    let created = match saga.git.create_worktree_from_plan(&plan) {
        Ok(created) => created,
        Err(error) => {
            return Err(after_git_create_failure(saga, worktree_id, &plan, error));
        }
    };
    let _ = created;
    if let Some(hook) = &faults.after_git_add {
        hook(&plan);
    }
    if let Err(error) = maybe_fail(faults, CreateStep::GitAdd) {
        return Err(compensate(saga, &plan, worktree_id, None, error));
    }
    persist_spawn_and_run(
        saga,
        request,
        faults,
        &agent,
        &plan,
        (worktree_id, session_id),
        now,
    )
}

fn persist_spawn_and_run<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    request: &CreateSession,
    faults: &CreateFaults,
    agent: &cli_master_storage::StoredAgent,
    plan: &WorktreePlan,
    ids: (WorktreeId, SessionId),
    now: i64,
) -> Result<CreatedSession, SagaError> {
    let (worktree_id, session_id) = ids;
    if let Err(error) = insert_starting_session(
        saga,
        session_id,
        request,
        plan.destination().to_path_buf(),
        now,
    ) {
        return Err(compensate(saga, plan, worktree_id, None, error));
    }
    if let Err(error) = activate_worktree(saga, worktree_id, session_id, now) {
        discard_session(saga, session_id);
        return Err(compensate(saga, plan, worktree_id, None, error));
    }
    if let Err(error) = maybe_fail(faults, CreateStep::PersistActive) {
        discard_session(saga, session_id);
        return Err(compensate(saga, plan, worktree_id, Some(session_id), error));
    }

    let command = agent
        .command_for_cwd(plan.destination().to_path_buf())
        .map_err(SagaError::from)?;
    let spawned = match saga.spawner.spawn(SpawnRequest {
        session_id,
        command: &command,
    }) {
        Ok(spawned) => spawned,
        Err(error) => {
            return Err(compensate(saga, plan, worktree_id, Some(session_id), error));
        }
    };
    if let Err(error) = maybe_fail(faults, CreateStep::Spawn) {
        return Err(compensate(saga, plan, worktree_id, Some(session_id), error));
    }
    if let Err(error) = persist_running(saga, session_id, spawned.pid, now) {
        return Err(compensate(saga, plan, worktree_id, Some(session_id), error));
    }
    if let Err(error) = maybe_fail(faults, CreateStep::PersistRunning) {
        return Err(compensate(saga, plan, worktree_id, Some(session_id), error));
    }

    let stored_session = require_session(saga, session_id)?;
    let stored_worktree = require_worktree(saga, worktree_id)?;
    Ok(CreatedSession {
        session: session_dto(stored_session, Some(&stored_worktree)),
        worktree: Some(worktree_dto(stored_worktree)),
        plan: Some(plan.clone()),
    })
}

fn persist_creating<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    worktree_id: WorktreeId,
    request: &CreateSession,
    plan: &WorktreePlan,
    now: i64,
) -> Result<(), SagaError> {
    let row = StoredWorktree {
        id: worktree_id,
        project_id: request.project_id,
        session_id: None,
        path: plan.destination().to_path_buf(),
        branch: plan.branch().to_owned(),
        state: WorktreeState::Creating,
        is_dirty: false,
        created_at_ms: now,
        updated_at_ms: now,
    };
    saga.storage()
        .insert_worktree(&row)
        .map_err(SagaError::from)
}

fn insert_starting_session<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    session_id: SessionId,
    request: &CreateSession,
    cwd: PathBuf,
    now: i64,
) -> Result<(), SagaError> {
    let row = StoredSession {
        id: session_id,
        project_id: request.project_id,
        agent_id: request.agent_id,
        name: request.name.clone(),
        cwd,
        status: SessionStatus::Starting,
        runtime_pid: None,
        daemon_instance_id: Some(saga.daemon_instance_id.clone()),
        exit_code: None,
        error_code: None,
        created_at_ms: now,
        updated_at_ms: now,
        last_activity_at_ms: Some(now),
    };
    saga.storage().insert_session(&row).map_err(SagaError::from)
}

fn activate_worktree<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    worktree_id: WorktreeId,
    session_id: SessionId,
    now: i64,
) -> Result<(), SagaError> {
    saga.storage()
        .update_worktree_state(
            worktree_id,
            WorktreeState::Active,
            false,
            Some(session_id),
            now,
        )
        .map_err(SagaError::from)
}

fn persist_running<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    session_id: SessionId,
    pid: u32,
    now: i64,
) -> Result<(), SagaError> {
    saga.storage()
        .update_session_runtime(
            session_id,
            &SessionRuntimeUpdate {
                status: SessionStatus::Running,
                runtime_pid: Some(pid),
                daemon_instance_id: Some(saga.daemon_instance_id.clone()),
                exit_code: None,
                error_code: None,
                last_activity_at_ms: Some(now),
                updated_at_ms: now,
            },
        )
        .map_err(SagaError::from)
}

fn after_git_create_failure<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    worktree_id: WorktreeId,
    plan: &WorktreePlan,
    error: cli_master_git::GitError,
) -> SagaError {
    let saga_error = SagaError::from(error);
    if saga_error.kind() == SagaErrorKind::PartialWorktree {
        mark_orphaned(saga, worktree_id);
        return saga_error.with_worktree_id(worktree_id);
    }
    discard_worktree(saga, worktree_id);
    let _ = plan;
    saga_error
}

fn compensate<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    plan: &WorktreePlan,
    worktree_id: WorktreeId,
    session_id: Option<SessionId>,
    original: SagaError,
) -> SagaError {
    if let Some(session_id) = session_id {
        discard_session(saga, session_id);
    }
    match saga.git.remove_worktree(
        plan.repository_root(),
        plan.managed_root(),
        plan.destination(),
        || WorktreeUse {
            running: false,
            in_use: false,
        },
    ) {
        Ok(()) => {
            discard_worktree(saga, worktree_id);
            original
        }
        Err(error) => {
            mark_orphaned(saga, worktree_id);
            if error.kind() == cli_master_git::GitErrorKind::PartialWorktree {
                return SagaError::from(error).with_worktree_id(worktree_id);
            }
            SagaError::partial_worktree(plan.destination(), error.message())
                .with_worktree_id(worktree_id)
        }
    }
}

fn discard_worktree<S: SessionSpawner>(saga: &SessionWorktreeSaga<S>, worktree_id: WorktreeId) {
    let _ = saga.storage().remove_worktree_metadata(worktree_id);
}

fn discard_session<S: SessionSpawner>(saga: &SessionWorktreeSaga<S>, session_id: SessionId) {
    let _ = saga.storage().remove_session_metadata(session_id);
}

fn mark_orphaned<S: SessionSpawner>(saga: &SessionWorktreeSaga<S>, worktree_id: WorktreeId) {
    let _ = saga.storage().update_worktree_state(
        worktree_id,
        WorktreeState::Orphaned,
        true,
        None,
        now_ms(),
    );
}

fn maybe_fail(faults: &CreateFaults, step: CreateStep) -> Result<(), SagaError> {
    if faults.fail_after == Some(step) {
        Err(SagaError::injected(step))
    } else {
        Ok(())
    }
}

fn short_id_for(worktree_id: WorktreeId) -> String {
    let hex = worktree_id.as_uuid().simple().to_string();
    hex.chars().take(12).collect()
}

pub(crate) fn require_session<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    session_id: SessionId,
) -> Result<StoredSession, SagaError> {
    saga.storage().get_session(session_id)?.ok_or_else(|| {
        SagaError::new(
            SagaErrorKind::NotFound,
            format!("Session metadata was not found for id {session_id}"),
            "Refresh sessions and retry",
        )
        .with_session_id(session_id)
    })
}

pub(crate) fn require_worktree<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    worktree_id: WorktreeId,
) -> Result<StoredWorktree, SagaError> {
    saga.storage().get_worktree(worktree_id)?.ok_or_else(|| {
        SagaError::new(
            SagaErrorKind::NotFound,
            format!("Worktree metadata was not found for id {worktree_id}"),
            "Refresh worktrees and retry",
        )
        .with_worktree_id(worktree_id)
    })
}
