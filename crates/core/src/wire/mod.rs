//! Strongly typed Beta v1 request, response, and event payload contracts.
//!
//! Request DTOs accept identifiers and user intent only. Filesystem paths,
//! branch names, working directories, process identifiers, and destructive
//! bypass flags remain daemon-authoritative.

pub mod event_name;
pub mod method;

mod event;
mod git_path;
mod request;
mod response;
mod value;

pub use event::{
    AgentChangedEvent, AgentRemovedEvent, DaemonShuttingDownEvent, GitStatusChangedEvent,
    ProjectChangedEvent, ProjectRemovedEvent, SessionChangedEvent, SessionDeletedEvent,
    SessionExitedEvent, SessionOutputEvent, SessionOutputGapEvent, SessionReplayCompleteEvent,
    SessionStatusChangedEvent, WorktreeChangedEvent, WorktreeRemovedEvent,
};
pub use git_path::GitRelativePath;
pub use request::{
    AgentCommand, AgentCustomCreateRequest, AgentCustomRemoveRequest, AgentCustomUpdateRequest,
    AgentDetectRequest, AgentSetEnabledRequest, EmptyRequest, GitDiffRequest, GitStatusRequest,
    GitTarget, ProjectAddRequest, ProjectRemoveRequest, ProjectRenameRequest, SessionCreateRequest,
    SessionDeleteRequest, SessionIsolation, SessionListRequest, SessionRenameRequest,
    SessionResizeRequest, SessionRestartRequest, SessionStartRequest, SessionStopRequest,
    SessionSubscribeRequest, SessionUnsubscribeRequest, SessionWriteRequest,
    WorktreePrepareRemoveRequest, WorktreeRemoveRequest, validate_agent_command,
};
pub use response::{
    AgentCustomCreateResponse, AgentCustomRemoveResponse, AgentCustomUpdateResponse,
    AgentDetectResponse, AgentDetection, AgentListResponse, AgentRecord, AgentSetEnabledResponse,
    DiagnosticIssue, DiagnosticsResponse, EmptyResponse, GitChangeKind, GitChangedFile,
    GitDiffResponse, GitStatusCounts, GitStatusResponse, HelloResponse, ProjectAddResponse,
    ProjectListResponse, ProjectRemoveResponse, ProjectRenameResponse, SessionCreateResponse,
    SessionDeleteResponse, SessionListResponse, SessionRenameResponse, SessionResizeResponse,
    SessionRestartResponse, SessionStartResponse, SessionStopResponse, SessionSubscribeResponse,
    SessionUnsubscribeResponse, SessionWriteResponse, StateSnapshotResponse,
    WorktreePrepareRemoveResponse, WorktreeRemovalBlocker, WorktreeRemoveResponse,
};
pub use value::{
    ConfirmationToken, DisplayName, ExecutableName, MAX_DISPLAY_NAME_BYTES, MAX_PTY_INPUT_BYTES,
    MAX_PTY_OUTPUT_BYTES, MAX_RELATIVE_DIRECTORY_BYTES, MAX_TERMINAL_DIMENSION, OutputCursor,
    OutputSequence, PtyInputBase64, PtyOutputBase64, RelativeDirectory, SelectedProjectPath,
    TerminalDimension, WireValidationError,
};

/// Request body for `system.hello`.
pub type HelloRequest = EmptyRequest;
/// Request body for `state.snapshot`.
pub type StateSnapshotRequest = EmptyRequest;
/// Request body for `project.list`.
pub type ProjectListRequest = EmptyRequest;
/// Request body for `agent.list`.
pub type AgentListRequest = EmptyRequest;
/// Request body for `diagnostics.get`.
pub type DiagnosticsRequest = EmptyRequest;

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{AgentId, EventEnvelope, ProjectId, RequestEnvelope, SessionId};

    use super::*;

    #[test]
    fn request_envelope_preserves_golden_method_and_payload_names() {
        let payload = SessionCreateRequest {
            project_id: ProjectId::new(),
            name: DisplayName::try_new("Implement IPC").unwrap(),
            agent_id: AgentId::new(),
            isolation: SessionIsolation::Current,
            relative_directory: None,
        };
        let request = RequestEnvelope::v1(method::SESSION_CREATE, payload);
        let value = serde_json::to_value(&request).unwrap();

        assert_eq!(value["method"], json!("session.create"));
        assert_eq!(value["payload"]["isolation"], json!("current"));
        assert!(value["payload"].get("cwd").is_none());
        assert!(value["payload"].get("branch").is_none());
    }

    #[test]
    fn output_event_envelope_preserves_golden_event_name() {
        let payload = SessionOutputEvent {
            session_id: SessionId::new(),
            base64: PtyOutputBase64::try_new("Aw==").unwrap(),
            output_sequence: OutputSequence::new(12),
            replay: false,
        };
        let event = EventEnvelope::v1(event_name::SESSION_OUTPUT, 99, payload);
        let value = serde_json::to_value(&event).unwrap();

        assert_eq!(value["event"], json!("session.output"));
        assert_eq!(value["payload"]["base64"], json!("Aw=="));
        assert_eq!(value["payload"]["outputSequence"], json!(12));
    }

    #[test]
    fn protocol_catalog_contains_expected_beta_names() {
        for expected in [
            method::SYSTEM_HELLO,
            method::STATE_SNAPSHOT,
            method::PROJECT_ADD,
            method::AGENT_CUSTOM_CREATE,
            method::SESSION_CREATE,
            method::SESSION_WRITE,
            method::GIT_STATUS,
            method::WORKTREE_REMOVE,
            method::DIAGNOSTICS_GET,
        ] {
            assert!(method::is_supported(expected));
        }
        for expected in [
            event_name::SESSION_OUTPUT,
            event_name::SESSION_REPLAY_COMPLETE,
            event_name::SESSION_OUTPUT_GAP,
            event_name::SESSION_STATUS_CHANGED,
            event_name::SESSION_EXITED,
        ] {
            assert!(event_name::is_supported(expected));
        }
    }
}
