//! `SQLite` persistence, repositories, and startup reconciliation for CLI Master.
//!
//! This crate owns the database connection, numbered migrations, and metadata
//! repositories. It does not spawn processes, inspect PIDs for liveness, or
//! issue Git commands. Live PTY ownership stays with the session manager; this
//! crate only records what the daemon tells it and reconciles on startup.

#![warn(missing_docs)]

mod connection;
mod error;
mod migrate;
mod paths;
mod records;
mod recovery;
mod repo;
mod secret;
mod serialize;
mod time;

use std::collections::BTreeMap;
use std::path::Path;

use cli_master_core::{AgentId, ProjectId, SessionId, SessionStatus, WorktreeId};
use rusqlite::Connection;
use serde_json::Value;

use crate::connection::{StorageLocation, maybe_backup_before_migrate};
use crate::repo::{
    AgentRepository, ProjectRepository, SessionRepository, SettingsRepository, WorktreeRepository,
};

pub use connection::Storage;
pub use error::{EntityKind, StorageError, StorageErrorKind};
pub use paths::{default_data_dir, default_database_path};
pub use records::{
    NewCustomAgent, NewProject, NewSession, NewWorktree, PathStatus, ReconciliationEvent,
    ReconciliationReason, RecoveryContext, StoredAgent, StoredProject, StoredSession,
    StoredWorktree, WorktreeState,
};
pub use recovery::LiveSessionIndex;

/// The newest schema version understood by this crate.
pub const LATEST_SCHEMA_VERSION: u32 = 2;

#[cfg(test)]
mod contract_tests;

impl Storage {
    /// Opens and configures a file-backed `SQLite` database.
    ///
    /// This does not apply migrations; call [`Self::migrate`] or
    /// [`Self::open_migrated`] before using repositories.
    ///
    /// # Errors
    ///
    /// Returns an error if the parent directory cannot be created or `SQLite`
    /// cannot open or configure the database.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| StorageError::io("open", EntityKind::Database, error))?;
            }
        }
        let connection = Connection::open(&path)
            .map_err(|error| StorageError::from_sqlite("open", EntityKind::Database, &error))?;
        Self::from_connection(connection, StorageLocation::File(path))
    }

    /// Opens a file-backed database, backs up when a destructive migration is
    /// pending, and applies every embedded migration.
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be opened, backed up, or migrated.
    pub fn open_migrated(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let storage = Self::open(path)?;
        storage.prepare()?;
        Ok(storage)
    }

    /// Opens and configures an isolated in-memory `SQLite` database.
    ///
    /// `SQLite` keeps its in-memory journal rather than switching it to WAL.
    ///
    /// # Errors
    ///
    /// Returns an error if `SQLite` cannot create or configure the database.
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let connection = Connection::open_in_memory()
            .map_err(|error| StorageError::from_sqlite("open", EntityKind::Database, &error))?;
        Self::from_connection(connection, StorageLocation::Memory)
    }

    /// Opens an in-memory database and applies migrations.
    ///
    /// # Errors
    ///
    /// Returns an error if the in-memory database cannot be created or migrated.
    pub fn open_in_memory_migrated() -> Result<Self, StorageError> {
        let storage = Self::open_in_memory()?;
        storage.migrate()?;
        Ok(storage)
    }

    fn prepare(&self) -> Result<(), StorageError> {
        self.with_connection_mut("open", |connection| {
            maybe_backup_before_migrate(connection, &self.location)?;
            migrate::migrate(connection)
        })
    }

    /// Applies every pending embedded migration in ascending version order.
    ///
    /// Each migration and its history record are committed in one immediate
    /// transaction. Reapplying migrations is safe and has no effect.
    ///
    /// # Errors
    ///
    /// Returns an error when `SQLite` rejects a migration or when the database
    /// contains an unknown or incompatible schema.
    pub fn migrate(&self) -> Result<(), StorageError> {
        self.with_connection_mut("migrate", migrate::migrate)
    }

    /// Returns the greatest migration version recorded in the database.
    ///
    /// An unmigrated database reports version zero.
    ///
    /// # Errors
    ///
    /// Returns an error if `SQLite` cannot inspect migration history or if the
    /// stored version cannot be represented by this crate.
    pub fn schema_version(&self) -> Result<u32, StorageError> {
        self.with_connection("schema version", migrate::schema_version)
    }

    /// Checkpoints WAL so the on-disk database is consistent, then leaves the
    /// connection open for further use.
    ///
    /// Dropping [`Storage`] also issues a passive WAL checkpoint.
    ///
    /// # Errors
    ///
    /// Returns an error if the lock is poisoned or `SQLite` rejects the checkpoint.
    pub fn close(&self) -> Result<(), StorageError> {
        self.checkpoint()
    }

    /// Returns the file path for a file-backed database.
    #[must_use]
    pub fn database_path(&self) -> Option<&Path> {
        self.file_path()
    }

    /// Registers a project. The directory is never deleted by this method.
    ///
    /// # Errors
    ///
    /// Returns an error if the path is missing, relative, duplicated, or invalid.
    pub fn add_project(&self, new: &NewProject) -> Result<StoredProject, StorageError> {
        self.with_connection("insert", |connection| {
            ProjectRepository::new(connection).insert(new)
        })
    }

    /// Renames a project without touching its directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the project does not exist or the name is empty.
    pub fn rename_project(
        &self,
        id: ProjectId,
        name: impl AsRef<str>,
    ) -> Result<StoredProject, StorageError> {
        self.with_connection("rename", |connection| {
            ProjectRepository::new(connection).rename(id, name.as_ref())
        })
    }

    /// Removes project metadata from the app. Repository files are left intact.
    ///
    /// # Errors
    ///
    /// Returns an error if the project is missing or still referenced by
    /// sessions or worktrees.
    pub fn remove_project(&self, id: ProjectId) -> Result<(), StorageError> {
        self.with_connection("remove", |connection| {
            ProjectRepository::new(connection).remove(id)
        })
    }

    /// Loads one project, reporting whether its path currently exists.
    ///
    /// # Errors
    ///
    /// Returns an error if the project does not exist.
    pub fn get_project(&self, id: ProjectId) -> Result<StoredProject, StorageError> {
        self.with_connection("get", |connection| {
            ProjectRepository::new(connection).get(id)
        })
    }

    /// Lists projects ordered by most recently opened.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub fn list_projects(&self) -> Result<Vec<StoredProject>, StorageError> {
        self.with_connection("list", |connection| {
            ProjectRepository::new(connection).list()
        })
    }

    /// Updates `last_opened_at` for a project.
    ///
    /// # Errors
    ///
    /// Returns an error if the project does not exist.
    pub fn touch_project_opened(&self, id: ProjectId) -> Result<StoredProject, StorageError> {
        self.with_connection("touch", |connection| {
            ProjectRepository::new(connection).touch_opened(id)
        })
    }

    /// Idempotently seeds a built-in agent row.
    ///
    /// # Errors
    ///
    /// Returns an error if the id already belongs to a custom agent or the row
    /// cannot be written.
    pub fn upsert_builtin_agent(
        &self,
        id: AgentId,
        name: impl AsRef<str>,
        executable: impl AsRef<str>,
        args: &[String],
    ) -> Result<StoredAgent, StorageError> {
        self.with_connection("upsert", |connection| {
            AgentRepository::new(connection).upsert_builtin(
                id,
                name.as_ref(),
                executable.as_ref(),
                args,
            )
        })
    }

    /// Registers a custom agent. Secret-like environment keys are rejected.
    ///
    /// # Errors
    ///
    /// Returns an error if validation fails or the id already exists.
    pub fn create_custom_agent(&self, new: &NewCustomAgent) -> Result<StoredAgent, StorageError> {
        self.with_connection("insert", |connection| {
            AgentRepository::new(connection).insert_custom(new)
        })
    }

    /// Updates a custom agent definition.
    ///
    /// # Errors
    ///
    /// Returns an error if the agent is built-in, missing, or invalid.
    pub fn update_custom_agent(
        &self,
        id: AgentId,
        name: impl AsRef<str>,
        executable: impl AsRef<str>,
        args: &[String],
        env: &BTreeMap<String, String>,
    ) -> Result<StoredAgent, StorageError> {
        self.with_connection("update", |connection| {
            AgentRepository::new(connection).update_custom(
                id,
                name.as_ref(),
                executable.as_ref(),
                args,
                env,
            )
        })
    }

    /// Enables or disables an agent without deleting history.
    ///
    /// # Errors
    ///
    /// Returns an error if the agent does not exist.
    pub fn set_agent_enabled(
        &self,
        id: AgentId,
        enabled: bool,
    ) -> Result<StoredAgent, StorageError> {
        self.with_connection("update", |connection| {
            AgentRepository::new(connection).set_enabled(id, enabled)
        })
    }

    /// Deletes a custom agent that is not referenced by sessions.
    ///
    /// # Errors
    ///
    /// Returns an error if the agent is built-in, missing, or still referenced.
    pub fn remove_custom_agent(&self, id: AgentId) -> Result<(), StorageError> {
        self.with_connection("remove", |connection| {
            AgentRepository::new(connection).remove_custom(id)
        })
    }

    /// Loads one agent definition.
    ///
    /// # Errors
    ///
    /// Returns an error if the agent does not exist.
    pub fn get_agent(&self, id: AgentId) -> Result<StoredAgent, StorageError> {
        self.with_connection("get", |connection| AgentRepository::new(connection).get(id))
    }

    /// Lists every stored agent.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub fn list_agents(&self) -> Result<Vec<StoredAgent>, StorageError> {
        self.with_connection("list", |connection| AgentRepository::new(connection).list())
    }

    /// Lists custom agent definitions only.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub fn list_custom_agents(&self) -> Result<Vec<StoredAgent>, StorageError> {
        self.with_connection("list", |connection| {
            AgentRepository::new(connection).list_custom()
        })
    }

    /// Inserts a session in `starting` status before the process is spawned.
    ///
    /// # Errors
    ///
    /// Returns an error if the working directory is missing or related rows do
    /// not exist.
    pub fn create_session(&self, new: &NewSession) -> Result<StoredSession, StorageError> {
        self.with_connection("insert", |connection| {
            SessionRepository::new(connection).insert(new)
        })
    }

    /// Loads one session.
    ///
    /// # Errors
    ///
    /// Returns an error if the session does not exist.
    pub fn get_session(&self, id: SessionId) -> Result<StoredSession, StorageError> {
        self.with_connection("get", |connection| {
            SessionRepository::new(connection).get(id)
        })
    }

    /// Lists every persisted session, newest update first.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub fn list_sessions(&self) -> Result<Vec<StoredSession>, StorageError> {
        self.with_connection("list", |connection| {
            SessionRepository::new(connection).list()
        })
    }

    /// Lists sessions belonging to a project.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub fn list_sessions_for_project(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<StoredSession>, StorageError> {
        self.with_connection("list", |connection| {
            SessionRepository::new(connection).list_for_project(project_id)
        })
    }

    /// Renames a session.
    ///
    /// # Errors
    ///
    /// Returns an error if the session is missing or the name is empty.
    pub fn rename_session(
        &self,
        id: SessionId,
        name: impl AsRef<str>,
    ) -> Result<StoredSession, StorageError> {
        self.with_connection("rename", |connection| {
            SessionRepository::new(connection).rename(id, name.as_ref())
        })
    }

    /// Persists a status transition. Does not inspect OS processes.
    ///
    /// # Errors
    ///
    /// Returns an error if the session does not exist.
    pub fn update_session_status(
        &self,
        id: SessionId,
        status: SessionStatus,
    ) -> Result<StoredSession, StorageError> {
        self.with_connection("update", |connection| {
            SessionRepository::new(connection).update_status(id, status)
        })
    }

    /// Records the Git branch and worktree path for a session.
    ///
    /// # Errors
    ///
    /// Returns an error if the session does not exist or a path is relative.
    pub fn set_session_branch_and_worktree(
        &self,
        id: SessionId,
        branch: Option<&str>,
        worktree_path: Option<&Path>,
    ) -> Result<StoredSession, StorageError> {
        self.with_connection("update", |connection| {
            SessionRepository::new(connection).set_branch_and_worktree(id, branch, worktree_path)
        })
    }

    /// Records that this daemon instance started the session process.
    ///
    /// `pid` is stored only as historical information.
    ///
    /// # Errors
    ///
    /// Returns an error if the session does not exist.
    pub fn record_session_started(
        &self,
        id: SessionId,
        pid: Option<u32>,
        daemon_instance_id: impl AsRef<str>,
    ) -> Result<StoredSession, StorageError> {
        self.with_connection("update", |connection| {
            SessionRepository::new(connection).record_started(id, pid, daemon_instance_id.as_ref())
        })
    }

    /// Records a normal or failed exit without deleting history.
    ///
    /// # Errors
    ///
    /// Returns an error if the session is missing or `status` is not terminal.
    pub fn record_session_exit(
        &self,
        id: SessionId,
        status: SessionStatus,
        exit_code: Option<i32>,
        error_code: Option<&str>,
    ) -> Result<StoredSession, StorageError> {
        self.with_connection("update", |connection| {
            SessionRepository::new(connection).record_exit(id, status, exit_code, error_code)
        })
    }

    /// Deletes session metadata. Worktrees and repository files are kept.
    ///
    /// # Errors
    ///
    /// Returns an error if the session does not exist.
    pub fn delete_session(&self, id: SessionId) -> Result<(), StorageError> {
        self.with_connection("delete", |connection| {
            SessionRepository::new(connection).delete(id)
        })
    }

    /// Inserts a worktree record. This does not create a Git worktree.
    ///
    /// # Errors
    ///
    /// Returns an error if the project is missing or the path/branch conflicts.
    pub fn create_worktree(&self, new: &NewWorktree) -> Result<StoredWorktree, StorageError> {
        self.with_connection("insert", |connection| {
            WorktreeRepository::new(connection).insert(new)
        })
    }

    /// Loads one worktree record.
    ///
    /// # Errors
    ///
    /// Returns an error if the worktree does not exist.
    pub fn get_worktree(&self, id: WorktreeId) -> Result<StoredWorktree, StorageError> {
        self.with_connection("get", |connection| {
            WorktreeRepository::new(connection).get(id)
        })
    }

    /// Lists worktrees for a project.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub fn list_worktrees_for_project(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<StoredWorktree>, StorageError> {
        self.with_connection("list", |connection| {
            WorktreeRepository::new(connection).list_for_project(project_id)
        })
    }

    /// Updates a worktree lifecycle state.
    ///
    /// # Errors
    ///
    /// Returns an error if the worktree does not exist.
    pub fn set_worktree_state(
        &self,
        id: WorktreeId,
        state: WorktreeState,
    ) -> Result<StoredWorktree, StorageError> {
        self.with_connection("update", |connection| {
            WorktreeRepository::new(connection).set_state(id, state)
        })
    }

    /// Associates or clears the session that owns a worktree row.
    ///
    /// # Errors
    ///
    /// Returns an error if the worktree does not exist or the session id conflicts.
    pub fn attach_worktree_session(
        &self,
        id: WorktreeId,
        session_id: Option<SessionId>,
    ) -> Result<StoredWorktree, StorageError> {
        self.with_connection("update", |connection| {
            WorktreeRepository::new(connection).attach_session(id, session_id)
        })
    }

    /// Deletes worktree metadata. Git directories are not removed.
    ///
    /// # Errors
    ///
    /// Returns an error if the worktree does not exist.
    pub fn remove_worktree(&self, id: WorktreeId) -> Result<(), StorageError> {
        self.with_connection("remove", |connection| {
            WorktreeRepository::new(connection).remove(id)
        })
    }

    /// Stores a JSON setting. Token-like keys are rejected.
    ///
    /// # Errors
    ///
    /// Returns an error if the key is forbidden or the write fails.
    pub fn put_setting(&self, key: &str, value: &Value) -> Result<(), StorageError> {
        self.with_connection("put", |connection| {
            SettingsRepository::new(connection).put(key, value)
        })
    }

    /// Loads a JSON setting.
    ///
    /// # Errors
    ///
    /// Returns an error if the stored value is not valid JSON.
    pub fn get_setting(&self, key: &str) -> Result<Option<Value>, StorageError> {
        self.with_connection("get", |connection| {
            SettingsRepository::new(connection).get(key)
        })
    }

    /// Removes a setting row.
    ///
    /// # Errors
    ///
    /// Returns an error if the delete fails.
    pub fn remove_setting(&self, key: &str) -> Result<(), StorageError> {
        self.with_connection("remove", |connection| {
            SettingsRepository::new(connection).remove(key)
        })
    }

    /// Reconciles persisted sessions with the live session manager.
    ///
    /// Rows left `starting`, `running`, or `idle` by a missing process become
    /// `unknown`. Processes are never restarted from this method.
    ///
    /// # Errors
    ///
    /// Returns an error if listing or updating sessions fails.
    pub fn reconcile_sessions(
        &self,
        context: &RecoveryContext<'_>,
    ) -> Result<Vec<ReconciliationEvent>, StorageError> {
        self.with_connection("reconcile", |connection| {
            recovery::reconcile_sessions(connection, context)
        })
    }

    /// Creates a session and its worktree reservation in one transaction.
    ///
    /// # Errors
    ///
    /// Returns an error if either insert fails. Partial writes are rolled back.
    pub fn create_session_with_worktree(
        &self,
        session: &NewSession,
        worktree: &NewWorktree,
    ) -> Result<(StoredSession, StoredWorktree), StorageError> {
        self.transaction(|transaction| {
            let stored_session = SessionRepository::new(transaction).insert(session)?;
            let stored_worktree = WorktreeRepository::new(transaction).insert(worktree)?;
            Ok((stored_session, stored_worktree))
        })
    }
}

#[cfg(test)]
pub(crate) fn connection_for_tests(
    storage: &Storage,
) -> std::sync::MutexGuard<'_, rusqlite::Connection> {
    storage
        .connection
        .lock()
        .expect("storage lock should not be poisoned")
}
