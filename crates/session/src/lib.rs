//! PTY-backed session lifecycle for the CLI Master daemon.
//!
//! [`SessionManager`] is the only runtime owner of child processes, PTY
//! masters, and replay buffers. Desktop and React code subscribe to events and
//! must not hold file descriptors or process handles.

#![warn(missing_docs)]

#[cfg(not(unix))]
compile_error!("cli-master-session supports Linux and macOS only");

mod buffer;
mod config;
mod error;
mod event;
mod manager;
mod pty;
mod unix;

pub use config::SessionManagerConfig;
pub use error::SessionError;
pub use event::{
    OutputChunk, OutputSnapshot, SessionEvent, SessionSubscription, StatusReason, SubscribeError,
};
pub use manager::{CreateSession, SessionManager};
pub use pty::PtySize;
