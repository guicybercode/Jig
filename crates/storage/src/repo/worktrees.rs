use std::path::PathBuf;

use cli_master_core::{ProjectId, SessionId, Worktree, WorktreeId};
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::error::{EntityKind, StorageError};
use crate::records::{NewWorktree, PathStatus, StoredWorktree, WorktreeState};
use crate::serialize::{absolute_path, required_text};
use crate::time::{now_rfc3339, rfc3339_to_unix_ms};

pub(crate) struct WorktreeRepository<'a> {
    connection: &'a Connection,
}

impl<'a> WorktreeRepository<'a> {
    pub(crate) const fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub(crate) fn insert(&self, new: &NewWorktree) -> Result<StoredWorktree, StorageError> {
        required_text(&new.branch, "branch", EntityKind::Worktree)?;
        let path = absolute_path(&new.path, EntityKind::Worktree)?;
        let timestamp = now_rfc3339()?;
        self.connection
            .execute(
                "INSERT INTO worktrees (
                    id, project_id, session_id, path, branch, state, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                params![
                    new.id.to_string(),
                    new.project_id.to_string(),
                    new.session_id.map(|id| id.to_string()),
                    path.to_string_lossy(),
                    new.branch.trim(),
                    new.state.as_db(),
                    timestamp
                ],
            )
            .map_err(|error| {
                crate::repo::remap_constraint(
                    StorageError::from_sqlite("insert", EntityKind::Worktree, &error),
                    StorageError::conflict(
                        "insert",
                        EntityKind::Worktree,
                        "a worktree with this path or branch already exists",
                        "Choose a different branch name or worktree path.",
                    ),
                )
            })?;
        self.get(new.id)
    }

    pub(crate) fn get(&self, id: WorktreeId) -> Result<StoredWorktree, StorageError> {
        self.connection
            .query_row(
                "SELECT id, project_id, session_id, path, branch, state, created_at, updated_at
                 FROM worktrees WHERE id = ?1",
                params![id.to_string()],
                map_worktree_row,
            )
            .optional()
            .map_err(|error| StorageError::from_sqlite("get", EntityKind::Worktree, &error))?
            .ok_or_else(|| StorageError::not_found("get", EntityKind::Worktree, id))
    }

    pub(crate) fn list_for_project(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<StoredWorktree>, StorageError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, project_id, session_id, path, branch, state, created_at, updated_at
                 FROM worktrees WHERE project_id = ?1 ORDER BY created_at",
            )
            .map_err(|error| StorageError::from_sqlite("list", EntityKind::Worktree, &error))?;
        let rows = statement
            .query_map(params![project_id.to_string()], map_worktree_row)
            .map_err(|error| StorageError::from_sqlite("list", EntityKind::Worktree, &error))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| StorageError::from_sqlite("list", EntityKind::Worktree, &error))
    }

    pub(crate) fn set_state(
        &self,
        id: WorktreeId,
        state: WorktreeState,
    ) -> Result<StoredWorktree, StorageError> {
        let updated = self
            .connection
            .execute(
                "UPDATE worktrees SET state = ?1, updated_at = ?2 WHERE id = ?3",
                params![state.as_db(), now_rfc3339()?, id.to_string()],
            )
            .map_err(|error| StorageError::from_sqlite("update", EntityKind::Worktree, &error))?;
        if updated == 0 {
            return Err(StorageError::not_found("update", EntityKind::Worktree, id));
        }
        self.get(id)
    }

    pub(crate) fn attach_session(
        &self,
        id: WorktreeId,
        session_id: Option<SessionId>,
    ) -> Result<StoredWorktree, StorageError> {
        let updated = self
            .connection
            .execute(
                "UPDATE worktrees SET session_id = ?1, updated_at = ?2 WHERE id = ?3",
                params![
                    session_id.map(|value| value.to_string()),
                    now_rfc3339()?,
                    id.to_string()
                ],
            )
            .map_err(|error| StorageError::from_sqlite("update", EntityKind::Worktree, &error))?;
        if updated == 0 {
            return Err(StorageError::not_found("update", EntityKind::Worktree, id));
        }
        self.get(id)
    }

    pub(crate) fn remove(&self, id: WorktreeId) -> Result<(), StorageError> {
        let deleted = self
            .connection
            .execute(
                "DELETE FROM worktrees WHERE id = ?1",
                params![id.to_string()],
            )
            .map_err(|error| StorageError::from_sqlite("remove", EntityKind::Worktree, &error))?;
        if deleted == 0 {
            return Err(StorageError::not_found("remove", EntityKind::Worktree, id));
        }
        Ok(())
    }
}

fn map_worktree_row(row: &Row<'_>) -> rusqlite::Result<StoredWorktree> {
    let id: String = row.get("id")?;
    let project_id: String = row.get("project_id")?;
    let session_id: Option<String> = row.get("session_id")?;
    let path = PathBuf::from(row.get::<_, String>("path")?);
    let created_at: String = row.get("created_at")?;
    let state = WorktreeState::from_db(&row.get::<_, String>("state")?);
    let path_status = if path.exists() {
        PathStatus::Available
    } else {
        PathStatus::Missing
    };

    Ok(StoredWorktree {
        worktree: Worktree {
            id: parse_id(&id)?,
            project_id: parse_id(&project_id)?,
            session_id: session_id.as_deref().map(parse_id).transpose()?,
            path,
            branch: row.get("branch")?,
            is_dirty: false,
            created_at_ms: rfc3339_to_unix_ms(&created_at).map_err(conversion_error)?,
        },
        state,
        path_status,
    })
}

fn parse_id<T>(value: &str) -> rusqlite::Result<T>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    value.parse().map_err(conversion_error)
}

fn conversion_error<E>(error: E) -> rusqlite::Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}
