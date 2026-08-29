use serde::{Deserialize, Serialize};

use crate::{AgentId, Project, ProjectId, Session, SessionId, SessionStatus, Worktree, WorktreeId};

use super::{
    AgentRecord, GitStatusResponse, GitTarget, OutputCursor, OutputSequence, PtyOutputBase64,
};

/// Payload emitted when a project is created or changed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectChangedEvent {
    /// Complete current public project DTO.
    pub project: Project,
}

/// Payload emitted after project metadata is removed.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRemovedEvent {
    /// Removed project identifier.
    pub project_id: ProjectId,
}

/// Payload emitted when an agent definition or availability state changes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentChangedEvent {
    /// Complete current public agent record.
    pub agent: AgentRecord,
}

/// Payload emitted after a custom agent definition is removed.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRemovedEvent {
    /// Removed custom-agent identifier.
    pub agent_id: AgentId,
}

/// Payload emitted when a session is created or its metadata changes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionChangedEvent {
    /// Complete current public session DTO.
    pub session: Session,
}

/// Payload emitted after stopped-session metadata is deleted.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDeletedEvent {
    /// Deleted session identifier.
    pub session_id: SessionId,
}

/// One bounded chunk of arbitrary terminal output.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionOutputEvent {
    /// Session that produced the output.
    pub session_id: SessionId,
    /// Canonical base64 bytes, bounded to 8 KiB after decoding.
    pub base64: PtyOutputBase64,
    /// Monotonic sequence within this session's current PTY lifetime.
    pub output_sequence: OutputSequence,
    /// Whether this chunk came from retained replay instead of the live stream.
    pub replay: bool,
}

/// Marker emitted after all retained output following a subscription cursor.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionReplayCompleteEvent {
    /// Session whose replay reached the live boundary.
    pub session_id: SessionId,
    /// Latest output sequence available when the replay snapshot completed.
    pub output_sequence: OutputSequence,
}

/// Marker emitted when a requested or live terminal range is unavailable.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionOutputGapEvent {
    /// Session whose output cannot be delivered contiguously.
    pub session_id: SessionId,
    /// Cursor originally requested or last successfully delivered.
    pub requested_cursor: OutputCursor,
    /// Earliest sequence still retained by the daemon.
    pub first_available_sequence: OutputSequence,
    /// Latest sequence produced when the gap was detected.
    pub latest_sequence: OutputSequence,
}

/// Payload emitted for an edge-triggered session lifecycle transition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatusChangedEvent {
    /// Session that transitioned.
    pub session_id: SessionId,
    /// Status before the transition.
    pub previous_status: SessionStatus,
    /// Status after the transition.
    pub status: SessionStatus,
    /// Transition time as Unix epoch milliseconds.
    pub changed_at_ms: i64,
    /// Stable safe reason code when one helps explain the transition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
}

/// Payload emitted once a managed process exit is observed.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionExitedEvent {
    /// Session whose process exited.
    pub session_id: SessionId,
    /// OS exit code; absent when termination did not produce one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Final inferred session status.
    pub status: SessionStatus,
    /// Exit observation time as Unix epoch milliseconds.
    pub exited_at_ms: i64,
}

/// Payload emitted when managed worktree metadata changes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeChangedEvent {
    /// Complete current public worktree DTO.
    pub worktree: Worktree,
}

/// Payload emitted after managed worktree metadata is removed.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRemovedEvent {
    /// Removed worktree identifier.
    pub worktree_id: WorktreeId,
}

/// Payload emitted after an explicit or low-rate Git status refresh changes state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusChangedEvent {
    /// Registered entity whose daemon-derived path was inspected.
    pub target: GitTarget,
    /// Complete current structured status.
    pub status: GitStatusResponse,
}

/// Payload emitted when the daemon begins a clean shutdown.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonShuttingDownEvent {
    /// Stable non-sensitive reason code.
    pub reason_code: String,
    /// Number of sessions still live when shutdown began.
    pub active_session_count: u32,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn terminal_output_has_stable_field_names_and_round_trips() {
        let event = SessionOutputEvent {
            session_id: SessionId::new(),
            base64: PtyOutputBase64::try_new("aGVsbG8=").unwrap(),
            output_sequence: OutputSequence::new(7),
            replay: true,
        };
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(
            value,
            json!({
                "sessionId": event.session_id,
                "base64": "aGVsbG8=",
                "outputSequence": 7,
                "replay": true
            })
        );
        assert_eq!(
            serde_json::from_value::<SessionOutputEvent>(value).unwrap(),
            event
        );
    }
}
