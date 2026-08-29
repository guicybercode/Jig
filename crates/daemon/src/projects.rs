use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use cli_master_core::wire::{
    EmptyResponse, ProjectAddRequest, ProjectListResponse, ProjectRemoveRequest,
    ProjectRenameRequest,
};
use cli_master_core::{ApiError, Project, ProjectId};
use cli_master_git::{Git, GitError, GitErrorKind};
use cli_master_storage::{Storage, StorageError};

/// Owns project validation and persistence for daemon IPC handlers.
#[derive(Debug)]
pub(super) struct ProjectRegistry {
    storage: Mutex<Storage>,
    git: Option<Git>,
}

impl ProjectRegistry {
    pub(super) fn new(storage: Storage) -> Self {
        Self {
            storage: Mutex::new(storage),
            git: Git::discover().ok(),
        }
    }

    pub(super) fn snapshot(&self) -> Result<Vec<Project>, ApiError> {
        let mut projects = self.storage()?.list_projects().map_err(storage_error)?;
        if let Some(git) = &self.git {
            for project in &mut projects {
                enrich_project(git, project);
            }
        }
        Ok(projects)
    }

    pub(super) fn list(&self) -> Result<ProjectListResponse, ApiError> {
        self.snapshot()
            .map(|projects| ProjectListResponse { projects })
    }

    pub(super) fn add(&self, request: ProjectAddRequest) -> Result<Project, ApiError> {
        let selected_path = Path::new(request.path.as_str());
        let (path, repository_root, current_branch) = if let Some(git) = &self.git {
            let inspection = git
                .inspect_repository(selected_path)
                .map_err(|error| git_error(&error))?;
            (
                inspection.path,
                inspection.repository_root,
                inspection.branch,
            )
        } else {
            (canonical_project_directory(selected_path)?, None, None)
        };
        let name = request.name.map_or_else(
            || default_project_name(repository_root.as_deref().unwrap_or(&path)),
            |name| Ok(name.into_inner()),
        )?;
        let now = unix_timestamp_ms()?;
        let project = Project {
            id: ProjectId::new(),
            name,
            path,
            repository_root,
            current_branch,
            created_at_ms: now,
            last_opened_at_ms: now,
        };
        self.storage()?
            .insert_project(&project)
            .map_err(storage_error)?;
        Ok(project)
    }

    pub(super) fn rename(&self, request: &ProjectRenameRequest) -> Result<Project, ApiError> {
        let storage = self.storage()?;
        storage
            .rename_project(request.project_id, request.name.as_str())
            .map_err(storage_error)?;
        let mut project = storage
            .get_project(request.project_id)
            .map_err(storage_error)?
            .ok_or_else(|| project_not_found(request.project_id))?;
        drop(storage);
        if let Some(git) = &self.git {
            enrich_project(git, &mut project);
        }
        Ok(project)
    }

    pub(super) fn remove(&self, request: ProjectRemoveRequest) -> Result<EmptyResponse, ApiError> {
        self.storage()?
            .remove_project_metadata(request.project_id)
            .map_err(storage_error)?;
        Ok(EmptyResponse::default())
    }

    fn storage(&self) -> Result<MutexGuard<'_, Storage>, ApiError> {
        self.storage.lock().map_err(|_| {
            ApiError::new(
                "storage_unavailable",
                "Project storage is temporarily unavailable.",
            )
            .with_action("Restart Jig and try again.")
        })
    }
}

fn enrich_project(git: &Git, project: &mut Project) {
    let Ok(inspection) = git.inspect_repository(&project.path) else {
        return;
    };
    project.path = inspection.path;
    project.repository_root = inspection.repository_root;
    project.current_branch = inspection.branch;
}

fn default_project_name(repository_root: &Path) -> Result<String, ApiError> {
    repository_root
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            ApiError::new(
                "project_name_unavailable",
                "Jig could not derive a project name from this folder.",
            )
            .with_action("Enter a display name and try again.")
        })
}

fn canonical_project_directory(path: &Path) -> Result<PathBuf, ApiError> {
    if !path.exists() {
        return Err(ApiError::new(
            "project_path_not_found",
            "The selected project folder does not exist.",
        )
        .with_action("Choose an existing folder and try again."));
    }
    if !path.is_dir() {
        return Err(ApiError::new(
            "project_path_not_directory",
            "The selected project path is not a folder.",
        )
        .with_action("Choose a folder instead of a file."));
    }
    fs::canonicalize(path).map_err(|error| {
        ApiError::new(
            "project_path_unavailable",
            "Jig could not open the selected project folder.",
        )
        .with_action("Check the folder permissions and try again.")
        .with_detail("reason", error.to_string())
    })
}

fn unix_timestamp_ms() -> Result<i64, ApiError> {
    let elapsed = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| {
        ApiError::new(
            "system_clock_invalid",
            "The system clock cannot be used to register this project.",
        )
        .with_action("Correct the system date and time, then try again.")
    })?;
    i64::try_from(elapsed.as_millis()).map_err(|_| {
        ApiError::new(
            "system_clock_invalid",
            "The system clock is outside the supported range.",
        )
        .with_action("Correct the system date and time, then try again.")
    })
}

fn git_error(error: &GitError) -> ApiError {
    let code = match error.kind() {
        GitErrorKind::NotFound => "git_or_path_not_found",
        GitErrorKind::InvalidInput => "invalid_project_path",
        GitErrorKind::NotRepository => "not_git_repository",
        GitErrorKind::CommandFailed => "git_command_failed",
        GitErrorKind::Timeout => "git_timeout",
        GitErrorKind::InvalidOutput => "git_invalid_output",
        GitErrorKind::Io => "git_io_error",
        GitErrorKind::DirtyWorktree
        | GitErrorKind::WorktreeInUse
        | GitErrorKind::UnsafePath
        | GitErrorKind::PartialWorktree => "git_validation_failed",
    };
    let mut api_error = ApiError::new(code, error.message()).with_action(error.action());
    if let Some(path) = error.path() {
        api_error = api_error.with_detail("path", path.to_string_lossy().into_owned());
    }
    if let Some(status) = error.exit_status() {
        api_error = api_error.with_detail("exitStatus", status);
    }
    api_error
}

fn storage_error(error: StorageError) -> ApiError {
    match error {
        StorageError::AlreadyExists { entity: "project" } => ApiError::new(
            "project_already_registered",
            "This project folder is already registered.",
        )
        .with_action("Choose another repository or open the existing project."),
        StorageError::NotFound {
            entity: "project",
            id,
        } => ApiError::new("project_not_found", "This project is no longer registered.")
            .with_action("Refresh the workspace and try again.")
            .with_detail("projectId", id),
        StorageError::RelationshipViolation {
            entity: "project", ..
        } => ApiError::new(
            "project_in_use",
            "This project still has sessions or worktrees attached.",
        )
        .with_action("Remove its sessions and worktrees before removing the project."),
        StorageError::InvalidInput { field, reason } => {
            ApiError::new("invalid_project", "The project details are invalid.")
                .with_action("Review the folder and display name, then try again.")
                .with_detail("field", field)
                .with_detail("reason", reason)
        }
        other => ApiError::new(
            "project_storage_failed",
            "Jig could not update the project database.",
        )
        .with_action("Restart Jig and try again.")
        .with_detail("reason", other.to_string()),
    }
}

fn project_not_found(project_id: ProjectId) -> ApiError {
    ApiError::new("project_not_found", "This project is no longer registered.")
        .with_action("Refresh the workspace and try again.")
        .with_detail("projectId", project_id.to_string())
}
