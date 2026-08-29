//! Stable domain data transfer objects and the versioned IPC protocol.
//!
//! This crate deliberately contains no process, filesystem, database, or network
//! I/O. Those concerns belong to adapters that consume these types.

#![warn(missing_docs)]

mod api;
mod codes;
mod command;
mod error;
mod ids;
mod methods;
mod model;
mod protocol;

pub use api::{
    AgentInfo, AgentRemoveRequest, CustomAgentRequest, DaemonHello, Diagnostics, GitDiff,
    GitInspectRequest, GitStatus, ProjectAddRequest, ProjectRemoveRequest, ProjectRenameRequest,
    SessionCreateRequest, SessionIdRequest, SessionOutputEvent, SessionRenameRequest,
    SessionResizeRequest, SessionStatusEvent, SessionSubscribeResponse, SessionWriteRequest,
    StateSnapshot, WorktreeCreateRequest, WorktreeIdRequest, WorktreeRemovalPlan,
    WorktreeRemoveRequest,
};
pub use command::{CommandSpec, CommandSpecError};
pub use error::ApiError;
pub use ids::{AgentId, AgentIdError, ProjectId, RequestId, SessionId, WorktreeId};
pub use model::{
    AgentDefinition, AgentSource, Project, Session, SessionStatus, Worktree, WorktreeState,
};
pub use protocol::{
    EnvelopeKind, EventEnvelope, PROTOCOL_V1, RequestEnvelope, ResponseEnvelope, ResponsePayload,
};

/// Stable IPC method and event names.
pub mod ipc {
    /// Stable machine-readable error codes returned across IPC.
    pub mod codes {
        pub use crate::codes::*;
    }

    pub use crate::methods::*;
}
