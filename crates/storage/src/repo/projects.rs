use std::path::{Path, PathBuf};

use cli_master_core::{Project, ProjectId};
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::error::{EntityKind, StorageError};
use crate::records::{NewProject, PathStatus, StoredProject};
use crate::serialize::{absolute_path, required_text};
use crate::time::{now_rfc3339, rfc3339_to_unix_ms};

pub(crate) struct ProjectRepository<'a> {
    connection: &'a Connection,
}

impl<'a> ProjectRepository<'a> {
    pub(crate) const fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub(crate) fn insert(&self, new: &NewProject) -> Result<StoredProject, StorageError> {
        required_text(&new.name, "name", EntityKind::Project)?;
        let path = resolve_existing_path(&new.path, EntityKind::Project)?;
        let repository_root = new
            .repository_root
            .as_deref()
            .map(|root| resolve_existing_path(root, EntityKind::Project))
            .transpose()?;
        let timestamp = now_rfc3339()?;
        let id = new.id.to_string();

        self.connection
            .execute(
                "INSERT INTO projects (
                    id, name, path, repository_root, created_at, updated_at, last_opened_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?5)",
                params![
                    id,
                    new.name.trim(),
                    path.to_string_lossy(),
                    repository_root
                        .as_ref()
                        .map(|root| root.to_string_lossy().into_owned()),
                    timestamp
                ],
            )
            .map_err(|error| {
                crate::repo::remap_constraint(
                    StorageError::from_sqlite("insert", EntityKind::Project, &error),
                    StorageError::conflict(
                        "insert",
                        EntityKind::Project,
                        "a project with this path is already registered",
                        "Open the existing project or choose another directory.",
                    ),
                )
            })?;

        self.get(new.id)
    }

    pub(crate) fn rename(&self, id: ProjectId, name: &str) -> Result<StoredProject, StorageError> {
        required_text(name, "name", EntityKind::Project)?;
        let timestamp = now_rfc3339()?;
        let updated = self
            .connection
            .execute(
                "UPDATE projects SET name = ?1, updated_at = ?2 WHERE id = ?3",
                params![name.trim(), timestamp, id.to_string()],
            )
            .map_err(|error| StorageError::from_sqlite("rename", EntityKind::Project, &error))?;
        if updated == 0 {
            return Err(StorageError::not_found("rename", EntityKind::Project, id));
        }
        self.get(id)
    }

    pub(crate) fn touch_opened(&self, id: ProjectId) -> Result<StoredProject, StorageError> {
        let timestamp = now_rfc3339()?;
        let updated = self
            .connection
            .execute(
                "UPDATE projects SET last_opened_at = ?1, updated_at = ?1 WHERE id = ?2",
                params![timestamp, id.to_string()],
            )
            .map_err(|error| StorageError::from_sqlite("touch", EntityKind::Project, &error))?;
        if updated == 0 {
            return Err(StorageError::not_found("touch", EntityKind::Project, id));
        }
        self.get(id)
    }

    pub(crate) fn remove(&self, id: ProjectId) -> Result<(), StorageError> {
        let session_count: i64 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE project_id = ?1",
                params![id.to_string()],
                |row| row.get(0),
            )
            .map_err(|error| StorageError::from_sqlite("remove", EntityKind::Project, &error))?;
        if session_count > 0 {
            return Err(StorageError::conflict(
                "remove",
                EntityKind::Project,
                "project still has session metadata",
                "Delete session metadata first. The repository directory is never removed.",
            ));
        }
        let worktree_count: i64 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM worktrees WHERE project_id = ?1",
                params![id.to_string()],
                |row| row.get(0),
            )
            .map_err(|error| StorageError::from_sqlite("remove", EntityKind::Project, &error))?;
        if worktree_count > 0 {
            return Err(StorageError::conflict(
                "remove",
                EntityKind::Project,
                "project still has worktree metadata",
                "Remove worktree records first. Repository files are never deleted.",
            ));
        }

        let deleted = self
            .connection
            .execute(
                "DELETE FROM projects WHERE id = ?1",
                params![id.to_string()],
            )
            .map_err(|error| StorageError::from_sqlite("remove", EntityKind::Project, &error))?;
        if deleted == 0 {
            return Err(StorageError::not_found("remove", EntityKind::Project, id));
        }
        Ok(())
    }

    pub(crate) fn get(&self, id: ProjectId) -> Result<StoredProject, StorageError> {
        self.connection
            .query_row(
                "SELECT id, name, path, repository_root, created_at, updated_at, last_opened_at
                 FROM projects WHERE id = ?1",
                params![id.to_string()],
                map_project_row,
            )
            .optional()
            .map_err(|error| StorageError::from_sqlite("get", EntityKind::Project, &error))?
            .ok_or_else(|| StorageError::not_found("get", EntityKind::Project, id))
    }

    pub(crate) fn list(&self) -> Result<Vec<StoredProject>, StorageError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, name, path, repository_root, created_at, updated_at, last_opened_at
                 FROM projects
                 ORDER BY last_opened_at DESC, name COLLATE NOCASE",
            )
            .map_err(|error| StorageError::from_sqlite("list", EntityKind::Project, &error))?;
        let rows = statement
            .query_map([], map_project_row)
            .map_err(|error| StorageError::from_sqlite("list", EntityKind::Project, &error))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| StorageError::from_sqlite("list", EntityKind::Project, &error))
    }
}

fn resolve_existing_path(path: &Path, entity: EntityKind) -> Result<PathBuf, StorageError> {
    let absolute = absolute_path(path, entity)?;
    if !absolute.exists() {
        return Err(StorageError::path_missing(
            "validate path",
            entity,
            &absolute,
        ));
    }
    std::fs::canonicalize(&absolute)
        .map_err(|error| StorageError::io("canonicalize", entity, error))
}

fn map_project_row(row: &Row<'_>) -> rusqlite::Result<StoredProject> {
    let id: String = row.get("id")?;
    let path = PathBuf::from(row.get::<_, String>("path")?);
    let repository_root = row
        .get::<_, Option<String>>("repository_root")?
        .map(PathBuf::from);
    let created_at: String = row.get("created_at")?;
    let updated_at: String = row.get("updated_at")?;
    let last_opened_at: String = row.get("last_opened_at")?;
    let path_status = if path.exists() {
        PathStatus::Available
    } else {
        PathStatus::Missing
    };

    let created_at_ms = rfc3339_to_unix_ms(&created_at).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let updated_at_ms = rfc3339_to_unix_ms(&updated_at).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let last_opened_at_ms = rfc3339_to_unix_ms(&last_opened_at).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })?;

    Ok(StoredProject {
        project: Project {
            id: id.parse().map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?,
            name: row.get("name")?,
            path,
            repository_root,
            current_branch: None,
            created_at_ms,
            updated_at_ms,
            last_opened_at_ms,
        },
        path_status,
    })
}
