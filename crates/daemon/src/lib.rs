//! Local, per-user session daemon for CLI Master.
//!
//! The daemon owns durable backend resources independently from the desktop
//! window and exposes a small, versioned JSON protocol over a Unix domain
//! socket. Linux and macOS are the supported targets for Beta v0.1.

#![warn(missing_docs)]

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
compile_error!("cli-master-daemon supports Linux and macOS only");

mod client;
mod config;
mod diagnostics;
mod error;
mod events;
mod git_inspection;
mod lock;
mod server;

pub use cli_master_core::wire::{HelloResponse, StateSnapshotResponse};
pub use config::DaemonConfig;
pub use diagnostics::DiagnosticLog;
pub use error::DaemonError;
pub use events::{
    ClientHandle, ClientId, EventBus, EventBusLimits, FanoutEvent, SubscribeError, SubscribeOutcome,
};
pub use server::{Daemon, MAX_FRAME_LENGTH};

const GIT_DIFF_ENVELOPE_HEADROOM: usize = 16 * 1024;
const MAX_JSON_ESCAPE_EXPANSION: usize = 6;

/// Maximum Git diff bytes returned over the Unix-socket IPC.
///
/// The limit reserves envelope headroom and accounts for JSON's worst-case
/// six-byte escape of each input byte. Direct callers of `cli_master_git` may
/// use a larger cap.
pub const MAX_GIT_DIFF_BYTES: usize =
    (MAX_FRAME_LENGTH - GIT_DIFF_ENVELOPE_HEADROOM) / MAX_JSON_ESCAPE_EXPANSION;

#[cfg(test)]
mod tests {
    use cli_master_core::wire::GitDiffResponse;
    use cli_master_core::{RequestId, ResponseEnvelope};

    use super::{MAX_FRAME_LENGTH, MAX_GIT_DIFF_BYTES};

    #[test]
    fn worst_case_json_escaped_diff_fits_one_frame() {
        let response = ResponseEnvelope::success(
            RequestId::new(),
            GitDiffResponse {
                text: "\u{1}".repeat(MAX_GIT_DIFF_BYTES),
                truncated: true,
                binary: false,
            },
        );
        let encoded = serde_json::to_vec(&response).expect("response should encode");

        assert!(encoded.len() <= MAX_FRAME_LENGTH);
    }
}
