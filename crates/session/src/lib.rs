//! Recoverable worktree orchestration and PTY-backed session lifecycle.
//!
//! [`SessionWorktreeSaga`] keeps Git and SQLite effects consistent and
//! delegates process ownership through [`SessionSpawner`]. [`SessionManager`]
//! is the production spawner and remains the only owner of PTY masters,
//! process groups, replay buffers, and in-memory status events.

#![warn(missing_docs)]

#[cfg(not(unix))]
compile_error!("cli-master-session supports Linux and macOS only");

mod buffer;
mod config;
mod create;
mod error;
mod event;
mod lock;
mod manager;
mod map;
mod pty;
mod recover;
mod remove;
mod spawn;
mod token;
mod unix;

pub use config::SessionManagerConfig;
pub use create::{CreateFaults, CreateSession, CreateStep, CreatedSession, LockHook, PlanHook};
pub use error::{SagaError, SagaErrorKind, SessionError, SessionErrorKind};
pub use event::{
    OutputChunk, OutputSnapshot, SessionEvent, SessionSubscription, StatusReason, SubscribeError,
};
pub use manager::{SessionLaunchRequest, SessionManager};
pub use pty::PtySize;
pub use recover::RecoveryReport;
pub use spawn::{FakeSpawner, SessionSpawner, SpawnRequest, SpawnedSession};
pub use token::TOKEN_TTL_MS;

use std::sync::{Mutex, MutexGuard, PoisonError};

use cli_master_core::wire::{ConfirmationToken, SessionIsolation, WorktreePrepareRemoveResponse};
use cli_master_core::{AgentId, Project, ProjectId, SessionId, WorktreeId};
use cli_master_git::Git;
use cli_master_storage::{Storage, StoredAgent};

use crate::lock::{DestinationLocks, MutationGuards};
use crate::token::TokenStore;

/// Orchestrates worktree-backed session creation and two-step removal.
///
/// The generic spawner is a test seam. Production composition supplies a
/// clone of [`SessionManager`], so every created process is registered in the
/// same runtime table and event stream used by `session.start/stop/write`.
pub struct SessionWorktreeSaga<S> {
    pub(crate) git: Git,
    storage: Mutex<Storage>,
    pub(crate) spawner: S,
    pub(crate) daemon_instance_id: String,
    pub(crate) destinations: DestinationLocks,
    pub(crate) mutations: MutationGuards,
    pub(crate) tokens: TokenStore,
}

impl<S: SessionSpawner> SessionWorktreeSaga<S> {
    /// Builds a saga around an already-migrated storage connection.
    ///
    /// # Errors
    ///
    /// Returns an error when `daemon_instance_id` is blank.
    pub fn new(
        git: Git,
        storage: Storage,
        spawner: S,
        daemon_instance_id: impl Into<String>,
    ) -> Result<Self, SessionError> {
        let daemon_instance_id = daemon_instance_id.into();
        if daemon_instance_id.trim().is_empty() {
            return Err(SessionError::new(
                SessionErrorKind::InvalidInput,
                "daemon instance id must not be blank",
                "Pass the current daemon lifetime identifier",
            ));
        }
        Ok(Self {
            git,
            storage: Mutex::new(storage),
            spawner,
            daemon_instance_id,
            destinations: DestinationLocks::default(),
            mutations: MutationGuards::default(),
            tokens: TokenStore::default(),
        })
    }

    /// Creates a session, isolating it in a managed worktree when requested.
    ///
    /// # Errors
    ///
    /// Returns an error when planning, persistence, Git, or spawn fails. A
    /// created worktree is compensated without `--force` when a later step fails.
    pub fn create_session(&self, request: &CreateSession) -> Result<CreatedSession, SessionError> {
        self.create_session_injected(request, &CreateFaults::default())
    }

    /// Creates a session while injecting a failure after a named effect.
    ///
    /// # Errors
    ///
    /// Same as [`Self::create_session`], plus [`SessionErrorKind::InjectedFailure`].
    pub fn create_session_injected(
        &self,
        request: &CreateSession,
        faults: &CreateFaults,
    ) -> Result<CreatedSession, SessionError> {
        if request.name.trim().is_empty() {
            return Err(SessionError::new(
                SessionErrorKind::InvalidInput,
                "session name must not be blank",
                "Provide a user-facing session name",
            ));
        }
        if request.isolation == SessionIsolation::NewWorktree && !request.managed_root.is_absolute()
        {
            return Err(SessionError::new(
                SessionErrorKind::InvalidInput,
                "managed worktree root must be an absolute path",
                "Pass the daemon-owned managed worktree root",
            )
            .with_path(&request.managed_root));
        }
        create::create(self, request, faults)
    }

    /// Re-reads Git identity, dirty state, protections, and session usage.
    ///
    /// A ready result issues a short-lived in-memory confirmation token. There
    /// is no dirty-delete bypass.
    ///
    /// # Errors
    ///
    /// Returns an error when the worktree is missing, a mutation guard is held,
    /// or Git cannot inspect the path.
    pub fn prepare_remove(
        &self,
        worktree_id: WorktreeId,
    ) -> Result<WorktreePrepareRemoveResponse, SessionError> {
        remove::prepare_remove(self, worktree_id)
    }

    /// Removes a clean unused worktree after the matching confirmation token.
    ///
    /// Git is called without `--force`. Metadata is deleted only after Git
    /// removal succeeds. Session rows are left untouched.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid token, dirty or in-use state, or Git failure.
    pub fn remove_worktree(
        &self,
        worktree_id: WorktreeId,
        confirmation_token: &ConfirmationToken,
    ) -> Result<(), SessionError> {
        remove::remove(self, worktree_id, confirmation_token)
    }

    /// Deletes stopped-session metadata without touching a worktree directory.
    ///
    /// # Errors
    ///
    /// Returns [`SessionErrorKind::SessionInUse`] while the session is live.
    pub fn delete_session(&self, session_id: SessionId) -> Result<(), SessionError> {
        remove::delete_session(self, session_id)
    }

    /// Records that a session process exited without deleting worktree files.
    ///
    /// # Errors
    ///
    /// Returns an error if the session is missing or the runtime update is invalid.
    pub fn record_session_exit(
        &self,
        session_id: SessionId,
        exit_code: Option<i32>,
    ) -> Result<(), SessionError> {
        remove::record_session_exit(self, session_id, exit_code)
    }

    /// Deletes project metadata only. The original repository directory is kept.
    ///
    /// # Errors
    ///
    /// Returns an error if the project is missing or still referenced by sessions
    /// or worktrees.
    pub fn remove_project(
        &self,
        project_id: ProjectId,
    ) -> Result<std::path::PathBuf, SessionError> {
        remove::remove_project(self, project_id)
    }

    /// Reconciles `creating` and `remove_pending` rows after a daemon restart.
    ///
    /// In-memory confirmation tokens do not survive this process, so pending
    /// removals cannot complete automatically. Session runtime recovery remains
    /// owned by `Storage::reconcile_sessions` during daemon startup.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot list or update worktree rows.
    pub fn recover(&self) -> Result<RecoveryReport, SessionError> {
        recover::recover(self)
    }

    pub(crate) fn storage(&self) -> MutexGuard<'_, Storage> {
        self.storage.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub(crate) fn require_project<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    project_id: ProjectId,
) -> Result<Project, SessionError> {
    saga.storage().get_project(project_id)?.ok_or_else(|| {
        SessionError::new(
            SessionErrorKind::NotFound,
            format!("Project metadata was not found for id {project_id}"),
            "Register the project before creating a session",
        )
    })
}

pub(crate) fn require_agent<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    agent_id: AgentId,
) -> Result<StoredAgent, SessionError> {
    let agent = saga.storage().get_agent(agent_id)?.ok_or_else(|| {
        SessionError::new(
            SessionErrorKind::NotFound,
            format!("Agent metadata was not found for id {agent_id}"),
            "Choose an enabled agent definition",
        )
    })?;
    if !agent.enabled {
        return Err(SessionError::new(
            SessionErrorKind::InvalidInput,
            format!("Agent {agent_id} is disabled"),
            "Enable the agent before creating a session",
        ));
    }
    Ok(agent)
}
