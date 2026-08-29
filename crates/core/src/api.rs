//! Versioned request and response bodies shared by the daemon and desktop.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{
    AgentId, AgentSource, Project, ProjectId, Session, SessionId, SessionStatus, Worktree,
    WorktreeId,
};

/// Daemon identity returned by `system.hello`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonHello {
    /// Protocol version accepted by the daemon.
    pub protocol_version: u16,
    /// Application version string, such as `0.1.0-beta.1`.
    pub app_version: String,
    /// Unique identifier for this daemon process.
    pub instance_id: String,
    /// Operating system family: `linux` or `macos`.
    pub platform: String,
}

/// Complete metadata snapshot returned after handshake.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateSnapshot {
    /// Connected daemon identity.
    pub daemon: DaemonHello,
    /// Registered projects.
    pub projects: Vec<Project>,
    /// Built-in and custom agents with detection status.
    pub agents: Vec<AgentInfo>,
    /// Session metadata, including unknown rows after restart.
    pub sessions: Vec<Session>,
    /// Managed worktrees.
    pub worktrees: Vec<Worktree>,
}

/// Public agent listing used by the desktop UI.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentInfo {
    /// Stable registry key.
    pub id: AgentId,
    /// User-facing name.
    pub display_name: String,
    /// Built-in or custom origin.
    pub source: AgentSource,
    /// Whether the agent may be launched.
    pub enabled: bool,
    /// Whether the executable was found in the launch environment.
    pub detected: bool,
    /// Bare name or absolute executable path.
    pub executable: String,
    /// Ordered argument list. Environment values are never included.
    pub args: Vec<String>,
}

/// Request to register a local Git repository.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectAddRequest {
    /// Directory selected by the user. Canonicalized before storage.
    pub path: PathBuf,
    /// Optional display name. Defaults to the repository directory name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Request to rename a project.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRenameRequest {
    /// Project to rename.
    pub project_id: ProjectId,
    /// New display name.
    pub name: String,
}

/// Request to remove application metadata for a project.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRemoveRequest {
    /// Project to unregister.
    pub project_id: ProjectId,
}

/// Request to create or update a custom agent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomAgentRequest {
    /// Optional stable key. Generated when omitted on create.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// User-facing name.
    pub display_name: String,
    /// Absolute path or bare executable name.
    pub executable: String,
    /// Ordered argument array. Never parsed as a shell string.
    #[serde(default)]
    pub args: Vec<String>,
}

/// Request to remove a custom agent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRemoveRequest {
    /// Agent to remove.
    pub agent_id: AgentId,
}

/// Request to create a session.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCreateRequest {
    /// Project that owns the session.
    pub project_id: ProjectId,
    /// Agent used to launch the process.
    pub agent_id: AgentId,
    /// Optional display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// When true, create a new Git worktree before launch.
    #[serde(default)]
    pub create_worktree: bool,
    /// Optional slug used for branch and worktree directory names.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree_slug: Option<String>,
    /// Initial PTY columns.
    #[serde(default = "default_columns")]
    pub cols: u16,
    /// Initial PTY rows.
    #[serde(default = "default_rows")]
    pub rows: u16,
}

/// Request that identifies one session.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionIdRequest {
    /// Target session.
    pub session_id: SessionId,
}

/// Request to rename a session.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRenameRequest {
    /// Target session.
    pub session_id: SessionId,
    /// New display name.
    pub name: String,
}

/// Request to write bytes to a PTY.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionWriteRequest {
    /// Target session.
    pub session_id: SessionId,
    /// Base64-encoded PTY input.
    pub bytes_base64: String,
}

/// Request to resize a PTY.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionResizeRequest {
    /// Target session.
    pub session_id: SessionId,
    /// Column count.
    pub cols: u16,
    /// Row count.
    pub rows: u16,
}

/// Replay snapshot returned by `session.subscribe`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSubscribeResponse {
    /// Session metadata at subscribe time.
    pub session: Session,
    /// Sequence number of the last buffered chunk, or zero if empty.
    pub last_sequence: u64,
    /// Base64-encoded replay buffer.
    pub replay_base64: String,
}

/// Output chunk event payload.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionOutputEvent {
    /// Session that produced the bytes.
    pub session_id: SessionId,
    /// Monotonic per-session sequence.
    pub sequence: u64,
    /// Base64-encoded PTY output.
    pub bytes_base64: String,
}

/// Status transition event payload.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatusEvent {
    /// Session that changed.
    pub session_id: SessionId,
    /// Previous status.
    pub previous: SessionStatus,
    /// New status.
    pub current: SessionStatus,
    /// Optional stable reason code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Request for Git status or diff.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitInspectRequest {
    /// Project whose repository should be inspected.
    pub project_id: ProjectId,
    /// Optional worktree. Defaults to the project root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree_id: Option<WorktreeId>,
}

/// Parsed Git status for one working tree.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    /// Canonical repository root.
    pub repository_root: PathBuf,
    /// Inspected working tree path.
    pub worktree_path: PathBuf,
    /// Current branch, or `HEAD` when detached.
    pub branch: String,
    /// Whether uncommitted, staged, or untracked changes exist.
    pub is_dirty: bool,
    /// Number of changed paths reported by porcelain status.
    pub changed_file_count: u32,
    /// Changed path names, truncated for display.
    pub changed_files: Vec<String>,
}

/// Size-capped textual Git diff.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitDiff {
    /// Diff text, possibly truncated.
    pub text: String,
    /// Whether the diff exceeded the size cap.
    pub truncated: bool,
}

/// Request to create a worktree without immediately starting a session.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeCreateRequest {
    /// Project that owns the repository.
    pub project_id: ProjectId,
    /// Optional slug used in the branch and directory name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
}

/// Request identifying one worktree.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeIdRequest {
    /// Target worktree.
    pub worktree_id: WorktreeId,
}

/// Findings returned before worktree removal.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRemovalPlan {
    /// Worktree under inspection.
    pub worktree_id: WorktreeId,
    /// Whether a live session currently uses this worktree.
    pub in_use: bool,
    /// Whether Git reports uncommitted changes.
    pub is_dirty: bool,
    /// Short-lived token required by `worktree.remove`.
    pub confirmation_token: String,
}

/// Request to complete worktree removal.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRemoveRequest {
    /// Target worktree.
    pub worktree_id: WorktreeId,
    /// Token returned by `worktree.prepare_remove`.
    pub confirmation_token: String,
    /// When true, Git is allowed to remove a dirty worktree after recheck.
    #[serde(default)]
    pub allow_dirty: bool,
}

/// Sanitized local diagnostic summary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    /// Daemon identity.
    pub daemon: DaemonHello,
    /// Database path.
    pub database_path: PathBuf,
    /// Log directory.
    pub log_dir: PathBuf,
    /// Runtime socket path.
    pub socket_path: PathBuf,
    /// Effective executable search path entries.
    pub search_paths: Vec<PathBuf>,
    /// Number of live PTY sessions in this daemon instance.
    pub live_session_count: u32,
    /// Recent sanitized log lines.
    pub recent_log_lines: Vec<String>,
}

const fn default_columns() -> u16 {
    80
}

const fn default_rows() -> u16 {
    24
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PROTOCOL_V1, ProjectId};

    #[test]
    fn snapshot_round_trips() {
        let snapshot = StateSnapshot {
            daemon: DaemonHello {
                protocol_version: PROTOCOL_V1,
                app_version: "0.1.0-beta.1".to_owned(),
                instance_id: "instance-1".to_owned(),
                platform: "linux".to_owned(),
            },
            projects: Vec::new(),
            agents: Vec::new(),
            sessions: Vec::new(),
            worktrees: Vec::new(),
        };
        let json = serde_json::to_string(&snapshot).expect("snapshot should serialize");
        let decoded: StateSnapshot =
            serde_json::from_str(&json).expect("snapshot should deserialize");
        assert_eq!(decoded, snapshot);
        assert!(json.contains("protocolVersion"));
    }

    #[test]
    fn session_create_defaults_pty_size() {
        let json = format!(
            r#"{{"projectId":"{}","agentId":"codex"}}"#,
            ProjectId::new()
        );
        let request: SessionCreateRequest =
            serde_json::from_str(&json).expect("request should deserialize");
        assert_eq!(request.cols, 80);
        assert_eq!(request.rows, 24);
        assert!(!request.create_worktree);
    }
}
