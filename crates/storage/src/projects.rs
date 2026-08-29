//! Project metadata persistence.

use std::str::FromStr;

use cli_master_core::{Project, ProjectId};
use rusqlite::{Row, params};

use crate::Storage;
use crate::error::{
    StorageError, corrupt_data, map_delete_error, map_write_error, persisted_validation,
};
use crate::models::{validate_display_name, validate_rfc3339_timestamp};
use crate::paths::{path_from_sql_value, path_to_sql_value};
use crate::values::rfc3339_timestamp_from_sql_value;

const PROJECT_COLUMNS: &str = "id, name, path, created_at, last_opened_at";

impl Storage {
    /// Inserts project metadata without modifying the project directory itself.
    ///
    /// The project path must be absolute and NUL-free. Symlink canonicalization
    /// is intentionally delegated to the Git or application service layer.
    ///
    /// Repository root and current branch are derived by the Git layer and are
    /// intentionally not persisted by this metadata store.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid fields, duplicate IDs or paths, or database failures.
    pub fn insert_project(&self, project: &Project) -> Result<(), StorageError> {
        validate_project(project)?;
        let path = path_to_sql_value(&project.path, "project path")?;
        self.connection
            .execute(
                "INSERT INTO projects (id, name, path, created_at, last_opened_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    project.id.to_string(),
                    project.name,
                    path,
                    project.created_at,
                    project.last_opened_at,
                ],
            )
            .map_err(|error| map_write_error(error, "project"))?;
        Ok(())
    }

    /// Lists all registered projects, most recently opened first.
    ///
    /// # Errors
    ///
    /// Returns an error if rows cannot be loaded or decoded.
    pub fn list_projects(&self) -> Result<Vec<Project>, StorageError> {
        let sql = format!(
            "SELECT {PROJECT_COLUMNS} FROM projects
             ORDER BY julianday(last_opened_at) DESC, name COLLATE NOCASE, id"
        );
        let mut statement = self.connection.prepare(&sql)?;
        let mut rows = statement.query([])?;
        let mut projects = Vec::new();
        while let Some(row) = rows.next()? {
            projects.push(decode_project(row)?);
        }
        Ok(projects)
    }

    /// Loads one registered project by ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the row cannot be loaded or decoded.
    pub fn get_project(&self, id: ProjectId) -> Result<Option<Project>, StorageError> {
        let sql = format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE id = ?1");
        let mut statement = self.connection.prepare(&sql)?;
        let mut rows = statement.query([id.to_string()])?;
        rows.next()?.map(decode_project).transpose()
    }

    /// Renames a registered project without touching its directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the name is invalid, the project is missing, or the update fails.
    pub fn rename_project(&self, id: ProjectId, name: &str) -> Result<(), StorageError> {
        validate_display_name("project name", name)?;
        let changed = self.connection.execute(
            "UPDATE projects SET name = ?1 WHERE id = ?2",
            params![name, id.to_string()],
        )?;
        require_changed(changed, id)
    }

    /// Updates a project's most-recently-opened timestamp.
    ///
    /// # Errors
    ///
    /// Returns an error if the timestamp is invalid, the project is missing, or the update fails.
    pub fn touch_project(&self, id: ProjectId, opened_at: &str) -> Result<(), StorageError> {
        validate_rfc3339_timestamp("project last_opened_at", opened_at)?;
        let changed = self.connection.execute(
            "UPDATE projects SET last_opened_at = ?1 WHERE id = ?2",
            params![opened_at, id.to_string()],
        )?;
        require_changed(changed, id)
    }

    /// Removes only the project metadata row; no filesystem content is deleted.
    ///
    /// Existing sessions or worktrees protect their project row through foreign keys.
    ///
    /// # Errors
    ///
    /// Returns an error if the project is missing, is still referenced, or deletion fails.
    pub fn remove_project_metadata(&self, id: ProjectId) -> Result<(), StorageError> {
        let changed = self
            .connection
            .execute("DELETE FROM projects WHERE id = ?1", [id.to_string()])
            .map_err(|error| map_delete_error(error, "project", "its sessions and worktrees"))?;
        require_changed(changed, id)
    }
}

fn validate_project(project: &Project) -> Result<(), StorageError> {
    validate_display_name("project name", &project.name)?;
    validate_rfc3339_timestamp("project created_at", &project.created_at)?;
    validate_rfc3339_timestamp("project last_opened_at", &project.last_opened_at)?;
    path_to_sql_value(&project.path, "project path")?;
    Ok(())
}

fn decode_project(row: &Row<'_>) -> Result<Project, StorageError> {
    let id_text: String = row.get(0)?;
    let id = ProjectId::from_str(&id_text)
        .map_err(|error| corrupt_data("project", "id", error.to_string()))?;
    let project = Project {
        id,
        name: row.get(1)?,
        path: path_from_sql_value(row.get_ref(2)?, "project", "path")?,
        created_at: rfc3339_timestamp_from_sql_value(row.get_ref(3)?, "project", "created_at")?,
        last_opened_at: rfc3339_timestamp_from_sql_value(
            row.get_ref(4)?,
            "project",
            "last_opened_at",
        )?,
    };
    persisted_validation("project", validate_project(&project))?;
    Ok(project)
}

fn require_changed(changed: usize, id: ProjectId) -> Result<(), StorageError> {
    if changed == 0 {
        Err(StorageError::NotFound {
            entity: "project",
            id: id.to_string(),
        })
    } else {
        Ok(())
    }
}
