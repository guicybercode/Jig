//! Stable domain data transfer objects and the versioned IPC protocol.
//!
//! This crate deliberately contains no process, filesystem, database, or network
//! I/O. Those concerns belong to adapters that consume these types.

#![warn(missing_docs)]

mod command;
mod error;
mod ids;
mod ipc;
mod model;
mod payloads;
mod protocol;
mod session;

pub use command::{CommandSpec, CommandSpecError};
pub use error::{ApiError, ApplicationError, error_codes};
pub use ids::{
    AgentId, AgentIdError, DaemonInstanceId, ProjectId, RequestId, SessionId, WorktreeId,
};
pub use ipc::{IpcEvent, IpcMethod};
pub use model::{
    AgentDefinition, AgentDetection, AgentSource, CustomAgent, DetectionStatus, GitDiff, GitStatus,
    Project, Session, Worktree, WorktreeState,
};
pub use payloads::{
    AgentCreateCustomRequest, AgentDeleteCustomRequest, AgentDetectResponse, AgentListResponse,
    AgentUpdateCustomRequest, DaemonLifecycle, DaemonStatus, DaemonStatusChangedEvent,
    DiagnosticsSnapshot, EmptyPayload, GitObserveRequest, GitStatusChangedEvent, HelloRequest,
    HelloResponse, MetadataChange, ProjectAddRequest, ProjectChangedEvent, ProjectIdRequest,
    ProjectListResponse, ProjectRenameRequest, SessionCreateRequest, SessionExitedEvent,
    SessionIdRequest, SessionListRequest, SessionListResponse, SessionOutputEvent,
    SessionResizeRequest, SessionStatusChangedEvent, SessionWriteRequest, StateSnapshot,
    WorktreeCreateRequest, WorktreeListRequest, WorktreeListResponse, WorktreeRemoveRequest,
    WorktreeRemoveResponse,
};
pub use protocol::{
    EnvelopeKind, EventEnvelope, PROTOCOL_V1, RequestEnvelope, ResponseEnvelope, ResponsePayload,
};
pub use session::{SessionStatus, SessionTransitionError};
