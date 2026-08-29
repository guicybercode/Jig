use std::path::PathBuf;

use cli_master_core::{
    AgentId, AgentSource, Project, ProjectId, Session, SessionId, SessionStatus, Worktree,
    WorktreeId, WorktreeState,
};

use crate::StorageError;
use crate::time::rfc3339_to_ms;

/// Project row stored in SQLite.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectRecord {
    /// Project identifier.
    pub id: ProjectId,
    /// Display name.
    pub name: String,
    /// Canonical repository path.
    pub path: PathBuf,
    /// Creation timestamp.
    pub created_at: String,
    /// Last opened timestamp.
    pub last_opened_at: String,
}

impl ProjectRecord {
    /// Converts this row into the public project DTO without Git inspection.
    ///
    /// # Errors
    ///
    /// Returns an error when a stored timestamp is invalid.
    pub fn into_project(self) -> Result<Project, StorageError> {
        Ok(Project {
            id: self.id,
            name: self.name,
            path: self.path.clone(),
            repository_root: Some(self.path),
            current_branch: None,
            created_at_ms: rfc3339_to_ms(&self.created_at)?,
            last_opened_at_ms: rfc3339_to_ms(&self.last_opened_at)?,
        })
    }
}

/// Agent row stored in SQLite.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentRecord {
    /// Registry key.
    pub id: AgentId,
    /// Built-in or custom origin.
    pub source: AgentSource,
    /// Display name.
    pub name: String,
    /// Bare name or absolute executable path.
    pub executable: String,
    /// JSON array of arguments.
    pub args_json: String,
    /// JSON object of environment overrides.
    pub env_json: String,
    /// Whether the agent may be launched.
    pub enabled: bool,
    /// Creation timestamp.
    pub created_at: String,
    /// Update timestamp.
    pub updated_at: String,
}

impl AgentRecord {
    /// Parses the stored argument JSON array.
    ///
    /// # Errors
    ///
    /// Returns an error when `args_json` is not a JSON string array.
    pub fn args(&self) -> Result<Vec<String>, StorageError> {
        serde_json::from_str(&self.args_json)
            .map_err(|error| StorageError::InvalidJson(format!("args_json: {error}")))
    }
}

/// Session row stored in SQLite.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionRecord {
    /// Session identifier.
    pub id: SessionId,
    /// Owning project.
    pub project_id: ProjectId,
    /// Agent used to launch.
    pub agent_id: AgentId,
    /// Display name.
    pub name: String,
    /// Working directory.
    pub cwd: PathBuf,
    /// Persisted lifecycle status.
    pub status: SessionStatus,
    /// Last known PID.
    pub runtime_pid: Option<i64>,
    /// Daemon instance that last owned the live PTY.
    pub daemon_instance_id: Option<String>,
    /// Process exit code.
    pub exit_code: Option<i32>,
    /// Stable error code.
    pub error_code: Option<String>,
    /// Creation timestamp.
    pub created_at: String,
    /// Update timestamp.
    pub updated_at: String,
    /// Last PTY activity timestamp.
    pub last_activity_at: Option<String>,
}

impl SessionRecord {
    /// Converts this row into the public session DTO.
    ///
    /// # Errors
    ///
    /// Returns an error when a stored timestamp is invalid.
    pub fn into_session(
        self,
        worktree_id: Option<WorktreeId>,
        worktree_path: Option<PathBuf>,
        branch: Option<String>,
    ) -> Result<Session, StorageError> {
        let pid = self.runtime_pid.and_then(|value| u32::try_from(value).ok());
        Ok(Session {
            id: self.id,
            project_id: self.project_id,
            name: self.name,
            agent_id: self.agent_id,
            cwd: self.cwd,
            pid,
            pty_id: None,
            branch,
            worktree_id,
            worktree_path,
            status: self.status,
            exit_code: self.exit_code,
            error_code: self.error_code,
            created_at_ms: rfc3339_to_ms(&self.created_at)?,
            updated_at_ms: rfc3339_to_ms(&self.updated_at)?,
        })
    }
}

/// Worktree row stored in SQLite.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorktreeRecord {
    /// Worktree identifier.
    pub id: WorktreeId,
    /// Owning project.
    pub project_id: ProjectId,
    /// Associated session, if any.
    pub session_id: Option<SessionId>,
    /// Worktree path.
    pub path: PathBuf,
    /// Branch name.
    pub branch: String,
    /// Persistence state.
    pub state: WorktreeState,
    /// Creation timestamp.
    pub created_at: String,
    /// Update timestamp.
    pub updated_at: String,
}

impl WorktreeRecord {
    /// Converts this row into the public worktree DTO.
    ///
    /// # Errors
    ///
    /// Returns an error when a stored timestamp is invalid.
    pub fn into_worktree(self, is_dirty: bool) -> Result<Worktree, StorageError> {
        Ok(Worktree {
            id: self.id,
            project_id: self.project_id,
            session_id: self.session_id,
            path: self.path,
            branch: self.branch,
            state: self.state,
            is_dirty,
            created_at_ms: rfc3339_to_ms(&self.created_at)?,
            updated_at_ms: rfc3339_to_ms(&self.updated_at)?,
        })
    }
}
