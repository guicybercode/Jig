use std::path::PathBuf;

use cli_master_core::{ProjectId, Session, SessionId, SessionStatus};
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::error::{EntityKind, StorageError};
use crate::records::{NewSession, StoredSession};
use crate::serialize::{
    absolute_path, required_text, session_status_from_db, session_status_to_db,
};
use crate::time::{now_rfc3339, rfc3339_to_unix_ms};

pub(crate) struct SessionRepository<'a> {
    connection: &'a Connection,
}

impl<'a> SessionRepository<'a> {
    pub(crate) const fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub(crate) fn insert(&self, new: &NewSession) -> Result<StoredSession, StorageError> {
        required_text(&new.name, "name", EntityKind::Session)?;
        let cwd = absolute_path(&new.cwd, EntityKind::Session)?;
        if !cwd.exists() {
            return Err(StorageError::path_missing(
                "insert",
                EntityKind::Session,
                &cwd,
            ));
        }
        let branch = new
            .branch
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let worktree_path = new
            .worktree_path
            .as_deref()
            .map(|path| absolute_path(path, EntityKind::Session))
            .transpose()?;
        let timestamp = now_rfc3339()?;

        self.connection
            .execute(
                "INSERT INTO sessions (
                    id, project_id, agent_id, name, cwd, status,
                    branch, worktree_path, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, 'starting', ?6, ?7, ?8, ?8)",
                params![
                    new.id.to_string(),
                    new.project_id.to_string(),
                    new.agent_id.to_string(),
                    new.name.trim(),
                    cwd.to_string_lossy(),
                    branch,
                    worktree_path
                        .as_ref()
                        .map(|path| path.to_string_lossy().into_owned()),
                    timestamp
                ],
            )
            .map_err(|error| {
                crate::repo::remap_constraint(
                    StorageError::from_sqlite("insert", EntityKind::Session, &error),
                    StorageError::conflict(
                        "insert",
                        EntityKind::Session,
                        "session could not be created because a related record is missing or duplicated",
                        "Confirm the project and agent still exist, then retry.",
                    ),
                )
            })?;
        self.get(new.id)
    }

    pub(crate) fn get(&self, id: SessionId) -> Result<StoredSession, StorageError> {
        self.connection
            .query_row(
                &session_select("WHERE sessions.id = ?1"),
                params![id.to_string()],
                map_session_row,
            )
            .optional()
            .map_err(|error| StorageError::from_sqlite("get", EntityKind::Session, &error))?
            .ok_or_else(|| StorageError::not_found("get", EntityKind::Session, id))
    }

    pub(crate) fn list(&self) -> Result<Vec<StoredSession>, StorageError> {
        self.query_list(&session_select("ORDER BY sessions.updated_at DESC"), [])
    }

    pub(crate) fn list_for_project(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<StoredSession>, StorageError> {
        self.query_list(
            &session_select("WHERE sessions.project_id = ?1 ORDER BY sessions.updated_at DESC"),
            params![project_id.to_string()],
        )
    }

    pub(crate) fn rename(&self, id: SessionId, name: &str) -> Result<StoredSession, StorageError> {
        required_text(name, "name", EntityKind::Session)?;
        self.touch_update(
            id,
            "UPDATE sessions SET name = ?1, updated_at = ?2 WHERE id = ?3",
            params![name.trim(), now_rfc3339()?, id.to_string()],
        )
    }

    pub(crate) fn update_status(
        &self,
        id: SessionId,
        status: SessionStatus,
    ) -> Result<StoredSession, StorageError> {
        self.touch_update(
            id,
            "UPDATE sessions SET status = ?1, updated_at = ?2, last_activity_at = ?2 WHERE id = ?3",
            params![session_status_to_db(status), now_rfc3339()?, id.to_string()],
        )
    }

    pub(crate) fn set_branch_and_worktree(
        &self,
        id: SessionId,
        branch: Option<&str>,
        worktree_path: Option<&std::path::Path>,
    ) -> Result<StoredSession, StorageError> {
        let branch = branch.map(str::trim).filter(|value| !value.is_empty());
        let worktree_path = worktree_path
            .map(|path| absolute_path(path, EntityKind::Session))
            .transpose()?;
        self.touch_update(
            id,
            "UPDATE sessions SET branch = ?1, worktree_path = ?2, updated_at = ?3 WHERE id = ?4",
            params![
                branch,
                worktree_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned()),
                now_rfc3339()?,
                id.to_string()
            ],
        )
    }

    pub(crate) fn record_started(
        &self,
        id: SessionId,
        pid: Option<u32>,
        daemon_instance_id: &str,
    ) -> Result<StoredSession, StorageError> {
        required_text(daemon_instance_id, "key", EntityKind::Session)?;
        let timestamp = now_rfc3339()?;
        self.touch_update(
            id,
            "UPDATE sessions
             SET status = 'running',
                 runtime_pid = ?1,
                 daemon_instance_id = ?2,
                 started_at = COALESCE(started_at, ?3),
                 updated_at = ?3,
                 last_activity_at = ?3
             WHERE id = ?4",
            params![
                pid.map(i64::from),
                daemon_instance_id,
                timestamp,
                id.to_string()
            ],
        )
    }

    pub(crate) fn record_exit(
        &self,
        id: SessionId,
        status: SessionStatus,
        exit_code: Option<i32>,
        error_code: Option<&str>,
    ) -> Result<StoredSession, StorageError> {
        if !matches!(status, SessionStatus::Exited | SessionStatus::Failed) {
            return Err(StorageError::invalid_input(
                "record exit",
                EntityKind::Session,
                "exit status must be exited or failed",
            ));
        }
        let timestamp = now_rfc3339()?;
        self.touch_update(
            id,
            "UPDATE sessions
             SET status = ?1,
                 exit_code = ?2,
                 error_code = ?3,
                 exited_at = ?4,
                 updated_at = ?4
             WHERE id = ?5",
            params![
                session_status_to_db(status),
                exit_code.map(i64::from),
                error_code,
                timestamp,
                id.to_string()
            ],
        )
    }

    pub(crate) fn mark_unknown(&self, id: SessionId) -> Result<StoredSession, StorageError> {
        self.touch_update(
            id,
            "UPDATE sessions SET status = 'unknown', updated_at = ?1 WHERE id = ?2",
            params![now_rfc3339()?, id.to_string()],
        )
    }

    pub(crate) fn delete(&self, id: SessionId) -> Result<(), StorageError> {
        let deleted = self
            .connection
            .execute(
                "DELETE FROM sessions WHERE id = ?1",
                params![id.to_string()],
            )
            .map_err(|error| StorageError::from_sqlite("delete", EntityKind::Session, &error))?;
        if deleted == 0 {
            return Err(StorageError::not_found("delete", EntityKind::Session, id));
        }
        Ok(())
    }

    fn touch_update(
        &self,
        id: SessionId,
        sql: &str,
        params: impl rusqlite::Params,
    ) -> Result<StoredSession, StorageError> {
        let updated = self
            .connection
            .execute(sql, params)
            .map_err(|error| StorageError::from_sqlite("update", EntityKind::Session, &error))?;
        if updated == 0 {
            return Err(StorageError::not_found("update", EntityKind::Session, id));
        }
        self.get(id)
    }

    fn query_list(
        &self,
        sql: &str,
        params: impl rusqlite::Params,
    ) -> Result<Vec<StoredSession>, StorageError> {
        let mut statement = self
            .connection
            .prepare(sql)
            .map_err(|error| StorageError::from_sqlite("list", EntityKind::Session, &error))?;
        let rows = statement
            .query_map(params, map_session_row)
            .map_err(|error| StorageError::from_sqlite("list", EntityKind::Session, &error))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| StorageError::from_sqlite("list", EntityKind::Session, &error))
    }
}

fn session_select(suffix: &str) -> String {
    format!(
        "SELECT
            sessions.id,
            sessions.project_id,
            sessions.agent_id,
            sessions.name,
            sessions.cwd,
            sessions.status,
            sessions.runtime_pid,
            sessions.daemon_instance_id,
            sessions.exit_code,
            sessions.error_code,
            sessions.created_at,
            sessions.updated_at,
            sessions.started_at,
            sessions.exited_at,
            sessions.branch,
            sessions.worktree_path,
            worktrees.id AS worktree_id
         FROM sessions
         LEFT JOIN worktrees ON worktrees.session_id = sessions.id
         {suffix}"
    )
}

fn map_session_row(row: &Row<'_>) -> rusqlite::Result<StoredSession> {
    let id: String = row.get("id")?;
    let project_id: String = row.get("project_id")?;
    let agent_id: String = row.get("agent_id")?;
    let status: String = row.get("status")?;
    let created_at: String = row.get("created_at")?;
    let updated_at: String = row.get("updated_at")?;
    let started_at: Option<String> = row.get("started_at")?;
    let exited_at: Option<String> = row.get("exited_at")?;
    let worktree_id: Option<String> = row.get("worktree_id")?;
    let pid: Option<i64> = row.get("runtime_pid")?;
    let exit_code: Option<i64> = row.get("exit_code")?;

    Ok(StoredSession {
        session: Session {
            id: parse_id(&id)?,
            project_id: parse_id(&project_id)?,
            name: row.get("name")?,
            agent_id: parse_id(&agent_id)?,
            cwd: PathBuf::from(row.get::<_, String>("cwd")?),
            pid: pid.map(pid_from_db).transpose()?,
            pty_id: None,
            branch: row.get("branch")?,
            worktree_id: worktree_id.as_deref().map(parse_id).transpose()?,
            worktree_path: row
                .get::<_, Option<String>>("worktree_path")?
                .map(PathBuf::from),
            status: session_status_from_db(&status),
            exit_code: exit_code.map(exit_code_from_db).transpose()?,
            created_at_ms: rfc3339_to_unix_ms(&created_at).map_err(conversion_error)?,
            updated_at_ms: rfc3339_to_unix_ms(&updated_at).map_err(conversion_error)?,
            started_at_ms: started_at
                .as_deref()
                .map(rfc3339_to_unix_ms)
                .transpose()
                .map_err(conversion_error)?,
            exited_at_ms: exited_at
                .as_deref()
                .map(rfc3339_to_unix_ms)
                .transpose()
                .map_err(conversion_error)?,
        },
        daemon_instance_id: row.get("daemon_instance_id")?,
        error_code: row.get("error_code")?,
    })
}

fn parse_id<T>(value: &str) -> rusqlite::Result<T>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    value.parse().map_err(conversion_error)
}

fn pid_from_db(value: i64) -> rusqlite::Result<u32> {
    u32::try_from(value).map_err(conversion_error)
}

fn exit_code_from_db(value: i64) -> rusqlite::Result<i32> {
    i32::try_from(value).map_err(conversion_error)
}

fn conversion_error<E>(error: E) -> rusqlite::Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}
