use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{AgentId, AgentSource, DaemonInstanceId, Project, Session, Worktree, WorktreeId};

use super::{AgentCommand, ConfirmationToken, DisplayName};

/// Successful response body for mutations with no additional data.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct EmptyResponse {}

/// Successful protocol negotiation response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HelloResponse {
    /// IPC protocol version spoken by the daemon.
    pub protocol_version: u16,
    /// Semantic version of the daemon executable.
    pub daemon_version: String,
    /// Identifier unique to this daemon process lifetime.
    pub instance_id: DaemonInstanceId,
}

/// Public agent definition without a session-specific working directory.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRecord {
    /// Stable local identifier.
    pub id: AgentId,
    /// User-facing agent name.
    pub display_name: DisplayName,
    /// Optional user-facing explanation of the agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether this definition ships with the app or was created by the user.
    pub source: AgentSource,
    /// Structured local launch configuration with no working directory.
    pub command: AgentCommand,
    /// Whether the definition may be selected for a new session.
    pub enabled: bool,
}

/// Result of detecting one configured agent executable.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDetection {
    /// Agent definition that was inspected.
    pub agent_id: AgentId,
    /// Whether a directly executable file was resolved.
    pub available: bool,
    /// Resolved executable path when detection succeeded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable_path: Option<PathBuf>,
    /// Stable safe failure code when detection did not succeed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

/// Durable and runtime state needed to bootstrap a reconnecting client.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateSnapshotResponse {
    /// Applied `SQLite` schema migration version.
    pub schema_version: u32,
    /// Registered projects.
    pub projects: Vec<Project>,
    /// Built-in and custom agent definitions.
    pub agents: Vec<AgentRecord>,
    /// Persisted sessions with current daemon-observed status.
    pub sessions: Vec<Session>,
    /// Managed worktrees.
    pub worktrees: Vec<Worktree>,
}

/// Response containing every registered project.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectListResponse {
    /// Registered projects in daemon-defined display order.
    pub projects: Vec<Project>,
}

/// Successful `project.add` response.
pub type ProjectAddResponse = Project;
/// Successful `project.rename` response.
pub type ProjectRenameResponse = Project;
/// Successful `project.remove` response.
pub type ProjectRemoveResponse = EmptyResponse;

/// Response containing every configured agent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentListResponse {
    /// Built-in and custom agents.
    pub agents: Vec<AgentRecord>,
}

/// Response containing requested executable-detection results.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDetectResponse {
    /// Detection result for every requested definition.
    pub detections: Vec<AgentDetection>,
}

/// Successful `agent.custom.create` response.
pub type AgentCustomCreateResponse = AgentRecord;
/// Successful `agent.custom.update` response.
pub type AgentCustomUpdateResponse = AgentRecord;
/// Successful `agent.set_enabled` response.
pub type AgentSetEnabledResponse = AgentRecord;
/// Successful `agent.custom.remove` response.
pub type AgentCustomRemoveResponse = EmptyResponse;

/// Response containing sessions in daemon-defined display order.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionListResponse {
    /// Sessions matching the optional request filter.
    pub sessions: Vec<Session>,
}

/// Successful `session.create` response.
pub type SessionCreateResponse = Session;
/// Successful `session.rename` response.
pub type SessionRenameResponse = Session;
/// Successful `session.start` response.
pub type SessionStartResponse = Session;
/// Successful `session.restart` response.
pub type SessionRestartResponse = Session;
/// Successful `session.stop` response.
pub type SessionStopResponse = Session;
/// Successful `session.delete` response.
pub type SessionDeleteResponse = EmptyResponse;
/// Successful `session.write` response.
pub type SessionWriteResponse = EmptyResponse;
/// Successful `session.resize` response.
pub type SessionResizeResponse = EmptyResponse;
/// Successful `session.subscribe` response; replay completion arrives as an event.
pub type SessionSubscribeResponse = EmptyResponse;
/// Successful `session.unsubscribe` response.
pub type SessionUnsubscribeResponse = EmptyResponse;

/// High-level classification of a changed Git path.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GitChangeKind {
    /// Existing content or metadata changed.
    Modified,
    /// A path was added, copied, or renamed into place.
    Added,
    /// A tracked path was deleted.
    Deleted,
    /// A path is not tracked by Git.
    Untracked,
    /// A tracked path was renamed or copied from another path.
    Renamed,
    /// A path matches an ignore rule and is reported when ignored files are listed.
    Ignored,
}

/// One changed repository-relative path.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitChangedFile {
    /// Repository-relative UTF-8 display path.
    pub path: String,
    /// Previous path for a rename or copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_path: Option<String>,
    /// High-level UI classification.
    pub kind: GitChangeKind,
    /// Whether the index differs from `HEAD`.
    pub staged: bool,
    /// Whether the working tree differs from the index.
    pub unstaged: bool,
}

/// Aggregate changed-file counts.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusCounts {
    /// Modified tracked paths.
    pub modified: u32,
    /// Added paths that are not renames.
    pub added: u32,
    /// Deleted tracked paths.
    pub deleted: u32,
    /// Untracked paths.
    pub untracked: u32,
    /// Renamed or copied paths.
    #[serde(default)]
    pub renamed: u32,
    /// Ignored paths reported by porcelain.
    #[serde(default)]
    pub ignored: u32,
}

/// Structured Git status for a daemon-resolved target.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct GitStatusResponse {
    /// Symbolic branch; absent for detached or unborn `HEAD`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Changed files in Git's stable porcelain order.
    pub files: Vec<GitChangedFile>,
    /// Counts grouped for the UI.
    pub counts: GitStatusCounts,
    /// Whether the index differs from `HEAD`.
    pub has_staged: bool,
    /// Whether tracked working-tree content differs from the index.
    pub has_tracked_changes: bool,
    /// Whether untracked paths exist.
    pub has_untracked: bool,
    /// Whether any staged, tracked, or untracked change exists.
    pub is_dirty: bool,
}

/// Bounded textual Git diff for a daemon-resolved target.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitDiffResponse {
    /// UTF-8 display text; invalid source bytes may be replaced lossily.
    pub text: String,
    /// Whether the daemon omitted bytes at its fixed safety limit.
    pub truncated: bool,
    /// Whether Git reported binary content instead of a textual patch.
    #[serde(default)]
    pub binary: bool,
}

/// Safe reason a worktree cannot currently be removed.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeRemovalBlocker {
    /// The index differs from `HEAD`.
    StagedChanges,
    /// Tracked working-tree content differs from the index.
    TrackedChanges,
    /// Ordinary untracked files are present.
    UntrackedFiles,
    /// Ignored files are present and would otherwise be silently deleted.
    IgnoredFiles,
    /// At least one index entry is marked `assume-unchanged`.
    AssumeUnchanged,
    /// At least one index entry is marked `skip-worktree`.
    SkipWorktree,
    /// Git has locked the worktree.
    Locked,
    /// An agent process is currently running in the worktree.
    Running,
    /// Another session or live operation currently claims the worktree.
    InUse,
    /// A future daemon reported an unrecognized safe blocker.
    #[serde(other)]
    Unknown,
}

/// Result of re-inspecting a worktree before removal.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    deny_unknown_fields,
    tag = "status",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum WorktreePrepareRemoveResponse {
    /// Current state is clean and unused, so a state-bound token is available.
    Ready {
        /// Worktree whose exact state was inspected.
        worktree_id: WorktreeId,
        /// Required short-lived token bound to the inspected state.
        confirmation_token: ConfirmationToken,
        /// Token expiration as Unix epoch milliseconds.
        expires_at_ms: i64,
    },
    /// Current state is unsafe; no confirmation token can exist in this variant.
    Blocked {
        /// Worktree whose exact state was inspected.
        worktree_id: WorktreeId,
        /// Whether staged, tracked, untracked, or ignored changes were found.
        is_dirty: bool,
        /// Explicit conservative reasons removal is blocked.
        blockers: Vec<WorktreeRemovalBlocker>,
    },
}

/// Successful `worktree.remove` response.
pub type WorktreeRemoveResponse = EmptyResponse;

/// Safe diagnostic issue without terminal output, argument values, or environment values.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticIssue {
    /// Stable machine-readable issue code.
    pub code: String,
    /// Safe user-facing summary.
    pub message: String,
    /// Optional safe remediation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

/// Sanitized daemon diagnostics suitable for display or a support bundle manifest.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsResponse {
    /// Daemon semantic version.
    pub daemon_version: String,
    /// IPC protocol version.
    pub protocol_version: u16,
    /// Applied `SQLite` schema version.
    pub schema_version: u32,
    /// Current daemon process lifetime.
    pub daemon_instance_id: DaemonInstanceId,
    /// `SQLite` data directory.
    pub data_path: PathBuf,
    /// Private runtime/socket directory.
    pub runtime_path: PathBuf,
    /// Sanitized log directory; log contents are not part of this DTO.
    pub log_path: PathBuf,
    /// Executable search directories, never environment values.
    pub effective_path: Vec<PathBuf>,
    /// Recent safe startup or lifecycle issue summaries.
    pub recent_issues: Vec<DiagnosticIssue>,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{AgentId, AgentSource};
    use serde_json::json;

    use super::*;

    #[test]
    fn hello_uses_stable_camel_case_fields() {
        let response = HelloResponse {
            protocol_version: 1,
            daemon_version: "0.1.0".to_owned(),
            instance_id: DaemonInstanceId::new(),
        };
        assert_eq!(
            serde_json::to_value(&response).unwrap(),
            json!({
                "protocolVersion": 1,
                "daemonVersion": "0.1.0",
                "instanceId": response.instance_id
            })
        );
    }

    #[test]
    fn agent_record_round_trips_without_inventing_a_working_directory() {
        let record = AgentRecord {
            id: AgentId::new(),
            display_name: DisplayName::try_new("Codex").unwrap(),
            description: None,
            source: AgentSource::BuiltIn,
            command: AgentCommand::try_new(
                "codex",
                vec!["--interactive".to_owned()],
                BTreeMap::new(),
            )
            .unwrap(),
            enabled: true,
        };
        let value = serde_json::to_value(&record).unwrap();
        assert_eq!(value["displayName"], json!("Codex"));
        assert_eq!(value["command"]["executable"], json!("codex"));
        assert_eq!(value["command"]["args"], json!(["--interactive"]));
        assert!(value["command"].get("cwd").is_none());
        assert_eq!(
            serde_json::from_value::<AgentRecord>(value).unwrap(),
            record
        );
    }

    #[test]
    fn removal_blockers_cover_the_git_safety_service() {
        let blockers = [
            WorktreeRemovalBlocker::StagedChanges,
            WorktreeRemovalBlocker::TrackedChanges,
            WorktreeRemovalBlocker::UntrackedFiles,
            WorktreeRemovalBlocker::IgnoredFiles,
            WorktreeRemovalBlocker::AssumeUnchanged,
            WorktreeRemovalBlocker::SkipWorktree,
            WorktreeRemovalBlocker::Locked,
            WorktreeRemovalBlocker::Running,
            WorktreeRemovalBlocker::InUse,
        ];
        assert_eq!(
            serde_json::to_value(blockers).unwrap(),
            json!([
                "staged_changes",
                "tracked_changes",
                "untracked_files",
                "ignored_files",
                "assume_unchanged",
                "skip_worktree",
                "locked",
                "running",
                "in_use"
            ])
        );
    }

    #[test]
    fn removal_preparation_makes_confirmation_state_unrepresentable_when_blocked() {
        let worktree_id = WorktreeId::new();
        let ready = WorktreePrepareRemoveResponse::Ready {
            worktree_id,
            confirmation_token: ConfirmationToken::try_new("abcdefghijklmnop").unwrap(),
            expires_at_ms: 123,
        };
        assert_eq!(
            serde_json::to_value(&ready).unwrap(),
            json!({
                "status": "ready",
                "worktreeId": worktree_id,
                "confirmationToken": "abcdefghijklmnop",
                "expiresAtMs": 123
            })
        );

        let blocked = json!({
            "status": "blocked",
            "worktreeId": worktree_id,
            "isDirty": true,
            "blockers": ["ignored_files"]
        });
        assert!(serde_json::from_value::<WorktreePrepareRemoveResponse>(blocked.clone()).is_ok());
        let mut invalid_blocked = blocked;
        invalid_blocked["confirmationToken"] = json!("abcdefghijklmnop");
        assert!(serde_json::from_value::<WorktreePrepareRemoveResponse>(invalid_blocked).is_err());
        assert!(
            serde_json::from_value::<WorktreePrepareRemoveResponse>(json!({
                "status": "ready",
                "worktreeId": worktree_id,
                "expiresAtMs": 123
            }))
            .is_err()
        );
    }
}
