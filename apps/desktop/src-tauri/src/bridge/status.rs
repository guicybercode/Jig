use cli_master_core::DaemonInstanceId;
use serde::Serialize;

/// Connection lifecycle visible to the webview.
///
/// `Unavailable` means the socket could not be reached. `Incompatible` means a
/// daemon answered `system.hello` with a protocol the desktop process cannot
/// speak. The bridge does not retry an incompatible daemon until the user asks
/// it to reconnect.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum DaemonStatus {
    /// A connect or sidecar spawn is in progress.
    Connecting,
    /// Handshake succeeded and requests may be forwarded.
    Ready {
        /// Protocol version confirmed during `system.hello`.
        protocol_version: u16,
        /// Semantic version reported by the daemon executable.
        daemon_version: String,
        /// Daemon process lifetime identifier.
        instance_id: DaemonInstanceId,
    },
    /// The previous socket closed and another connect attempt will run.
    Reconnecting {
        /// One-based bounded backoff attempt.
        attempt: u32,
    },
    /// Connect attempts were exhausted or the binary could not be started.
    Unavailable {
        /// Non-secret diagnostic reason.
        detail: String,
    },
    /// The daemon spoke a protocol version this desktop build rejects.
    Incompatible {
        /// Protocol version reported by the daemon, when known.
        protocol_version: Option<u16>,
        /// Daemon semantic version, when known.
        daemon_version: Option<String>,
        /// Non-secret diagnostic reason.
        detail: String,
    },
}

/// Frontend event name for relayed daemon event envelopes.
pub const DAEMON_EVENT: &str = "daemon://event";
/// Frontend event name for connection status changes.
pub const DAEMON_STATUS: &str = "daemon://status";
