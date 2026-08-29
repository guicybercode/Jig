//! PTY session manager that owns child process groups and output buffers.
//!
//! The manager is the in-memory authority for live sessions. SQLite metadata
//! remains the caller's responsibility.

#![warn(missing_docs)]

mod error;
mod manager;

pub use error::SessionError;
pub use manager::{SessionManager, SessionManagerConfig, TerminalSize};
