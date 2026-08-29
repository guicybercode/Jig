use rusqlite::params;

use crate::{Storage, StorageError};

/// Persisted project row matching the v1 schema.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectRecord {
    /// Stable identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Canonical repository path.
    pub path: String,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
    /// RFC 3339 last-opened timestamp.
    pub last_opened_at: String,
}

/// Persisted agent definition row matching the v1 schema.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentRecord {
    /// Stable identifier.
    pub id: String,
    /// `built_in` or `custom`.
    pub source: String,
    /// Display name.
    pub name: String,
    /// Executable path or bare name.
    pub executable: String,
    /// JSON array of arguments.
    pub args_json: String,
    /// JSON object of environment overrides.
    pub env_json: String,
    /// Whether the adapter is enabled.
    pub enabled: bool,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
    /// RFC 3339 update timestamp.
    pub updated_at: String,
}

/// Persisted session metadata row matching the v1 schema.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionRecord {
    /// Stable identifier.
    pub id: String,
    /// Owning project.
    pub project_id: String,
    /// Agent used to launch the session.
    pub agent_id: String,
    /// Display name.
    pub name: String,
    /// Process working directory.
    pub cwd: String,
    /// Lifecycle status wire value.
    pub status: String,
    /// Optional OS pid.
    pub runtime_pid: Option<i64>,
    /// Optional daemon instance identifier.
    pub daemon_instance_id: Option<String>,
    /// Optional exit code.
    pub exit_code: Option<i64>,
    /// Optional error code.
    pub error_code: Option<String>,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
    /// RFC 3339 update timestamp.
    pub updated_at: String,
    /// Optional RFC 3339 last-activity timestamp.
    pub last_activity_at: Option<String>,
}

/// Persisted worktree row matching the v1 schema.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorktreeRecord {
    /// Stable identifier.
    pub id: String,
    /// Owning project.
    pub project_id: String,
    /// Optional associated session.
    pub session_id: Option<String>,
    /// Worktree path.
    pub path: String,
    /// Branch name.
    pub branch: String,
    /// Worktree state wire value.
    pub state: String,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
    /// RFC 3339 update timestamp.
    pub updated_at: String,
}

impl Storage {
    /// Inserts a project row.
    ///
    /// # Errors
    ///
    /// Returns an error when `SQLite` rejects the row.
    pub fn insert_project(&self, record: &ProjectRecord) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO projects (id, name, path, created_at, last_opened_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                record.id,
                record.name,
                record.path,
                record.created_at,
                record.last_opened_at
            ],
        )?;
        Ok(())
    }

    /// Loads a project by identifier.
    ///
    /// # Errors
    ///
    /// Returns an error when `SQLite` cannot execute the query.
    pub fn get_project(&self, id: &str) -> Result<Option<ProjectRecord>, StorageError> {
        let result = self.connection.query_row(
            "SELECT id, name, path, created_at, last_opened_at FROM projects WHERE id = ?1",
            params![id],
            |row| {
                Ok(ProjectRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    path: row.get(2)?,
                    created_at: row.get(3)?,
                    last_opened_at: row.get(4)?,
                })
            },
        );
        match result {
            Ok(record) => Ok(Some(record)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Inserts an agent definition row.
    ///
    /// # Errors
    ///
    /// Returns an error when `SQLite` rejects the row.
    pub fn insert_agent(&self, record: &AgentRecord) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO agents (
                id, source, name, executable, args_json, env_json, enabled, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                record.id,
                record.source,
                record.name,
                record.executable,
                record.args_json,
                record.env_json,
                i64::from(u8::from(record.enabled)),
                record.created_at,
                record.updated_at
            ],
        )?;
        Ok(())
    }

    /// Inserts a session metadata row.
    ///
    /// # Errors
    ///
    /// Returns an error when `SQLite` rejects the row.
    pub fn insert_session(&self, record: &SessionRecord) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO sessions (
                id, project_id, agent_id, name, cwd, status, runtime_pid, daemon_instance_id,
                exit_code, error_code, created_at, updated_at, last_activity_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                record.id,
                record.project_id,
                record.agent_id,
                record.name,
                record.cwd,
                record.status,
                record.runtime_pid,
                record.daemon_instance_id,
                record.exit_code,
                record.error_code,
                record.created_at,
                record.updated_at,
                record.last_activity_at
            ],
        )?;
        Ok(())
    }

    /// Loads a session by identifier.
    ///
    /// # Errors
    ///
    /// Returns an error when `SQLite` cannot execute the query.
    pub fn get_session(&self, id: &str) -> Result<Option<SessionRecord>, StorageError> {
        let result = self.connection.query_row(
            "SELECT id, project_id, agent_id, name, cwd, status, runtime_pid, daemon_instance_id,
                    exit_code, error_code, created_at, updated_at, last_activity_at
             FROM sessions WHERE id = ?1",
            params![id],
            |row| {
                Ok(SessionRecord {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    agent_id: row.get(2)?,
                    name: row.get(3)?,
                    cwd: row.get(4)?,
                    status: row.get(5)?,
                    runtime_pid: row.get(6)?,
                    daemon_instance_id: row.get(7)?,
                    exit_code: row.get(8)?,
                    error_code: row.get(9)?,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                    last_activity_at: row.get(12)?,
                })
            },
        );
        match result {
            Ok(record) => Ok(Some(record)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Inserts a worktree row.
    ///
    /// # Errors
    ///
    /// Returns an error when `SQLite` rejects the row.
    pub fn insert_worktree(&self, record: &WorktreeRecord) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO worktrees (
                id, project_id, session_id, path, branch, state, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                record.id,
                record.project_id,
                record.session_id,
                record.path,
                record.branch,
                record.state,
                record.created_at,
                record.updated_at
            ],
        )?;
        Ok(())
    }

    /// Loads a worktree by identifier.
    ///
    /// # Errors
    ///
    /// Returns an error when `SQLite` cannot execute the query.
    pub fn get_worktree(&self, id: &str) -> Result<Option<WorktreeRecord>, StorageError> {
        let result = self.connection.query_row(
            "SELECT id, project_id, session_id, path, branch, state, created_at, updated_at
             FROM worktrees WHERE id = ?1",
            params![id],
            |row| {
                Ok(WorktreeRecord {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    session_id: row.get(2)?,
                    path: row.get(3)?,
                    branch: row.get(4)?,
                    state: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            },
        );
        match result {
            Ok(record) => Ok(Some(record)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Updates a session status after a live process transition.
    ///
    /// # Errors
    ///
    /// Returns an error when `SQLite` rejects the update.
    pub fn update_session_status(
        &self,
        id: &str,
        status: &str,
        runtime_pid: Option<i64>,
        exit_code: Option<i64>,
        updated_at: &str,
    ) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE sessions
             SET status = ?2, runtime_pid = ?3, exit_code = ?4, updated_at = ?5
             WHERE id = ?1",
            params![id, status, runtime_pid, exit_code, updated_at],
        )?;
        Ok(())
    }

    /// Removes a project. Sessions and worktrees that still reference it block
    /// the delete through foreign keys.
    ///
    /// # Errors
    ///
    /// Returns an error when `SQLite` rejects the delete.
    pub fn remove_project(&self, id: &str) -> Result<(), StorageError> {
        self.connection
            .execute("DELETE FROM projects WHERE id = ?1", params![id])?;
        Ok(())
    }
}
