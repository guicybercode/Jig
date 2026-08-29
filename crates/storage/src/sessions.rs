//! Session metadata persistence.

use std::str::FromStr;

use cli_master_core::{AgentId, ProjectId, SessionId};
use rusqlite::{Connection, Row, params};

use crate::Storage;
use crate::error::{StorageError, corrupt_data, map_write_error, persisted_validation};
use crate::models::{
    SessionRuntimeUpdate, StoredSession, session_status_from_database, session_status_to_database,
    validate_daemon_instance_id, validate_display_name, validate_timestamp,
};
use crate::paths::{path_from_sql_value, path_to_sql_value};
use crate::values::{optional_timestamp_from_sql_value, timestamp_from_sql_value};

const SESSION_COLUMNS: &str = "id, project_id, agent_id, name, cwd, status, runtime_pid, \
    daemon_instance_id, exit_code, error_code, created_at, updated_at, last_activity_at";

impl Storage {
    /// Inserts a validated session and its initial daemon runtime state.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid metadata, missing project/agent references,
    /// duplicate IDs, or database failures.
    pub fn insert_session(&self, session: &StoredSession) -> Result<(), StorageError> {
        session.validate()?;
        let cwd = path_to_sql_value(&session.cwd, "session cwd")?;
        self.with_connection("insert session", |connection| {
            connection
                .execute(
                    "INSERT INTO sessions (
                    id, project_id, agent_id, name, cwd, status, runtime_pid,
                    daemon_instance_id, exit_code, error_code, created_at,
                    updated_at, last_activity_at
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13
                 )",
                    params![
                        session.id.to_string(),
                        session.project_id.to_string(),
                        session.agent_id.to_string(),
                        session.name,
                        cwd,
                        session_status_to_database(session.status),
                        session.runtime_pid.map(i64::from),
                        session.daemon_instance_id,
                        session.exit_code,
                        session.error_code,
                        session.created_at_ms,
                        session.updated_at_ms,
                        session.last_activity_at_ms,
                    ],
                )
                .map_err(|error| map_write_error(error, "session"))?;
            Ok(())
        })
    }

    /// Lists all sessions, most recently updated first.
    ///
    /// # Errors
    ///
    /// Returns an error if rows cannot be loaded or decoded.
    pub fn list_sessions(&self) -> Result<Vec<StoredSession>, StorageError> {
        self.with_connection("list sessions", list_sessions_from_connection)
    }

    /// Lists sessions owned by one project, most recently updated first.
    ///
    /// # Errors
    ///
    /// Returns an error if rows cannot be loaded or decoded.
    pub fn list_sessions_for_project(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<StoredSession>, StorageError> {
        self.with_connection("list project sessions", |connection| {
            let sql = format!(
                "SELECT {SESSION_COLUMNS} FROM sessions WHERE project_id = ?1
                 ORDER BY CAST(updated_at AS INTEGER) DESC, id"
            );
            let mut statement = connection.prepare(&sql)?;
            let mut rows = statement.query([project_id.to_string()])?;
            collect_sessions(&mut rows)
        })
    }

    /// Loads one session by ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the row cannot be loaded or decoded.
    pub fn get_session(&self, id: SessionId) -> Result<Option<StoredSession>, StorageError> {
        self.with_connection("get session", |connection| {
            let sql = format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1");
            let mut statement = connection.prepare(&sql)?;
            let mut rows = statement.query([id.to_string()])?;
            rows.next()?.map(decode_session).transpose()
        })
    }

    /// Renames a session and updates its metadata timestamp atomically.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid name/timestamp, a missing session, or database failures.
    pub fn rename_session(
        &self,
        id: SessionId,
        name: &str,
        updated_at_ms: i64,
    ) -> Result<(), StorageError> {
        validate_display_name("session name", name)?;
        validate_timestamp("session updated_at_ms", updated_at_ms)?;
        self.with_connection("rename session", |connection| {
            let changed = connection.execute(
                "UPDATE sessions SET name = ?1, updated_at = ?2 WHERE id = ?3",
                params![name, updated_at_ms, id.to_string()],
            )?;
            require_changed(changed, id)
        })
    }

    /// Replaces daemon-owned runtime fields in one atomic statement.
    ///
    /// # Errors
    ///
    /// Returns an error for an inconsistent runtime state, a missing session,
    /// or database failures.
    pub fn update_session_runtime(
        &self,
        id: SessionId,
        update: &SessionRuntimeUpdate,
    ) -> Result<(), StorageError> {
        update.validate()?;
        self.with_connection("update session runtime", |connection| {
            let changed = connection.execute(
                "UPDATE sessions
                 SET status = ?1, runtime_pid = ?2, daemon_instance_id = ?3,
                     exit_code = ?4, error_code = ?5, updated_at = ?6,
                     last_activity_at = ?7
                 WHERE id = ?8",
                params![
                    session_status_to_database(update.status),
                    update.runtime_pid.map(i64::from),
                    update.daemon_instance_id,
                    update.exit_code,
                    update.error_code,
                    update.updated_at_ms,
                    update.last_activity_at_ms,
                    id.to_string(),
                ],
            )?;
            require_changed(changed, id)
        })
    }

    /// Marks live sessions owned by prior daemon instances as unknown.
    ///
    /// This is intended for daemon startup after acquiring the single-instance
    /// lock. It clears stale PIDs and ownership IDs because the new daemon does
    /// not own those processes or PTYs.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid daemon instance ID or timestamp,
    /// or database failures.
    pub fn recover_stale_sessions_for_daemon(
        &self,
        current_daemon_instance_id: &str,
        updated_at_ms: i64,
    ) -> Result<usize, StorageError> {
        validate_daemon_instance_id(current_daemon_instance_id)?;
        validate_timestamp("session updated_at_ms", updated_at_ms)?;
        self.with_connection("recover stale sessions", |connection| {
            Ok(connection.execute(
                "UPDATE sessions
                 SET status = 'unknown', runtime_pid = NULL, daemon_instance_id = NULL,
                     exit_code = NULL, error_code = 'daemon_restarted', updated_at = ?1
                 WHERE status IN ('starting', 'running', 'idle')
                   AND (daemon_instance_id IS NULL OR daemon_instance_id <> ?2)",
                params![updated_at_ms, current_daemon_instance_id],
            )?)
        })
    }

    /// Removes session metadata without terminating a process or deleting files.
    ///
    /// Any associated worktree row is retained and disassociated by its foreign key.
    ///
    /// # Errors
    ///
    /// Returns an error if the session is missing or deletion fails.
    pub fn remove_session_metadata(&self, id: SessionId) -> Result<(), StorageError> {
        self.with_connection("remove session", |connection| {
            let changed =
                connection.execute("DELETE FROM sessions WHERE id = ?1", [id.to_string()])?;
            require_changed(changed, id)
        })
    }
}

pub(crate) fn list_sessions_from_connection(
    connection: &Connection,
) -> Result<Vec<StoredSession>, StorageError> {
    let sql = format!(
        "SELECT {SESSION_COLUMNS} FROM sessions
         ORDER BY CAST(updated_at AS INTEGER) DESC, id"
    );
    let mut statement = connection.prepare(&sql)?;
    let mut rows = statement.query([])?;
    collect_sessions(&mut rows)
}

fn collect_sessions(rows: &mut rusqlite::Rows<'_>) -> Result<Vec<StoredSession>, StorageError> {
    let mut sessions = Vec::new();
    while let Some(row) = rows.next()? {
        sessions.push(decode_session(row)?);
    }
    Ok(sessions)
}

fn decode_session(row: &Row<'_>) -> Result<StoredSession, StorageError> {
    let id_text: String = row.get(0)?;
    let project_id_text: String = row.get(1)?;
    let agent_id_text: String = row.get(2)?;
    let status_text: String = row.get(5)?;
    let raw_pid: Option<i64> = row.get(6)?;
    let session = StoredSession {
        id: SessionId::from_str(&id_text)
            .map_err(|error| corrupt_data("session", "id", error.to_string()))?,
        project_id: ProjectId::from_str(&project_id_text)
            .map_err(|error| corrupt_data("session", "project_id", error.to_string()))?,
        agent_id: AgentId::from_str(&agent_id_text)
            .map_err(|error| corrupt_data("session", "agent_id", error.to_string()))?,
        name: row.get(3)?,
        cwd: path_from_sql_value(row.get_ref(4)?, "session", "cwd")?,
        status: session_status_from_database(&status_text)?,
        runtime_pid: raw_pid
            .map(|pid| {
                u32::try_from(pid)
                    .map_err(|_| corrupt_data("session", "runtime_pid", "must fit in u32"))
            })
            .transpose()?,
        daemon_instance_id: row.get(7)?,
        exit_code: row.get(8)?,
        error_code: row.get(9)?,
        created_at_ms: timestamp_from_sql_value(row.get_ref(10)?, "session", "created_at")?,
        updated_at_ms: timestamp_from_sql_value(row.get_ref(11)?, "session", "updated_at")?,
        last_activity_at_ms: optional_timestamp_from_sql_value(
            row.get_ref(12)?,
            "session",
            "last_activity_at",
        )?,
    };
    persisted_validation("session", session.validate())?;
    Ok(session)
}

fn require_changed(changed: usize, id: SessionId) -> Result<(), StorageError> {
    if changed == 0 {
        Err(StorageError::NotFound {
            entity: "session",
            id: id.to_string(),
        })
    } else {
        Ok(())
    }
}
