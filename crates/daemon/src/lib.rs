//! Local, per-user session daemon for CLI Master.
//!
//! The daemon owns durable backend resources independently from the desktop
//! window and exposes a small, versioned JSON protocol over a Unix domain
//! socket. Linux and macOS are the supported targets for Beta v0.1.

#![warn(missing_docs)]

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
compile_error!("cli-master-daemon supports Linux and macOS only");

mod config;
mod error;
mod lock;
mod server;

pub use cli_master_core::wire::{HelloResponse, StateSnapshotResponse};
pub use config::DaemonConfig;
pub use error::DaemonError;
pub use server::{Daemon, MAX_FRAME_LENGTH};
