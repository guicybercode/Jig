use std::path::{Path, PathBuf};
use std::sync::Arc;

use cli_master_core::wire::{RelativeDirectory, SessionIsolation};
use cli_master_core::{
    AgentId, ProjectId, Session, SessionId, SessionStatus, Worktree, WorktreeId,
};
use cli_master_git::{WorktreePlan, WorktreeUse};
use cli_master_storage::{SessionRuntimeUpdate, StoredSession, StoredWorktree, WorktreeState};

use crate::error::{SagaError, SagaErrorKind};
use crate::lock::lock_destination;
use crate::map::{session_dto, worktree_dto};
use crate::saga::{SessionWorktreeSaga, require_agent, require_project};
use crate::spawn::{SessionSpawner, SpawnRequest};
use crate::token::now_ms;

const DEFAULT_PTY_COLS: u16 = 80;
const DEFAULT_PTY_ROWS: u16 = 24;

#[derive(Clone, Copy)]
pub(crate) enum Launch<'a> {
    Prepare(Option<&'a RelativeDirectory>),
    Start,
}

impl<'a> Launch<'a> {
    fn relative_directory(self) -> Option<&'a RelativeDirectory> {
        match self {
            Self::Prepare(relative) => relative,
            Self::Start => None,
        }
    }

    fn starts_process(self) -> bool {
        matches!(self, Self::Start)
    }
}

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
    /// Persisted session; preparing metadata alone never assigns a pid.
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
    launch: Launch<'_>,
) -> Result<CreatedSession, SagaError> {
    match request.isolation {
        SessionIsolation::Current => create_current(saga, request, faults, launch),
        SessionIsolation::NewWorktree => create_worktree(saga, request, faults, launch),
    }
}

fn create_current<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    request: &CreateSession,
    faults: &CreateFaults,
    launch: Launch<'_>,
) -> Result<CreatedSession, SagaError> {
    let now = now_ms();
    let project = require_project(saga, request.project_id)?;
    let agent = require_agent(saga, request.agent_id)?;
    let cwd = resolve_session_directory(&project.path, launch.relative_directory())?;
    let command = agent
        .command_for_cwd(cwd.clone())
        .map_err(SagaError::from)?;
    maybe_fail(faults, CreateStep::Plan)?;
    maybe_fail(faults, CreateStep::PersistCreating)?;
    maybe_fail(faults, CreateStep::GitAdd)?;
    let session_id = SessionId::new();
    let session = session_row(saga, session_id, request, cwd, now, launch);
    saga.storage().insert_session(&session)?;
    maybe_fail(faults, CreateStep::PersistActive).inspect_err(|_| {
        discard_session(saga, session_id);
    })?;
    if !launch.starts_process() {
        return Ok(CreatedSession {
            session: session_dto(session, None, None),
            worktree: None,
            plan: None,
        });
    }
    let spawned = saga
        .spawner
        .spawn(SpawnRequest {
            session_id,
            project_id: request.project_id,
            agent_id: request.agent_id,
            name: &request.name,
            command: &command,
            branch: None,
            worktree_id: None,
            worktree_path: None,
            cols: DEFAULT_PTY_COLS,
            rows: DEFAULT_PTY_ROWS,
        })
        .inspect_err(|_| discard_session(saga, session_id))?;
    if let Err(error) = maybe_fail(faults, CreateStep::Spawn) {
        return Err(rollback_spawned(saga, session_id, error));
    }
    if let Err(error) = persist_running(saga, session_id, spawned.pid, now) {
        return Err(rollback_spawned(saga, session_id, error));
    }
    if let Err(error) = maybe_fail(faults, CreateStep::PersistRunning) {
        return Err(rollback_spawned(saga, session_id, error));
    }
    let stored = match require_session(saga, session_id) {
        Ok(stored) => stored,
        Err(error) => return Err(rollback_spawned(saga, session_id, error)),
    };
    Ok(CreatedSession {
        session: session_dto(stored, None, spawned.pty_id),
        worktree: None,
        plan: None,
    })
}

fn create_worktree<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    request: &CreateSession,
    faults: &CreateFaults,
    launch: Launch<'_>,
) -> Result<CreatedSession, SagaError> {
    let now = now_ms();
    let project = require_project(saga, request.project_id)?;
    require_agent(saga, request.agent_id)?;
    let worktree_id = WorktreeId::new();
    let session_id = SessionId::new();
    let short_id = request
        .short_id
        .clone()
        .unwrap_or_else(|| short_id_for(worktree_id));
    let plan = saga.git.plan_worktree(
        project.repository_root.as_ref().unwrap_or(&project.path),
        &request.managed_root,
        &request.name,
        &short_id,
    )?;
    let selected_directory = canonical_directory(&project.path)?;
    let project_relative = selected_directory
        .strip_prefix(plan.repository_root())
        .map_err(|_| {
            SagaError::new(
                SagaErrorKind::InvalidInput,
                "selected project directory is outside its Git repository root",
                "Register a directory inside the repository and try again",
            )
            .with_path(&project.path)
        })?;
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
        &plan,
        (worktree_id, session_id, project_relative),
        now,
        launch,
    )
}

fn persist_spawn_and_run<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    request: &CreateSession,
    faults: &CreateFaults,
    plan: &WorktreePlan,
    ids: (WorktreeId, SessionId, &Path),
    now: i64,
    launch: Launch<'_>,
) -> Result<CreatedSession, SagaError> {
    let (worktree_id, session_id, project_relative) = ids;
    let selected_root = resolve_directory(
        plan.destination(),
        &plan.destination().join(project_relative),
    )
    .map_err(|error| compensate(saga, plan, worktree_id, None, error))?;
    let cwd = resolve_session_directory(&selected_root, launch.relative_directory())
        .map_err(|error| compensate(saga, plan, worktree_id, None, error))?;
    let agent = require_agent(saga, request.agent_id)
        .map_err(|error| compensate(saga, plan, worktree_id, None, error))?;
    let command = agent
        .command_for_cwd(cwd.clone())
        .map_err(|error| compensate(saga, plan, worktree_id, None, SagaError::from(error)))?;
    let session = session_row(saga, session_id, request, cwd, now, launch);
    let persisted =
        saga.storage()
            .insert_prepared_session_with_worktree(&session, worktree_id, now);
    if let Err(error) = persisted {
        return Err(compensate(
            saga,
            plan,
            worktree_id,
            None,
            SagaError::from(error),
        ));
    }
    if let Err(error) = maybe_fail(faults, CreateStep::PersistActive) {
        return Err(compensate(saga, plan, worktree_id, Some(session_id), error));
    }

    if !launch.starts_process() {
        let worktree = require_worktree(saga, worktree_id)
            .map_err(|error| compensate(saga, plan, worktree_id, Some(session_id), error))?;
        return Ok(CreatedSession {
            session: session_dto(session, Some(&worktree), None),
            worktree: Some(worktree_dto(worktree)),
            plan: Some(plan.clone()),
        });
    }
    let spawned = match saga.spawner.spawn(SpawnRequest {
        session_id,
        project_id: request.project_id,
        agent_id: request.agent_id,
        name: &request.name,
        command: &command,
        branch: Some(plan.branch()),
        worktree_id: Some(worktree_id),
        worktree_path: Some(plan.destination()),
        cols: DEFAULT_PTY_COLS,
        rows: DEFAULT_PTY_ROWS,
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

    let stored_session = match require_session(saga, session_id) {
        Ok(stored) => stored,
        Err(error) => return Err(compensate(saga, plan, worktree_id, Some(session_id), error)),
    };
    let stored_worktree = match require_worktree(saga, worktree_id) {
        Ok(stored) => stored,
        Err(error) => return Err(compensate(saga, plan, worktree_id, Some(session_id), error)),
    };
    Ok(CreatedSession {
        session: session_dto(stored_session, Some(&stored_worktree), spawned.pty_id),
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

fn session_row<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    session_id: SessionId,
    request: &CreateSession,
    cwd: PathBuf,
    now: i64,
    launch: Launch<'_>,
) -> StoredSession {
    StoredSession {
        id: session_id,
        project_id: request.project_id,
        agent_id: request.agent_id,
        name: request.name.clone(),
        cwd,
        status: if launch.starts_process() {
            SessionStatus::Starting
        } else {
            SessionStatus::Unknown
        },
        runtime_pid: None,
        daemon_instance_id: launch
            .starts_process()
            .then(|| saga.daemon_instance_id.clone()),
        exit_code: None,
        error_code: None,
        created_at_ms: now,
        updated_at_ms: now,
        last_activity_at_ms: launch.starts_process().then_some(now),
    }
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
        mark_orphaned(saga, worktree_id, None);
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
        if let Err(rollback_error) = saga.spawner.rollback(session_id) {
            mark_orphaned(saga, worktree_id, Some(session_id));
            return SagaError::partial_worktree(
                plan.destination(),
                format!("session runtime rollback failed: {rollback_error}"),
            )
            .with_worktree_id(worktree_id)
            .with_session_id(session_id);
        }
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
            // Successful session deletion clears the foreign-key association;
            // do not try to reattach an identifier that no longer exists.
            let remaining_session = saga
                .storage()
                .get_worktree(worktree_id)
                .ok()
                .flatten()
                .and_then(|worktree| worktree.session_id);
            mark_orphaned(saga, worktree_id, remaining_session);
            if error.kind() == cli_master_git::GitErrorKind::PartialWorktree {
                return SagaError::from(error).with_worktree_id(worktree_id);
            }
            SagaError::partial_worktree(plan.destination(), error.message())
                .with_worktree_id(worktree_id)
        }
    }
}

fn rollback_spawned<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    session_id: SessionId,
    original: SagaError,
) -> SagaError {
    match saga.spawner.rollback(session_id) {
        Ok(()) => {
            discard_session(saga, session_id);
            original
        }
        Err(error) => error.with_session_id(session_id),
    }
}

fn discard_worktree<S: SessionSpawner>(saga: &SessionWorktreeSaga<S>, worktree_id: WorktreeId) {
    let _ = saga.storage().remove_worktree_metadata(worktree_id);
}

fn discard_session<S: SessionSpawner>(saga: &SessionWorktreeSaga<S>, session_id: SessionId) {
    let _ = saga.storage().remove_session_metadata(session_id);
}

fn mark_orphaned<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    worktree_id: WorktreeId,
    session_id: Option<SessionId>,
) {
    let _ = saga.storage().update_worktree_state(
        worktree_id,
        WorktreeState::Orphaned,
        true,
        session_id,
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

/// Resolves an existing session directory inside its daemon-selected root.
///
/// Both paths are canonicalized so a child symlink cannot escape the project
/// or managed worktree. The typed relative directory has already passed wire
/// validation for parent traversal and absolute paths.
///
/// # Errors
///
/// Returns an error when either directory is unavailable or the resolved child
/// is outside the canonical root.
pub fn resolve_session_directory(
    root: &Path,
    relative_directory: Option<&RelativeDirectory>,
) -> Result<PathBuf, SagaError> {
    let directory = relative_directory.map_or_else(
        || root.to_path_buf(),
        |relative| root.join(relative.as_str()),
    );
    resolve_directory(root, &directory)
}

fn resolve_directory(root: &Path, directory: &Path) -> Result<PathBuf, SagaError> {
    let canonical_root = canonical_directory(root)?;
    let canonical_directory = canonical_directory(directory)?;
    if !canonical_directory.starts_with(&canonical_root) {
        return Err(SagaError::new(
            SagaErrorKind::InvalidInput,
            "session directory resolves outside its project or worktree root",
            "Choose an existing directory inside the session root",
        )
        .with_path(directory));
    }
    Ok(canonical_directory)
}

fn canonical_directory(directory: &Path) -> Result<PathBuf, SagaError> {
    let resolved = directory.canonicalize().map_err(|error| {
        SagaError::new(
            SagaErrorKind::InvalidInput,
            format!("session directory could not be opened: {error}"),
            "Choose an existing directory inside the session root and check its permissions",
        )
        .with_path(directory)
    })?;
    if !resolved.is_dir() {
        return Err(SagaError::new(
            SagaErrorKind::InvalidInput,
            "session working directory is not a directory",
            "Choose an existing directory inside the session root",
        )
        .with_path(directory));
    }
    Ok(resolved)
}
