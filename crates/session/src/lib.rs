//! Concurrent pseudo-terminal session ownership for CLI Master.
//!
//! The manager launches structured commands directly in native PTYs, keeps
//! bounded in-memory output history for reconnecting clients, and isolates all
//! lifecycle operations to the target session.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod config;
mod create;
mod error;
mod event;
mod lock;
mod manager;
mod replay;
mod runtime;
mod size;
mod state;
mod worker;

pub use config::{
    DEFAULT_EVENT_CAPACITY, DEFAULT_MAX_WRITE_BYTES, DEFAULT_READ_CHUNK_BYTES,
    DEFAULT_REPLAY_MAX_BYTES, DEFAULT_REPLAY_MAX_CHUNKS, MAX_EVENT_CAPACITY, MAX_TRACKED_PROCESSES,
    SessionManagerConfig,
};
pub use error::SessionError;
pub use event::{
    IoOperation, OutputChunk, ReconnectSnapshot, SessionEvent, SessionHandle, SessionSnapshot,
    SessionSubscription, StatusChangeReason,
};
pub use manager::SessionManager;
pub use size::TerminalSize;
