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
mod redact;
pub mod wire;

pub use command::{
    CommandSpec, CommandSpecError, MAX_STARTUP_INPUT_BYTES, validate_structured_invocation,
};
pub use error::{ApiError, ApplicationError};
pub use ids::{AgentId, DaemonInstanceId, ProjectId, RequestId, SessionId, WorktreeId};
pub use model::{
    AgentDefinition, AgentSource, Project, Session, SessionStatus, Worktree, WorktreeState,
};
pub use protocol::{
    EnvelopeKind, EventEnvelope, PROTOCOL_V1, RequestEnvelope, ResponseEnvelope, ResponsePayload,
};
pub use redact::{
    REDACTED, is_sensitive_name, redact_json_value, redact_map, redact_text, redact_value,
};
