//! Stable domain data transfer objects and the versioned IPC protocol.
//!
//! This crate deliberately contains no process, filesystem, database, or network
//! I/O. Those concerns belong to adapters that consume these types.

#![warn(missing_docs)]

mod command;
mod error;
mod ids;
mod model;
mod protocol;
pub mod wire;

pub use command::{CommandSpec, CommandSpecError};
pub use error::ApiError;
pub use ids::{AgentId, DaemonInstanceId, ProjectId, RequestId, SessionId, WorktreeId};
pub use model::{
    AgentDefinition, AgentSource, Project, Session, SessionStatus, Worktree, WorktreeState,
};
pub use protocol::{
    APPLICATION_VERSION, EnvelopeKind, EventEnvelope, PROTOCOL_V1, RequestEnvelope,
    ResponseEnvelope, ResponsePayload,
};
