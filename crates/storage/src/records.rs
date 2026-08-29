use std::collections::BTreeMap;
use std::path::PathBuf;

use cli_master_core::{
    AgentDefinition, AgentId, AgentSource, CommandSpec, Project, ProjectId, Session, SessionId,
    SessionStatus, Worktree, WorktreeId,
};

/// Filesystem availability of a stored path. Missing paths are never deleted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathStatus {
    /// The path currently exists on disk.
    Available,
    /// The path is missing, moved, or not currently mounted.
    Missing,
}

/// Git worktree lifecycle stored in SQLite.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorktreeState {
    /// Git isolation is being created.
    Creating,
    /// The worktree directory is usable.
    Active,
    /// Removal was requested and is waiting for confirmation or retry.
    RemovePending,
    /// Removal failed or the directory disappeared unexpectedly.
    Orphaned,
}

impl WorktreeState {
    pub(crate) const fn as_db(self) -> &'static str {
        match self {
            Self::Creating => "creating",
            Self::Active => "active",
            Self::RemovePending => "remove_pending",
            Self::Orphaned => "orphaned",
        }
    }

    pub(crate) fn from_db(value: &str) -> Self {
        match value {
            "creating" => Self::Creating,
            "active" => Self::Active,
            "remove_pending" => Self::RemovePending,
            _ => Self::Orphaned,
        }
    }
}

/// Input for registering a project. The directory is never deleted by storage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewProject {
    /// Stable identifier.
    pub id: ProjectId,
    /// Display name.
    pub name: String,
    /// Canonical absolute project path.
    pub path: PathBuf,
    /// Canonical repository root when the path is inside a Git work tree.
    pub repository_root: Option<PathBuf>,
}

/// Persisted project plus live filesystem status.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredProject {
    /// Public project DTO.
    pub project: Project,
    /// Whether `project.path` exists right now.
    pub path_status: PathStatus,
}

impl StoredProject {
    /// Returns whether new sessions can use this project path.
    #[must_use]
    pub const fn path_is_usable(&self) -> bool {
        matches!(self.path_status, PathStatus::Available)
    }
}

/// Input for a custom agent definition. Environment must be explicit overrides.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewCustomAgent {
    /// Stable identifier.
    pub id: AgentId,
    /// Display name.
    pub name: String,
    /// Absolute path or bare executable name.
    pub executable: String,
    /// Ordered argument array. Never a shell string.
    pub args: Vec<String>,
    /// Non-secret environment overrides only.
    pub env: BTreeMap<String, String>,
}

/// Persisted agent definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredAgent {
    /// Stable identifier.
    pub id: AgentId,
    /// Built-in or custom origin.
    pub source: AgentSource,
    /// Display name.
    pub name: String,
    /// Executable path or name.
    pub executable: String,
    /// Ordered arguments.
    pub args: Vec<String>,
    /// Allowed environment overrides.
    pub env: BTreeMap<String, String>,
    /// Whether the definition may be launched.
    pub enabled: bool,
    /// Creation time as Unix epoch milliseconds.
    pub created_at_ms: i64,
    /// Latest edit time as Unix epoch milliseconds.
    pub updated_at_ms: i64,
}

impl StoredAgent {
    /// Maps to the public agent DTO using `cwd` from the launch context.
    ///
    /// # Errors
    ///
    /// Returns a command validation error when `cwd` is empty.
    pub fn to_definition(
        &self,
        cwd: impl Into<PathBuf>,
    ) -> Result<AgentDefinition, cli_master_core::CommandSpecError> {
        Ok(AgentDefinition {
            id: self.id,
            display_name: self.name.clone(),
            description: None,
            source: self.source,
            command: CommandSpec::try_from_parts(
                self.executable.clone(),
                self.args.clone(),
                cwd,
                self.env.clone(),
            )?,
        })
    }
}

/// Input for inserting a session row in `starting` status.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewSession {
    /// Stable identifier.
    pub id: SessionId,
    /// Owning project.
    pub project_id: ProjectId,
    /// Agent used to launch.
    pub agent_id: AgentId,
    /// Display name / task-like label.
    pub name: String,
    /// Working directory at launch.
    pub cwd: PathBuf,
    /// Associated Git branch, when known.
    pub branch: Option<String>,
    /// Associated worktree path, when isolation is enabled.
    pub worktree_path: Option<PathBuf>,
}

/// Session metadata plus daemon-only recovery fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredSession {
    /// Public session DTO. `pid` is historical after restart.
    pub session: Session,
    /// Daemon instance that last owned the live PTY, when any.
    pub daemon_instance_id: Option<String>,
    /// Stable error code for a failed launch or exit.
    pub error_code: Option<String>,
}

/// Input for inserting a worktree row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewWorktree {
    /// Stable identifier.
    pub id: WorktreeId,
    /// Owning project.
    pub project_id: ProjectId,
    /// Session that reserved this worktree, if any.
    pub session_id: Option<SessionId>,
    /// Absolute worktree path.
    pub path: PathBuf,
    /// Branch checked out in the worktree.
    pub branch: String,
    /// Initial lifecycle state.
    pub state: WorktreeState,
}

/// Persisted worktree plus filesystem status.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredWorktree {
    /// Public worktree DTO. `is_dirty` is unknown until Git inspects it.
    pub worktree: Worktree,
    /// Stored lifecycle state.
    pub state: WorktreeState,
    /// Whether `worktree.path` exists right now.
    pub path_status: PathStatus,
}

/// Distinguishes why a persisted live row was or was not changed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReconciliationReason {
    /// Metadata exists and needed no change.
    Known,
    /// [`crate::LiveSessionIndex`] currently owns this session.
    Running,
    /// This daemon instance recorded a live status but the process is gone.
    ProcessGone,
    /// A previous daemon instance left a live status; PTYs cannot be reattached.
    DaemonRestarted,
    /// The session already recorded a normal or failed exit.
    ExitedNormally,
}

/// One persisted session after startup reconciliation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconciliationEvent {
    /// Session that was inspected.
    pub session_id: SessionId,
    /// Status stored before reconciliation.
    pub previous_status: SessionStatus,
    /// Status stored after reconciliation.
    pub new_status: SessionStatus,
    /// Why the status was kept or changed.
    pub reason: ReconciliationReason,
}

/// Inputs the daemon supplies when recovering after start.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryContext<'a> {
    /// Identifier of the daemon process that is starting now.
    pub current_daemon_instance_id: &'a str,
    /// Session IDs currently owned by this process's session manager.
    pub live_session_ids: &'a [SessionId],
}
