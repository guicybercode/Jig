use std::sync::{Mutex, MutexGuard, PoisonError};

use cli_master_core::wire::{ConfirmationToken, SessionIsolation, WorktreePrepareRemoveResponse};
use cli_master_core::{AgentId, Project, ProjectId, SessionId, WorktreeId};
use cli_master_git::Git;
use cli_master_storage::{Storage, StoredAgent};

use crate::lock::{DestinationLocks, MutationGuards};
use crate::token::TokenStore;
use crate::{
    CreateFaults, CreateSession, CreatedSession, RecoveryReport, SagaError, SagaErrorKind,
    SessionSpawner,
};

/// Orchestrates recoverable worktree-backed session creation and removal.
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
    ) -> Result<Self, SagaError> {
        let daemon_instance_id = daemon_instance_id.into();
        if daemon_instance_id.trim().is_empty() {
            return Err(SagaError::new(
                SagaErrorKind::InvalidInput,
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

    /// Creates a session, optionally in a managed worktree.
    ///
    /// # Errors
    ///
    /// Returns an error when validation, persistence, Git, or process launch fails.
    pub fn create_session(&self, request: &CreateSession) -> Result<CreatedSession, SagaError> {
        self.create_session_injected(request, &CreateFaults::default())
    }

    /// Creates a session with test-only fault hooks.
    ///
    /// # Errors
    ///
    /// Returns the same failures as [`Self::create_session`] plus injected faults.
    pub fn create_session_injected(
        &self,
        request: &CreateSession,
        faults: &CreateFaults,
    ) -> Result<CreatedSession, SagaError> {
        if request.name.trim().is_empty() {
            return Err(SagaError::new(
                SagaErrorKind::InvalidInput,
                "session name must not be blank",
                "Provide a user-facing session name",
            ));
        }
        if request.isolation == SessionIsolation::NewWorktree && !request.managed_root.is_absolute()
        {
            return Err(SagaError::new(
                SagaErrorKind::InvalidInput,
                "managed worktree root must be an absolute path",
                "Pass the daemon-owned managed worktree root",
            )
            .with_path(&request.managed_root));
        }
        crate::create::create(self, request, faults)
    }

    /// Inspects whether a managed worktree can be removed and issues a token when safe.
    ///
    /// # Errors
    ///
    /// Returns an error when metadata or Git inspection fails.
    pub fn prepare_remove(
        &self,
        worktree_id: WorktreeId,
    ) -> Result<WorktreePrepareRemoveResponse, SagaError> {
        crate::remove::prepare_remove(self, worktree_id)
    }

    /// Removes a clean unused worktree using a matching confirmation token.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid tokens, blockers, or Git failures.
    pub fn remove_worktree(
        &self,
        worktree_id: WorktreeId,
        confirmation_token: &ConfirmationToken,
    ) -> Result<(), SagaError> {
        crate::remove::remove(self, worktree_id, confirmation_token)
    }

    /// Deletes stopped-session metadata without touching worktree files.
    ///
    /// # Errors
    ///
    /// Returns an error while the session is live or storage fails.
    pub fn delete_session(&self, session_id: SessionId) -> Result<(), SagaError> {
        crate::remove::delete_session(self, session_id)
    }

    /// Records a session process exit in durable metadata.
    ///
    /// # Errors
    ///
    /// Returns an error when the session is missing or storage fails.
    pub fn record_session_exit(
        &self,
        session_id: SessionId,
        exit_code: Option<i32>,
    ) -> Result<(), SagaError> {
        crate::remove::record_session_exit(self, session_id, exit_code)
    }

    /// Removes project metadata without deleting the project directory.
    ///
    /// # Errors
    ///
    /// Returns an error when the project is missing, referenced, or storage fails.
    pub fn remove_project(&self, project_id: ProjectId) -> Result<std::path::PathBuf, SagaError> {
        crate::remove::remove_project(self, project_id)
    }

    /// Reconciles incomplete worktree mutations after a daemon restart.
    ///
    /// # Errors
    ///
    /// Returns an error when durable state cannot be inspected or updated.
    pub fn recover(&self) -> Result<RecoveryReport, SagaError> {
        crate::recover::recover(self)
    }

    pub(crate) fn storage(&self) -> MutexGuard<'_, Storage> {
        self.storage.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub(crate) fn require_project<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    project_id: ProjectId,
) -> Result<Project, SagaError> {
    saga.storage().get_project(project_id)?.ok_or_else(|| {
        SagaError::new(
            SagaErrorKind::NotFound,
            format!("Project metadata was not found for id {project_id}"),
            "Register the project before creating a session",
        )
    })
}

pub(crate) fn require_agent<S: SessionSpawner>(
    saga: &SessionWorktreeSaga<S>,
    agent_id: AgentId,
) -> Result<StoredAgent, SagaError> {
    let agent = saga.storage().get_agent(agent_id)?.ok_or_else(|| {
        SagaError::new(
            SagaErrorKind::NotFound,
            format!("Agent metadata was not found for id {agent_id}"),
            "Choose an enabled agent definition",
        )
    })?;
    if !agent.enabled {
        return Err(SagaError::new(
            SagaErrorKind::InvalidInput,
            format!("Agent {agent_id} is disabled"),
            "Enable the agent before creating a session",
        ));
    }
    Ok(agent)
}
