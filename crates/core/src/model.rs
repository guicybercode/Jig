use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::session::SessionStatus;
use crate::{AgentId, ProjectId, SessionId, WorktreeId};

/// Origin of an agent definition.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentSource {
    /// Definition supplied with the application.
    BuiltIn,
    /// Definition created explicitly by the user.
    Custom,
}

/// Lifecycle of a managed Git worktree record.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeState {
    /// Git and `SQLite` have not both accepted the worktree yet.
    Creating,
    /// The worktree exists and may be used as a session cwd.
    Active,
    /// Removal was requested and is waiting on confirmation or cleanup.
    RemovePending,
    /// Git and `SQLite` disagree; recovery instructions should be shown.
    Orphaned,
}

/// Catalog entry for a built-in or custom agent.
///
/// This is identity and enablement only. Launch argv is a [`crate::CommandSpec`]
/// produced at session start. Custom argv lives on [`CustomAgent`].
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDefinition {
    /// Stable catalog key for built-ins, `UUIDv7` string for custom agents.
    pub id: AgentId,
    /// User-facing agent name.
    pub display_name: String,
    /// Optional user-facing explanation of the agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether this definition is built in or user supplied.
    pub source: AgentSource,
    /// When false, the agent cannot be used to start new sessions.
    pub enabled: bool,
}

/// Persisted user-defined CLI adapter.
///
/// Environment values are sent to the local UI so the owner can edit them.
/// Logs and `Debug` must not print those values.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomAgent {
    /// Stable custom-agent identifier.
    pub id: AgentId,
    /// User-facing name.
    pub display_name: String,
    /// Absolute path or bare executable name.
    pub executable: String,
    /// Ordered argument array. Never a shell string.
    pub args: Vec<String>,
    /// Non-secret environment overrides. Authentication tokens are not stored.
    pub env: BTreeMap<String, String>,
    /// When false, the agent cannot be used to start new sessions.
    pub enabled: bool,
    /// RFC 3339 UTC creation time.
    pub created_at: String,
    /// RFC 3339 UTC last edit time.
    pub updated_at: String,
}

impl fmt::Debug for CustomAgent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CustomAgent")
            .field("id", &self.id)
            .field("display_name", &self.display_name)
            .field("executable", &self.executable)
            .field("args_count", &self.args.len())
            .field("env_keys", &self.env.keys().collect::<Vec<_>>())
            .field("enabled", &self.enabled)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

/// Wire form of executable detection. Mapped from the agents crate at the
/// daemon boundary; the UI never searches PATH itself.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionStatus {
    /// An executable regular file was resolved.
    Found,
    /// No candidate exists in the configured search path.
    NotFound,
    /// A candidate exists but is not executable.
    NotExecutable,
}

/// Detection result for one catalog agent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDetection {
    /// Catalog identifier that was probed.
    pub agent_id: AgentId,
    /// Outcome of PATH/absolute-path inspection.
    pub status: DetectionStatus,
    /// Resolved executable when [`DetectionStatus::Found`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable: Option<PathBuf>,
}

/// Persisted metadata for a registered local project.
///
/// `path` is the canonical Git repository root. Observed branch and dirty
/// state belong on [`GitStatus`], not here.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    /// Stable local identifier.
    pub id: ProjectId,
    /// User-editable display name.
    pub name: String,
    /// Canonical absolute repository root.
    pub path: PathBuf,
    /// RFC 3339 UTC creation time.
    pub created_at: String,
    /// RFC 3339 UTC last-open time.
    pub last_opened_at: String,
}

/// Public session metadata. Process handles and PTY masters are not included.
///
/// `status` is the last persisted snapshot. For live sessions the daemon
/// reconciles it with `SessionManager` before returning this DTO.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// Stable local identifier.
    pub id: SessionId,
    /// Project that owns this session.
    pub project_id: ProjectId,
    /// User-editable display name.
    pub name: String,
    /// Agent definition used to launch this session.
    pub agent_id: AgentId,
    /// Effective process working directory.
    pub cwd: PathBuf,
    /// Managed worktree associated with the session, when isolation is used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree_id: Option<WorktreeId>,
    /// Current lifecycle state.
    pub status: SessionStatus,
    /// Process exit code, once available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Stable error code when [`SessionStatus::Failed`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// RFC 3339 UTC creation time.
    pub created_at: String,
    /// RFC 3339 UTC last metadata update.
    pub updated_at: String,
    /// RFC 3339 UTC last PTY activity, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_activity_at: Option<String>,
}

/// Persisted metadata for a managed Git worktree.
///
/// Dirty/clean is not stored here. Call `git.status` for observed tree state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    /// Stable local identifier.
    pub id: WorktreeId,
    /// Project repository that owns the worktree.
    pub project_id: ProjectId,
    /// Session currently associated with this worktree.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    /// Local worktree path.
    pub path: PathBuf,
    /// Checked-out branch name.
    pub branch: String,
    /// Recovery-aware lifecycle of the worktree record.
    pub state: WorktreeState,
    /// RFC 3339 UTC creation time.
    pub created_at: String,
    /// RFC 3339 UTC last metadata update.
    pub updated_at: String,
}

/// Observed Git status for a project root or a managed worktree.
///
/// This is runtime observation, not persisted metadata. Counts come from
/// `git status --porcelain=v2 -z`. Localized human Git output is never parsed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    /// Project whose repository was inspected.
    pub project_id: ProjectId,
    /// Worktree that was inspected, when not the project root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree_id: Option<WorktreeId>,
    /// Current branch name, when HEAD is not detached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Upstream branch name, when configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
    /// Commits ahead of upstream.
    pub ahead: u32,
    /// Commits behind upstream.
    pub behind: u32,
    /// Changed tracked files, including staged and unstaged.
    pub changed_files: u32,
    /// Files with staged hunks.
    pub staged_files: u32,
    /// Untracked files.
    pub untracked_files: u32,
    /// Whether the tree has staged, unstaged, or untracked changes.
    pub is_dirty: bool,
    /// RFC 3339 UTC time this snapshot was observed.
    pub observed_at: String,
}

/// Bounded textual diff for a project root or managed worktree.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitDiff {
    /// Project whose repository was inspected.
    pub project_id: ProjectId,
    /// Worktree that was inspected, when not the project root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree_id: Option<WorktreeId>,
    /// Unified diff text. May be empty.
    pub text: String,
    /// True when the diff exceeded the daemon size cap.
    pub truncated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AgentId, ProjectId, SessionId, WorktreeId};

    const TIMESTAMP: &str = "2026-08-29T01:18:00Z";

    fn assert_json_round_trip<T>(value: &T)
    where
        T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug,
    {
        let json = serde_json::to_string(value).expect("DTO should serialize");
        let decoded: T = serde_json::from_str(&json).expect("DTO should deserialize");
        assert_eq!(&decoded, value);
    }

    #[test]
    fn project_round_trips_without_observed_git_fields() {
        let project = Project {
            id: ProjectId::new(),
            name: "core".to_owned(),
            path: PathBuf::from("/tmp/core"),
            created_at: TIMESTAMP.to_owned(),
            last_opened_at: TIMESTAMP.to_owned(),
        };
        let value = serde_json::to_value(&project).expect("project should serialize");
        assert_eq!(value["lastOpenedAt"], TIMESTAMP);
        assert!(value.get("currentBranch").is_none());
        assert!(value.get("repositoryRoot").is_none());
        assert_json_round_trip(&project);
    }

    #[test]
    fn session_public_dto_omits_pty_and_pid() {
        let session = Session {
            id: SessionId::new(),
            project_id: ProjectId::new(),
            name: "Implement auth".to_owned(),
            agent_id: AgentId::parse_str(AgentId::CODEX).expect("codex"),
            cwd: PathBuf::from("/tmp/worktrees/auth"),
            worktree_id: Some(WorktreeId::new()),
            status: SessionStatus::Running,
            exit_code: None,
            error_code: None,
            created_at: TIMESTAMP.to_owned(),
            updated_at: TIMESTAMP.to_owned(),
            last_activity_at: Some(TIMESTAMP.to_owned()),
        };
        let value = serde_json::to_value(&session).expect("session should serialize");
        assert!(value.get("pid").is_none());
        assert!(value.get("ptyId").is_none());
        assert!(value.get("branch").is_none());
        assert_json_round_trip(&session);
    }

    #[test]
    fn custom_agent_debug_redacts_env_and_args() {
        let agent = CustomAgent {
            id: AgentId::new_custom(),
            display_name: "Company Agent".to_owned(),
            executable: "company-agent".to_owned(),
            args: vec!["--token=secret".to_owned()],
            env: BTreeMap::from([("TOKEN".to_owned(), "secret".to_owned())]),
            enabled: true,
            created_at: TIMESTAMP.to_owned(),
            updated_at: TIMESTAMP.to_owned(),
        };
        let debug = format!("{agent:?}");
        assert!(debug.contains("TOKEN"));
        assert!(!debug.contains("secret"));
        assert_json_round_trip(&agent);
    }

    #[test]
    fn git_status_and_worktree_round_trip() {
        let project_id = ProjectId::new();
        let worktree = Worktree {
            id: WorktreeId::new(),
            project_id,
            session_id: Some(SessionId::new()),
            path: PathBuf::from("/tmp/worktrees/auth"),
            branch: "agent/auth".to_owned(),
            state: WorktreeState::Active,
            created_at: TIMESTAMP.to_owned(),
            updated_at: TIMESTAMP.to_owned(),
        };
        let status = GitStatus {
            project_id,
            worktree_id: Some(worktree.id),
            branch: Some("agent/auth".to_owned()),
            upstream: Some("origin/main".to_owned()),
            ahead: 1,
            behind: 0,
            changed_files: 2,
            staged_files: 1,
            untracked_files: 1,
            is_dirty: true,
            observed_at: TIMESTAMP.to_owned(),
        };
        assert_json_round_trip(&worktree);
        assert_json_round_trip(&status);
    }
}
