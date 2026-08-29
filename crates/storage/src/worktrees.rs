//! Worktree metadata persistence.

use std::str::FromStr;

use cli_master_core::{ProjectId, SessionId, WorktreeId};
use rusqlite::{Row, params};

use crate::Storage;
use crate::error::{StorageError, corrupt_data, map_write_error, persisted_validation};
use crate::models::{StoredWorktree, WorktreeState, validate_timestamp};
use crate::paths::{path_from_sql_value, path_to_sql_value};
use crate::values::timestamp_from_sql_value;

const WORKTREE_COLUMNS: &str = "id, project_id, session_id, path, branch, state, \
    is_dirty, created_at, updated_at";

impl Storage {
    /// Inserts worktree metadata without running Git or creating directories.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid metadata, cross-project associations,
    /// duplicate unique values, missing references, or database failures.
    pub fn insert_worktree(&self, worktree: &StoredWorktree) -> Result<(), StorageError> {
        worktree.validate()?;
        let path = path_to_sql_value(&worktree.path, "worktree path")?;
        self.connection
            .execute(
                "INSERT INTO worktrees (
                    id, project_id, session_id, path, branch, state,
                    is_dirty, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    worktree.id.to_string(),
                    worktree.project_id.to_string(),
                    worktree.session_id.map(|id| id.to_string()),
                    path,
                    worktree.branch,
                    worktree.state.as_database_value(),
                    worktree.is_dirty,
                    worktree.created_at_ms,
                    worktree.updated_at_ms,
                ],
            )
            .map_err(|error| map_write_error(error, "worktree"))?;
        Ok(())
    }

    /// Lists all managed worktrees, newest first.
    ///
    /// # Errors
    ///
    /// Returns an error if rows cannot be loaded or decoded.
    pub fn list_worktrees(&self) -> Result<Vec<StoredWorktree>, StorageError> {
        let sql = format!(
            "SELECT {WORKTREE_COLUMNS} FROM worktrees
             ORDER BY CAST(created_at AS INTEGER) DESC, id"
        );
        let mut statement = self.connection.prepare(&sql)?;
        let mut rows = statement.query([])?;
        collect_worktrees(&mut rows)
    }

    /// Lists managed worktrees for one project, newest first.
    ///
    /// # Errors
    ///
    /// Returns an error if rows cannot be loaded or decoded.
    pub fn list_worktrees_for_project(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<StoredWorktree>, StorageError> {
        let sql = format!(
            "SELECT {WORKTREE_COLUMNS} FROM worktrees WHERE project_id = ?1
             ORDER BY CAST(created_at AS INTEGER) DESC, id"
        );
        let mut statement = self.connection.prepare(&sql)?;
        let mut rows = statement.query([project_id.to_string()])?;
        collect_worktrees(&mut rows)
    }

    /// Loads one managed worktree by ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the row cannot be loaded or decoded.
    pub fn get_worktree(&self, id: WorktreeId) -> Result<Option<StoredWorktree>, StorageError> {
        let sql = format!("SELECT {WORKTREE_COLUMNS} FROM worktrees WHERE id = ?1");
        let mut statement = self.connection.prepare(&sql)?;
        let mut rows = statement.query([id.to_string()])?;
        rows.next()?.map(decode_worktree).transpose()
    }

    /// Atomically updates lifecycle state, dirty status, and session association.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid timestamps, cross-project associations,
    /// duplicate session associations, a missing worktree, or database failures.
    pub fn update_worktree_state(
        &self,
        id: WorktreeId,
        state: WorktreeState,
        is_dirty: bool,
        session_id: Option<SessionId>,
        updated_at_ms: i64,
    ) -> Result<(), StorageError> {
        validate_timestamp("worktree updated_at_ms", updated_at_ms)?;
        let changed = self
            .connection
            .execute(
                "UPDATE worktrees
                 SET state = ?1, is_dirty = ?2, session_id = ?3, updated_at = ?4
                 WHERE id = ?5",
                params![
                    state.as_database_value(),
                    is_dirty,
                    session_id.map(|session| session.to_string()),
                    updated_at_ms,
                    id.to_string(),
                ],
            )
            .map_err(|error| map_write_error(error, "worktree"))?;
        require_changed(changed, id)
    }

    /// Removes only worktree metadata; no Git or filesystem operation is performed.
    ///
    /// # Errors
    ///
    /// Returns an error if the worktree is missing or deletion fails.
    pub fn remove_worktree_metadata(&self, id: WorktreeId) -> Result<(), StorageError> {
        let changed = self
            .connection
            .execute("DELETE FROM worktrees WHERE id = ?1", [id.to_string()])?;
        require_changed(changed, id)
    }
}

fn collect_worktrees(rows: &mut rusqlite::Rows<'_>) -> Result<Vec<StoredWorktree>, StorageError> {
    let mut worktrees = Vec::new();
    while let Some(row) = rows.next()? {
        worktrees.push(decode_worktree(row)?);
    }
    Ok(worktrees)
}

fn decode_worktree(row: &Row<'_>) -> Result<StoredWorktree, StorageError> {
    let id_text: String = row.get(0)?;
    let project_id_text: String = row.get(1)?;
    let session_id_text: Option<String> = row.get(2)?;
    let state_text: String = row.get(5)?;
    let worktree = StoredWorktree {
        id: WorktreeId::from_str(&id_text)
            .map_err(|error| corrupt_data("worktree", "id", error.to_string()))?,
        project_id: ProjectId::from_str(&project_id_text)
            .map_err(|error| corrupt_data("worktree", "project_id", error.to_string()))?,
        session_id: session_id_text
            .map(|id| {
                SessionId::from_str(&id)
                    .map_err(|error| corrupt_data("worktree", "session_id", error.to_string()))
            })
            .transpose()?,
        path: path_from_sql_value(row.get_ref(3)?, "worktree", "path")?,
        branch: row.get(4)?,
        state: WorktreeState::from_database_value(&state_text)?,
        is_dirty: row.get(6)?,
        created_at_ms: timestamp_from_sql_value(row.get_ref(7)?, "worktree", "created_at")?,
        updated_at_ms: timestamp_from_sql_value(row.get_ref(8)?, "worktree", "updated_at")?,
    };
    persisted_validation("worktree", worktree.validate())?;
    Ok(worktree)
}

fn require_changed(changed: usize, id: WorktreeId) -> Result<(), StorageError> {
    if changed == 0 {
        Err(StorageError::NotFound {
            entity: "worktree",
            id: id.to_string(),
        })
    } else {
        Ok(())
    }
}
