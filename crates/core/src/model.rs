use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{AgentId, CommandSpec, ProjectId, SessionId, WorktreeId};

/// Lifecycle state inferred for an agent session.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// The session process is being prepared.
    Starting,
    /// The process is alive and has recent activity.
    Running,
    /// The process is alive but has no recent activity.
    Idle,
    /// A stop was requested and the process group is being signaled.
    Stopping,
    /// The process exited successfully or was stopped.
    Exited,
    /// The process failed to start or exited unsuccessfully.
    Failed,
    /// The process state cannot currently be determined.
    #[serde(other)]
    Unknown,
}

impl SessionStatus {
    /// Returns whether a process is expected to exist for this status.
    #[must_use]
    pub const fn is_live(self) -> bool {
        matches!(
            self,
            Self::Starting | Self::Running | Self::Idle | Self::Stopping
        )
    }
}

/// Origin of an agent definition.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentSource {
    /// Definition supplied with the application.
    BuiltIn,
    /// Definition created explicitly by the user.
    Custom,
}

/// Serializable definition of an available CLI coding agent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDefinition {
    /// Stable local identifier.
    pub id: AgentId,
    /// User-facing agent name.
    pub display_name: String,
    /// Optional user-facing explanation of the agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether this definition is built in or user supplied.
    pub source: AgentSource,
    /// Structured default launch command.
    pub command: CommandSpec,
}

/// Serializable metadata for a registered local project.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    /// Stable local identifier.
    pub id: ProjectId,
    /// User-editable display name.
    pub name: String,
    /// User-selected local path.
    pub path: PathBuf,
    /// Canonical Git repository root, when the project is a repository.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_root: Option<PathBuf>,
    /// Current branch observed during the latest repository refresh.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_branch: Option<String>,
    /// Creation time as Unix epoch milliseconds.
    pub created_at_ms: i64,
    /// Most recent open time as Unix epoch milliseconds.
    pub last_opened_at_ms: i64,
}

/// Serializable metadata for one managed agent session.
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
    /// OS process identifier when a process is attached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    /// Daemon-specific PTY handle when a PTY is attached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pty_id: Option<String>,
    /// Git branch associated with the session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Managed worktree associated with the session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree_id: Option<WorktreeId>,
    /// Local path of the associated worktree, when Git isolation is enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree_path: Option<PathBuf>,
    /// Current inferred lifecycle state.
    pub status: SessionStatus,
    /// Process exit code, once available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Creation time as Unix epoch milliseconds.
    pub created_at_ms: i64,
    /// Most recent metadata update as Unix epoch milliseconds.
    pub updated_at_ms: i64,
}

/// Serializable metadata for a managed Git worktree.
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
    /// Whether the latest Git inspection found uncommitted changes.
    pub is_dirty: bool,
    /// Creation time as Unix epoch milliseconds.
    pub created_at_ms: i64,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn assert_json_round_trip<T>(value: &T)
    where
        T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug,
    {
        let json = serde_json::to_string(value).expect("DTO should serialize");
        let decoded: T = serde_json::from_str(&json).expect("DTO should deserialize");
        assert_eq!(&decoded, value);
    }

    #[test]
    fn unknown_session_status_is_forward_compatible() {
        let status: SessionStatus =
            serde_json::from_str("\"waiting_for_vendor\"").expect("unknown status should decode");
        assert_eq!(status, SessionStatus::Unknown);
    }

    #[test]
    fn live_statuses_match_process_ownership() {
        assert!(SessionStatus::Starting.is_live());
        assert!(SessionStatus::Running.is_live());
        assert!(SessionStatus::Idle.is_live());
        assert!(SessionStatus::Stopping.is_live());
        assert!(!SessionStatus::Exited.is_live());
        assert!(!SessionStatus::Failed.is_live());
        assert!(!SessionStatus::Unknown.is_live());
    }

    #[test]
    fn project_round_trips_with_camel_case_wire_fields() {
        let project = Project {
            id: ProjectId::new(),
            name: "core".to_owned(),
            path: PathBuf::from("/tmp/core"),
            repository_root: Some(PathBuf::from("/tmp/core")),
            current_branch: Some("main".to_owned()),
            created_at_ms: 1,
            last_opened_at_ms: 2,
        };
        let value = serde_json::to_value(&project).expect("project should serialize");
        let decoded: Project =
            serde_json::from_value(value.clone()).expect("project should deserialize");

        assert_eq!(decoded, project);
        assert_eq!(value["lastOpenedAtMs"], 2);
    }

    #[test]
    fn all_known_session_statuses_have_stable_wire_values() {
        let cases = [
            (SessionStatus::Starting, "starting"),
            (SessionStatus::Running, "running"),
            (SessionStatus::Idle, "idle"),
            (SessionStatus::Stopping, "stopping"),
            (SessionStatus::Exited, "exited"),
            (SessionStatus::Failed, "failed"),
            (SessionStatus::Unknown, "unknown"),
        ];

        for (status, wire_value) in cases {
            let encoded = serde_json::to_string(&status).expect("status should serialize");
            assert_eq!(encoded, format!("\"{wire_value}\""));
        }
    }

    #[test]
    fn agent_session_and_worktree_dtos_round_trip() {
        let project_id = ProjectId::new();
        let agent_id = AgentId::new();
        let session_id = SessionId::new();
        let worktree_id = WorktreeId::new();
        let command = CommandSpec::try_from_parts(
            "codex",
            ["--interactive"],
            "/tmp/project",
            BTreeMap::new(),
        )
        .expect("command fixture should be valid");
        let agent = AgentDefinition {
            id: agent_id,
            display_name: "Codex".to_owned(),
            description: Some("Local coding agent".to_owned()),
            source: AgentSource::BuiltIn,
            command,
        };
        let session = Session {
            id: session_id,
            project_id,
            name: "Implement auth".to_owned(),
            agent_id,
            cwd: PathBuf::from("/tmp/worktrees/auth"),
            pid: Some(1_234),
            pty_id: Some("pty-1".to_owned()),
            branch: Some("agent/auth".to_owned()),
            worktree_id: Some(worktree_id),
            worktree_path: Some(PathBuf::from("/tmp/worktrees/auth")),
            status: SessionStatus::Running,
            exit_code: None,
            created_at_ms: 1,
            updated_at_ms: 2,
        };
        let worktree = Worktree {
            id: worktree_id,
            project_id,
            session_id: Some(session_id),
            path: PathBuf::from("/tmp/worktrees/auth"),
            branch: "agent/auth".to_owned(),
            is_dirty: true,
            created_at_ms: 1,
        };

        assert_json_round_trip(&agent);
        assert_json_round_trip(&session);
        assert_json_round_trip(&worktree);
    }
}
