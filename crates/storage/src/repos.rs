use std::path::{Path, PathBuf};
use std::str::FromStr;

use cli_master_core::{
    AgentId, AgentSource, ProjectId, SessionId, SessionStatus, WorktreeId, WorktreeState,
};
use rusqlite::{OptionalExtension, params};

use crate::records::{AgentRecord, ProjectRecord, SessionRecord, WorktreeRecord};
use crate::time::now_rfc3339;
use crate::{Storage, StorageError};

impl Storage {
    /// Inserts a newly registered project.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite rejects the insert.
    pub fn insert_project(
        &self,
        id: ProjectId,
        name: &str,
        path: &Path,
    ) -> Result<ProjectRecord, StorageError> {
        let now = now_rfc3339();
        let path_text = path_to_string(path)?;
        self.connection.execute(
            "INSERT INTO projects (id, name, path, created_at, last_opened_at)
             VALUES (?1, ?2, ?3, ?4, ?4)",
            params![id.to_string(), name, path_text, now],
        )?;
        self.get_project(id)?
            .ok_or_else(|| StorageError::Invariant("inserted project missing".to_owned()))
    }

    /// Returns every registered project ordered by last opened time.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot load the rows.
    pub fn list_projects(&self) -> Result<Vec<ProjectRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, name, path, created_at, last_opened_at
             FROM projects
             ORDER BY last_opened_at DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(ProjectRecord {
                id: parse_project_id(&row.get::<_, String>(0)?)?,
                name: row.get(1)?,
                path: PathBuf::from(row.get::<_, String>(2)?),
                created_at: row.get(3)?,
                last_opened_at: row.get(4)?,
            })
        })?;
        collect_records(rows)
    }

    /// Returns one project.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot load the row.
    pub fn get_project(&self, id: ProjectId) -> Result<Option<ProjectRecord>, StorageError> {
        self.connection
            .query_row(
                "SELECT id, name, path, created_at, last_opened_at
                 FROM projects WHERE id = ?1",
                params![id.to_string()],
                |row| {
                    Ok(ProjectRecord {
                        id: parse_project_id(&row.get::<_, String>(0)?)?,
                        name: row.get(1)?,
                        path: PathBuf::from(row.get::<_, String>(2)?),
                        created_at: row.get(3)?,
                        last_opened_at: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(StorageError::from)
    }

    /// Updates a project display name.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite rejects the update.
    pub fn rename_project(&self, id: ProjectId, name: &str) -> Result<(), StorageError> {
        let changed = self.connection.execute(
            "UPDATE projects SET name = ?1 WHERE id = ?2",
            params![name, id.to_string()],
        )?;
        if changed == 0 {
            return Err(StorageError::NotFound("project"));
        }
        Ok(())
    }

    /// Updates `last_opened_at` for a project.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite rejects the update.
    pub fn touch_project(&self, id: ProjectId) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE projects SET last_opened_at = ?1 WHERE id = ?2",
            params![now_rfc3339(), id.to_string()],
        )?;
        Ok(())
    }

    /// Deletes project metadata. The repository directory is not touched.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite rejects the delete, including foreign-key
    /// violations when sessions or worktrees still reference the project.
    pub fn delete_project(&self, id: ProjectId) -> Result<(), StorageError> {
        let changed = self.connection.execute(
            "DELETE FROM projects WHERE id = ?1",
            params![id.to_string()],
        )?;
        if changed == 0 {
            return Err(StorageError::NotFound("project"));
        }
        Ok(())
    }

    /// Returns whether a canonical path is already registered.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query the table.
    pub fn project_id_for_path(&self, path: &Path) -> Result<Option<ProjectId>, StorageError> {
        let path_text = path_to_string(path)?;
        let value: Option<String> = self
            .connection
            .query_row(
                "SELECT id FROM projects WHERE path = ?1",
                params![path_text],
                |row| row.get(0),
            )
            .optional()?;
        value
            .as_deref()
            .map(parse_project_id)
            .transpose()
            .map_err(StorageError::from)
    }

    /// Lists every agent definition.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot load the rows.
    pub fn list_agents(&self) -> Result<Vec<AgentRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, source, name, executable, args_json, env_json, enabled, created_at, updated_at
             FROM agents
             ORDER BY source, name",
        )?;
        let rows = statement.query_map([], map_agent_row)?;
        collect_records(rows)
    }

    /// Returns one agent definition.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot load the row.
    pub fn get_agent(&self, id: &AgentId) -> Result<Option<AgentRecord>, StorageError> {
        self.connection
            .query_row(
                "SELECT id, source, name, executable, args_json, env_json, enabled, created_at, updated_at
                 FROM agents WHERE id = ?1",
                params![id.as_str()],
                map_agent_row,
            )
            .optional()
            .map_err(StorageError::from)
    }

    /// Idempotently seeds or refreshes a built-in agent row.
    ///
    /// Existing `enabled` values are preserved.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite rejects the upsert.
    pub fn upsert_builtin_agent(
        &self,
        id: &str,
        name: &str,
        executable: &str,
    ) -> Result<(), StorageError> {
        let now = now_rfc3339();
        self.connection.execute(
            "INSERT INTO agents (
                id, source, name, executable, args_json, env_json, enabled, created_at, updated_at
             ) VALUES (?1, 'built_in', ?2, ?3, '[]', '{}', 1, ?4, ?4)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                executable = excluded.executable,
                args_json = excluded.args_json,
                source = 'built_in',
                updated_at = excluded.updated_at",
            params![id, name, executable, now],
        )?;
        Ok(())
    }

    /// Seeds the four Beta v0.1 built-in agent rows.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite rejects an upsert.
    pub fn seed_builtin_agents(&self) -> Result<(), StorageError> {
        self.upsert_builtin_agent("codex", "Codex", "codex")?;
        self.upsert_builtin_agent("claude", "Claude Code", "claude")?;
        self.upsert_builtin_agent("gemini", "Gemini CLI", "gemini")?;
        self.upsert_builtin_agent("opencode", "OpenCode", "opencode")?;
        Ok(())
    }

    /// Inserts a custom agent definition.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite rejects the insert.
    pub fn insert_custom_agent(
        &self,
        id: &AgentId,
        name: &str,
        executable: &str,
        args_json: &str,
        env_json: &str,
    ) -> Result<AgentRecord, StorageError> {
        let now = now_rfc3339();
        self.connection.execute(
            "INSERT INTO agents (
                id, source, name, executable, args_json, env_json, enabled, created_at, updated_at
             ) VALUES (?1, 'custom', ?2, ?3, ?4, ?5, 1, ?6, ?6)",
            params![id.as_str(), name, executable, args_json, env_json, now],
        )?;
        self.get_agent(id)?
            .ok_or_else(|| StorageError::Invariant("inserted agent missing".to_owned()))
    }

    /// Updates a custom agent definition.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite rejects the update.
    pub fn update_custom_agent(
        &self,
        id: &AgentId,
        name: &str,
        executable: &str,
        args_json: &str,
        env_json: &str,
    ) -> Result<(), StorageError> {
        let changed = self.connection.execute(
            "UPDATE agents
             SET name = ?1, executable = ?2, args_json = ?3, env_json = ?4, updated_at = ?5
             WHERE id = ?6 AND source = 'custom'",
            params![
                name,
                executable,
                args_json,
                env_json,
                now_rfc3339(),
                id.as_str()
            ],
        )?;
        if changed == 0 {
            return Err(StorageError::NotFound("agent"));
        }
        Ok(())
    }

    /// Deletes a custom agent that is not referenced by a session.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite rejects the delete.
    pub fn delete_custom_agent(&self, id: &AgentId) -> Result<(), StorageError> {
        let changed = self.connection.execute(
            "DELETE FROM agents WHERE id = ?1 AND source = 'custom'",
            params![id.as_str()],
        )?;
        if changed == 0 {
            return Err(StorageError::NotFound("agent"));
        }
        Ok(())
    }

    /// Returns whether any session references the agent.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query sessions.
    pub fn agent_in_use(&self, id: &AgentId) -> Result<bool, StorageError> {
        let count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM sessions WHERE agent_id = ?1",
            params![id.as_str()],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Inserts a session row.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite rejects the insert.
    pub fn insert_session(&self, record: &SessionRecord) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO sessions (
                id, project_id, agent_id, name, cwd, status, runtime_pid, daemon_instance_id,
                exit_code, error_code, created_at, updated_at, last_activity_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                record.id.to_string(),
                record.project_id.to_string(),
                record.agent_id.as_str(),
                record.name,
                path_to_string(&record.cwd)?,
                status_to_db(record.status),
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

    /// Lists sessions, optionally filtered by project.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot load the rows.
    pub fn list_sessions(
        &self,
        project_id: Option<ProjectId>,
    ) -> Result<Vec<SessionRecord>, StorageError> {
        if let Some(project_id) = project_id {
            let mut statement = self.connection.prepare(
                "SELECT id, project_id, agent_id, name, cwd, status, runtime_pid, daemon_instance_id,
                        exit_code, error_code, created_at, updated_at, last_activity_at
                 FROM sessions
                 WHERE project_id = ?1
                 ORDER BY updated_at DESC",
            )?;
            let rows = statement.query_map(params![project_id.to_string()], map_session_row)?;
            collect_records(rows)
        } else {
            let mut statement = self.connection.prepare(
                "SELECT id, project_id, agent_id, name, cwd, status, runtime_pid, daemon_instance_id,
                        exit_code, error_code, created_at, updated_at, last_activity_at
                 FROM sessions
                 ORDER BY updated_at DESC",
            )?;
            let rows = statement.query_map([], map_session_row)?;
            collect_records(rows)
        }
    }

    /// Returns one session.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot load the row.
    pub fn get_session(&self, id: SessionId) -> Result<Option<SessionRecord>, StorageError> {
        self.connection
            .query_row(
                "SELECT id, project_id, agent_id, name, cwd, status, runtime_pid, daemon_instance_id,
                        exit_code, error_code, created_at, updated_at, last_activity_at
                 FROM sessions WHERE id = ?1",
                params![id.to_string()],
                map_session_row,
            )
            .optional()
            .map_err(StorageError::from)
    }

    /// Updates session lifecycle fields.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite rejects the update.
    pub fn update_session_runtime(
        &self,
        id: SessionId,
        status: SessionStatus,
        runtime_pid: Option<i64>,
        daemon_instance_id: Option<&str>,
        exit_code: Option<i32>,
        error_code: Option<&str>,
    ) -> Result<(), StorageError> {
        let changed = self.connection.execute(
            "UPDATE sessions
             SET status = ?1, runtime_pid = ?2, daemon_instance_id = ?3,
                 exit_code = ?4, error_code = ?5, updated_at = ?6
             WHERE id = ?7",
            params![
                status_to_db(status),
                runtime_pid,
                daemon_instance_id,
                exit_code,
                error_code,
                now_rfc3339(),
                id.to_string()
            ],
        )?;
        if changed == 0 {
            return Err(StorageError::NotFound("session"));
        }
        Ok(())
    }

    /// Updates a session display name.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite rejects the update.
    pub fn rename_session(&self, id: SessionId, name: &str) -> Result<(), StorageError> {
        let changed = self.connection.execute(
            "UPDATE sessions SET name = ?1, updated_at = ?2 WHERE id = ?3",
            params![name, now_rfc3339(), id.to_string()],
        )?;
        if changed == 0 {
            return Err(StorageError::NotFound("session"));
        }
        Ok(())
    }

    /// Deletes session metadata.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite rejects the delete.
    pub fn delete_session(&self, id: SessionId) -> Result<(), StorageError> {
        let changed = self.connection.execute(
            "DELETE FROM sessions WHERE id = ?1",
            params![id.to_string()],
        )?;
        if changed == 0 {
            return Err(StorageError::NotFound("session"));
        }
        Ok(())
    }

    /// Marks live rows from a different daemon instance as `unknown`.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite rejects the update.
    pub fn reconcile_unknown_sessions(&self, instance_id: &str) -> Result<u32, StorageError> {
        let changed = self.connection.execute(
            "UPDATE sessions
             SET status = 'unknown', runtime_pid = NULL, updated_at = ?1
             WHERE status IN ('starting', 'running', 'idle')
               AND (daemon_instance_id IS NULL OR daemon_instance_id != ?2)",
            params![now_rfc3339(), instance_id],
        )?;
        u32::try_from(changed)
            .map_err(|_| StorageError::Invariant("changed count overflow".to_owned()))
    }

    /// Returns whether any live-looking session references the project.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query the table.
    pub fn project_has_sessions(&self, id: ProjectId) -> Result<bool, StorageError> {
        let count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM sessions WHERE project_id = ?1",
            params![id.to_string()],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Inserts a worktree row.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite rejects the insert.
    pub fn insert_worktree(&self, record: &WorktreeRecord) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO worktrees (
                id, project_id, session_id, path, branch, state, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                record.id.to_string(),
                record.project_id.to_string(),
                record.session_id.map(|id| id.to_string()),
                path_to_string(&record.path)?,
                record.branch,
                worktree_state_to_db(record.state),
                record.created_at,
                record.updated_at
            ],
        )?;
        Ok(())
    }

    /// Lists worktrees for a project, or every worktree when `project_id` is `None`.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot load the rows.
    pub fn list_worktrees(
        &self,
        project_id: Option<ProjectId>,
    ) -> Result<Vec<WorktreeRecord>, StorageError> {
        if let Some(project_id) = project_id {
            let mut statement = self.connection.prepare(
                "SELECT id, project_id, session_id, path, branch, state, created_at, updated_at
                 FROM worktrees
                 WHERE project_id = ?1
                 ORDER BY created_at DESC",
            )?;
            let rows = statement.query_map(params![project_id.to_string()], map_worktree_row)?;
            collect_records(rows)
        } else {
            let mut statement = self.connection.prepare(
                "SELECT id, project_id, session_id, path, branch, state, created_at, updated_at
                 FROM worktrees
                 ORDER BY created_at DESC",
            )?;
            let rows = statement.query_map([], map_worktree_row)?;
            collect_records(rows)
        }
    }

    /// Returns one worktree.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot load the row.
    pub fn get_worktree(&self, id: WorktreeId) -> Result<Option<WorktreeRecord>, StorageError> {
        self.connection
            .query_row(
                "SELECT id, project_id, session_id, path, branch, state, created_at, updated_at
                 FROM worktrees WHERE id = ?1",
                params![id.to_string()],
                map_worktree_row,
            )
            .optional()
            .map_err(StorageError::from)
    }

    /// Returns the worktree attached to a session.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot load the row.
    pub fn worktree_for_session(
        &self,
        session_id: SessionId,
    ) -> Result<Option<WorktreeRecord>, StorageError> {
        self.connection
            .query_row(
                "SELECT id, project_id, session_id, path, branch, state, created_at, updated_at
                 FROM worktrees WHERE session_id = ?1",
                params![session_id.to_string()],
                map_worktree_row,
            )
            .optional()
            .map_err(StorageError::from)
    }

    /// Updates worktree state, path, branch, or session association.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite rejects the update.
    pub fn update_worktree(
        &self,
        id: WorktreeId,
        state: WorktreeState,
        session_id: Option<SessionId>,
    ) -> Result<(), StorageError> {
        let changed = self.connection.execute(
            "UPDATE worktrees
             SET state = ?1, session_id = ?2, updated_at = ?3
             WHERE id = ?4",
            params![
                worktree_state_to_db(state),
                session_id.map(|value| value.to_string()),
                now_rfc3339(),
                id.to_string()
            ],
        )?;
        if changed == 0 {
            return Err(StorageError::NotFound("worktree"));
        }
        Ok(())
    }

    /// Deletes a worktree row.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite rejects the delete.
    pub fn delete_worktree(&self, id: WorktreeId) -> Result<(), StorageError> {
        let changed = self.connection.execute(
            "DELETE FROM worktrees WHERE id = ?1",
            params![id.to_string()],
        )?;
        if changed == 0 {
            return Err(StorageError::NotFound("worktree"));
        }
        Ok(())
    }

    /// Returns whether any worktree references the project.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query the table.
    pub fn project_has_worktrees(&self, id: ProjectId) -> Result<bool, StorageError> {
        let count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM worktrees WHERE project_id = ?1",
            params![id.to_string()],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }
}

fn map_agent_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentRecord> {
    Ok(AgentRecord {
        id: parse_agent_id(&row.get::<_, String>(0)?)?,
        source: parse_agent_source(&row.get::<_, String>(1)?)?,
        name: row.get(2)?,
        executable: row.get(3)?,
        args_json: row.get(4)?,
        env_json: row.get(5)?,
        enabled: row.get::<_, i64>(6)? != 0,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn map_session_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRecord> {
    Ok(SessionRecord {
        id: parse_session_id(&row.get::<_, String>(0)?)?,
        project_id: parse_project_id(&row.get::<_, String>(1)?)?,
        agent_id: parse_agent_id(&row.get::<_, String>(2)?)?,
        name: row.get(3)?,
        cwd: PathBuf::from(row.get::<_, String>(4)?),
        status: parse_session_status(&row.get::<_, String>(5)?)?,
        runtime_pid: row.get(6)?,
        daemon_instance_id: row.get(7)?,
        exit_code: row.get(8)?,
        error_code: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
        last_activity_at: row.get(12)?,
    })
}

fn map_worktree_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorktreeRecord> {
    let session_id = row.get::<_, Option<String>>(2)?;
    Ok(WorktreeRecord {
        id: parse_worktree_id(&row.get::<_, String>(0)?)?,
        project_id: parse_project_id(&row.get::<_, String>(1)?)?,
        session_id: session_id.as_deref().map(parse_session_id).transpose()?,
        path: PathBuf::from(row.get::<_, String>(3)?),
        branch: row.get(4)?,
        state: parse_worktree_state(&row.get::<_, String>(5)?)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn collect_records<T>(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>>,
) -> Result<Vec<T>, StorageError> {
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

fn path_to_string(path: &Path) -> Result<String, StorageError> {
    path.to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| StorageError::InvalidPath(path.to_path_buf()))
}

fn status_to_db(status: SessionStatus) -> &'static str {
    match status {
        SessionStatus::Starting => "starting",
        SessionStatus::Running => "running",
        SessionStatus::Idle => "idle",
        SessionStatus::Exited => "exited",
        SessionStatus::Failed => "failed",
        SessionStatus::Unknown => "unknown",
    }
}

fn worktree_state_to_db(state: WorktreeState) -> &'static str {
    match state {
        WorktreeState::Creating => "creating",
        WorktreeState::Active => "active",
        WorktreeState::RemovePending => "remove_pending",
        WorktreeState::Orphaned => "orphaned",
    }
}

fn parse_project_id(value: &str) -> rusqlite::Result<ProjectId> {
    ProjectId::from_str(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })
}

fn parse_session_id(value: &str) -> rusqlite::Result<SessionId> {
    SessionId::from_str(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })
}

fn parse_worktree_id(value: &str) -> rusqlite::Result<WorktreeId> {
    WorktreeId::from_str(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })
}

fn parse_agent_id(value: &str) -> rusqlite::Result<AgentId> {
    AgentId::from_key(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })
}

fn parse_agent_source(value: &str) -> rusqlite::Result<AgentSource> {
    match value {
        "built_in" => Ok(AgentSource::BuiltIn),
        "custom" => Ok(AgentSource::Custom),
        other => Err(conversion_error(format!("unknown agent source {other}"))),
    }
}

fn parse_session_status(value: &str) -> rusqlite::Result<SessionStatus> {
    match value {
        "starting" => Ok(SessionStatus::Starting),
        "running" => Ok(SessionStatus::Running),
        "idle" => Ok(SessionStatus::Idle),
        "exited" => Ok(SessionStatus::Exited),
        "failed" => Ok(SessionStatus::Failed),
        "unknown" => Ok(SessionStatus::Unknown),
        other => Err(conversion_error(format!("unknown session status {other}"))),
    }
}

fn parse_worktree_state(value: &str) -> rusqlite::Result<WorktreeState> {
    match value {
        "creating" => Ok(WorktreeState::Creating),
        "active" => Ok(WorktreeState::Active),
        "remove_pending" => Ok(WorktreeState::RemovePending),
        "orphaned" => Ok(WorktreeState::Orphaned),
        other => Err(conversion_error(format!("unknown worktree state {other}"))),
    }
}

fn conversion_error(message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, message.into())
}
