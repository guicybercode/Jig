//! Stable domain data transfer objects and the versioned IPC protocol.
//!
//! This crate deliberately contains no process, filesystem, database, or network
//! I/O. Those concerns belong to adapters that consume these types.

#![warn(missing_docs)]

mod command;
mod diagnostics;
mod error;
mod ids;
mod limits;
mod model;
mod protocol;
mod redact;

pub use command::{CommandSpec, CommandSpecError};
pub use diagnostics::{
    AgentDiagnostics, DaemonDiagnostics, DiagnosticsReport, ExecutableDiagnostics, LogRecordDto,
    SqliteDiagnostics,
};
pub use error::{ApiError, ApplicationError, ErrorCode};
pub use ids::{AgentId, ProjectId, RequestId, SessionId, WorktreeId};
pub use limits::{
    COMMAND_OUTPUT_MAX_BYTES, CONFIRMATION_TTL, DIAGNOSTICS_TIMEOUT, DIFF_MAX_BYTES,
    GIT_COMMAND_TIMEOUT, LOG_FILE_MAX_BYTES, LOG_FILE_RETENTION, LOGIN_SHELL_PATH_TIMEOUT,
    OUTPUT_BUFFER_MAX_BYTES, PROCESS_FORCE_KILL_TIMEOUT, PROCESS_STOP_GRACE, RECENT_ERRORS_MAX,
    RECENT_LOGS_MAX, VERSION_DETECT_TIMEOUT,
};
pub use model::{AgentDefinition, AgentSource, Project, Session, SessionStatus, Worktree};
pub use protocol::{
    EnvelopeKind, EventEnvelope, PROTOCOL_V1, RequestEnvelope, ResponseEnvelope, ResponsePayload,
};
pub use redact::{
    REDACTED, is_sensitive_name, redact_json_value, redact_map, redact_text, redact_value,
};
