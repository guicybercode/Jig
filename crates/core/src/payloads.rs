use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{
    AgentDefinition, AgentDetection, AgentId, ApplicationError, CustomAgent, DaemonInstanceId,
    GitStatus, Project, ProjectId, Session, SessionId, SessionStatus, Worktree, WorktreeId,
};

/// Empty request or response body. Serializes as `{}`.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct EmptyPayload {}

/// Handshake sent by a desktop or future CLI client.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HelloRequest {
    /// Protocol version the client wants to speak.
    pub protocol_version: u16,
    /// Short client label such as `desktop`.
    pub client: String,
}

/// Handshake returned by the daemon after version negotiation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HelloResponse {
    /// Protocol version the daemon will speak for this connection.
    pub protocol_version: u16,
    /// Daemon process identity. Changes after every daemon restart.
    pub daemon_instance_id: DaemonInstanceId,
    /// Application semver string.
    pub app_version: String,
    /// `linux` or `macos`.
    pub platform: String,
}

/// Lifecycle of the daemon process itself.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DaemonLifecycle {
    /// The daemon is starting and is not yet accepting requests.
    Starting,
    /// The daemon is accepting requests.
    Ready,
    /// The daemon is shutting down.
    ShuttingDown,
}

/// Public daemon status. No socket paths or PIDs.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonStatus {
    /// Current daemon process identity.
    pub instance_id: DaemonInstanceId,
    /// Process lifecycle.
    pub lifecycle: DaemonLifecycle,
    /// Protocol version currently spoken.
    pub protocol_version: u16,
    /// Application semver string.
    pub app_version: String,
    /// `linux` or `macos`.
    pub platform: String,
}

/// Authoritative metadata snapshot used after connect and reconnect.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateSnapshot {
    /// Current daemon status.
    pub daemon: DaemonStatus,
    /// Registered projects.
    pub projects: Vec<Project>,
    /// Built-in and custom agent catalog.
    pub agents: Vec<AgentDefinition>,
    /// Custom agent records, including argv. Built-ins are not listed here.
    pub custom_agents: Vec<CustomAgent>,
    /// Last known session metadata.
    pub sessions: Vec<Session>,
    /// Managed worktrees.
    pub worktrees: Vec<Worktree>,
}

/// Register a Git repository as a project.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectAddRequest {
    /// User-selected directory. The daemon canonicalizes it to the repo root.
    pub path: PathBuf,
    /// Optional display name. Defaults to the directory name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Identify a project.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIdRequest {
    /// Target project.
    pub project_id: ProjectId,
}

/// Rename a project.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRenameRequest {
    /// Target project.
    pub project_id: ProjectId,
    /// New display name.
    pub name: String,
}

/// List of projects.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectListResponse {
    /// Registered projects.
    pub projects: Vec<Project>,
}

/// List of agent catalog entries.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentListResponse {
    /// Built-in and custom catalog entries.
    pub agents: Vec<AgentDefinition>,
}

/// Detection results for the current launch environment.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDetectResponse {
    /// One result per catalog agent.
    pub detections: Vec<AgentDetection>,
}

/// Create a custom agent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCreateCustomRequest {
    /// Optional stable key. Generated as `UUIDv7` when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<AgentId>,
    /// User-facing name.
    pub display_name: String,
    /// Absolute path or bare executable name.
    pub executable: String,
    /// Ordered argument array.
    #[serde(default)]
    pub args: Vec<String>,
    /// Non-secret environment overrides.
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
}

/// Update a custom agent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentUpdateCustomRequest {
    /// Custom agent to update.
    pub id: AgentId,
    /// User-facing name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Absolute path or bare executable name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable: Option<String>,
    /// Ordered argument array.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    /// Non-secret environment overrides. Replaces the previous map.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<std::collections::BTreeMap<String, String>>,
    /// Enable or disable the agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Delete a custom agent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDeleteCustomRequest {
    /// Custom agent to delete.
    pub id: AgentId,
}

/// List sessions, optionally for one project.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionListRequest {
    /// When set, only sessions for this project are returned.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
}

/// List of sessions.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionListResponse {
    /// Session metadata.
    pub sessions: Vec<Session>,
}

/// Create session metadata. Does not spawn a process.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCreateRequest {
    /// Project that owns the session.
    pub project_id: ProjectId,
    /// Agent to launch later.
    pub agent_id: AgentId,
    /// User-facing session name.
    pub name: String,
    /// Optional worktree to use as cwd. When omitted, the project root is used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree_id: Option<WorktreeId>,
}

/// Identify a session.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionIdRequest {
    /// Target session.
    pub session_id: SessionId,
}

/// Bytes to write to a PTY. The field is standard base64.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionWriteRequest {
    /// Target session.
    pub session_id: SessionId,
    /// Standard-base64 PTY input. Not a command string.
    pub data_base64: String,
}

/// PTY grid size.
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

/// List worktrees for a project.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeListRequest {
    /// Project whose worktrees should be listed.
    pub project_id: ProjectId,
}

/// List of worktrees.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeListResponse {
    /// Managed worktrees.
    pub worktrees: Vec<Worktree>,
}

/// Create a managed worktree.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeCreateRequest {
    /// Project that owns the worktree.
    pub project_id: ProjectId,
    /// Optional branch name. Generated when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Optional slug used when generating the branch name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Prepare or complete worktree removal.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRemoveRequest {
    /// Worktree to remove.
    pub worktree_id: WorktreeId,
    /// Confirmation token returned by a previous prepare step. Required to
    /// actually delete. When omitted, the daemon only inspects safety and
    /// returns a token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmation_token: Option<String>,
    /// Explicit permission to remove a dirty worktree.
    #[serde(default)]
    pub allow_dirty: bool,
}

/// Result of the two-step worktree removal flow.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRemoveResponse {
    /// True when the worktree was deleted.
    pub removed: bool,
    /// Whether uncommitted changes were observed.
    pub is_dirty: bool,
    /// Whether a live session still uses the worktree.
    pub in_use: bool,
    /// Token required to confirm deletion. Present only when not yet removed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmation_token: Option<String>,
}

/// Inspect Git status or diff.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitObserveRequest {
    /// Project whose repository should be inspected.
    pub project_id: ProjectId,
    /// Worktree to inspect. When omitted, the project root is used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree_id: Option<WorktreeId>,
}

/// Sanitized diagnostic snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsSnapshot {
    /// Application semver string.
    pub app_version: String,
    /// Protocol version.
    pub protocol_version: u16,
    /// `linux` or `macos`.
    pub platform: String,
    /// Data directory containing SQLite, with home abbreviated when possible.
    pub data_dir: String,
    /// Schema version currently applied.
    pub schema_version: u32,
    /// Number of live sessions in this daemon instance.
    pub live_session_count: u32,
}

/// PTY output chunk.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionOutputEvent {
    /// Session that produced the bytes.
    pub session_id: SessionId,
    /// Monotonic per-session sequence number.
    pub sequence: u64,
    /// Standard-base64 PTY output.
    pub data_base64: String,
}

/// Session status transition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatusChangedEvent {
    /// Session whose status changed.
    pub session_id: SessionId,
    /// Previous status.
    pub from: SessionStatus,
    /// New status.
    pub to: SessionStatus,
    /// RFC 3339 UTC time of the transition.
    pub changed_at: String,
    /// Optional stable reason code such as `process_exited`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Session process exit.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionExitedEvent {
    /// Session that exited.
    pub session_id: SessionId,
    /// Final status, `exited` or `failed`.
    pub status: SessionStatus,
    /// Process exit code, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Public error, when the exit is classified as failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ApplicationError>,
}

/// Project metadata changed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectChangedEvent {
    /// Change kind.
    pub change: MetadataChange,
    /// Project after the change. Absent when removed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<Project>,
    /// Identifier of a removed project.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
}

/// How a metadata record changed.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataChange {
    /// A record was created.
    Added,
    /// A record was updated.
    Updated,
    /// A record was removed.
    Removed,
}

/// Git status was refreshed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusChangedEvent {
    /// Latest observed status.
    pub status: GitStatus,
}

/// Daemon lifecycle changed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonStatusChangedEvent {
    /// Latest daemon status.
    pub status: DaemonStatus,
}
