//! Private Unix-socket bridge from the Tauri process to `cli-masterd`.
//!
//! This module forwards versioned wire envelopes. It does not implement
//! `project.*`, `agent.*`, `session.*`, or `git.*` as Tauri commands.

mod backoff;
mod client;
pub(crate) mod commands;
mod error;
mod frame;
mod locate;
mod log;
mod method;
mod relay;
mod session;
mod sidecar;
mod status;

pub(crate) use client::DaemonBridge;

#[cfg(test)]
mod test_support;
