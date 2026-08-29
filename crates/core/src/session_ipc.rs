//! Stable session IPC method names, event names, and payloads.
//!
//! These types are the daemon/desktop contract. The session crate emits the
//! equivalent in-memory events; the daemon encodes them into envelopes.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};

use crate::{Session, SessionId, SessionStatus};

/// Stable request names for session operations.
pub mod methods {
    /// Create a session and spawn its process.
    pub const CREATE: &str = "session.create";
    /// List known sessions.
    pub const LIST: &str = "session.list";
    /// Fetch one session's public metadata.
    pub const GET: &str = "session.get";
    /// Subscribe to replay snapshot plus live output.
    pub const SUBSCRIBE: &str = "session.subscribe";
    /// Stop receiving live output without stopping the process.
    pub const UNSUBSCRIBE: &str = "session.unsubscribe";
    /// Write raw bytes to the PTY master.
    pub const WRITE: &str = "session.write";
    /// Change PTY rows and columns.
    pub const RESIZE: &str = "session.resize";
    /// Request graceful stop with signal escalation.
    pub const STOP: &str = "session.stop";
    /// Force-kill the session process group.
    pub const KILL: &str = "session.kill";
    /// Stop if needed, then spawn again with the same metadata.
    pub const RESTART: &str = "session.restart";
    /// Rename a session.
    pub const RENAME: &str = "session.rename";
    /// Delete metadata for a session that is not live.
    pub const DELETE: &str = "session.delete";
}

/// Stable event names emitted for session changes.
pub mod events {
    /// A session record was created.
    pub const CREATED: &str = "session.created";
    /// Public session metadata changed.
    pub const UPDATED: &str = "session.updated";
    /// Session metadata was removed.
    pub const DELETED: &str = "session.deleted";
    /// Batched PTY output.
    pub const OUTPUT: &str = "session.output";
    /// A subscriber lagged and must request a new snapshot.
    pub const OUTPUT_GAP: &str = "session.output_gap";
    /// Lifecycle status changed.
    pub const STATUS_CHANGED: &str = "session.status_changed";
    /// The child process exited and an exit code was captured when available.
    pub const EXITED: &str = "session.exited";
}

/// Why a session status changed.
///
/// These reasons use only process and PTY signals. They do not describe
/// vendor-specific agent states such as "thinking".
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusReason {
    /// The child process was spawned in a PTY.
    Spawned,
    /// Validation or spawn failed before a process existed.
    SpawnFailed,
    /// Recent PTY input or output.
    Activity,
    /// The process exists but no PTY activity occurred for the idle interval.
    IdleTimeout,
    /// The client requested a graceful stop.
    StopRequested,
    /// The client requested an immediate kill.
    KillRequested,
    /// The session is being started again with the same identity.
    RestartRequested,
    /// `wait` reported that the child ended.
    ProcessExited,
    /// The PTY reader reached end-of-file.
    ReaderClosed,
}

/// Complete public session record used by created/updated/deleted events.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRecordPayload {
    /// Public session metadata.
    pub session: Session,
}

/// Batched terminal output. Bytes are standard base64 so the JSON envelope can
/// carry arbitrary PTY data.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionOutputPayload {
    /// Session that produced the bytes.
    pub session_id: SessionId,
    /// Monotonic per-session sequence number for this chunk.
    pub sequence: u64,
    /// Standard base64 encoding of the raw PTY bytes.
    pub data: String,
}

impl SessionOutputPayload {
    /// Encodes raw PTY bytes for the wire protocol.
    #[must_use]
    pub fn encode(session_id: SessionId, sequence: u64, bytes: &[u8]) -> Self {
        Self {
            session_id,
            sequence,
            data: STANDARD.encode(bytes),
        }
    }

    /// Decodes the base64 payload into raw PTY bytes.
    ///
    /// # Errors
    ///
    /// Returns an error when the payload is not valid standard base64.
    pub fn decode_bytes(&self) -> Result<Vec<u8>, base64::DecodeError> {
        STANDARD.decode(&self.data)
    }
}

/// Notifies a slow subscriber that it missed live chunks.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionOutputGapPayload {
    /// Session whose live stream was interrupted for this subscriber.
    pub session_id: SessionId,
    /// Highest sequence still retained in the replay buffer.
    pub last_available_sequence: u64,
}

/// Edge-triggered lifecycle transition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatusChangedPayload {
    /// Session whose status changed.
    pub session_id: SessionId,
    /// Status before the transition.
    pub previous: SessionStatus,
    /// Status after the transition.
    pub current: SessionStatus,
    /// Transition time as Unix epoch milliseconds.
    pub at_ms: i64,
    /// Process-level reason for the change.
    pub reason: StatusReason,
}

/// Final process result after `wait` completes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionExitedPayload {
    /// Session that exited.
    pub session_id: SessionId,
    /// Exit code when the process reported one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Terminal status, usually `exited` or `failed`.
    pub status: SessionStatus,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SessionStatus;

    #[test]
    fn method_and_event_names_are_stable() {
        assert_eq!(methods::CREATE, "session.create");
        assert_eq!(methods::SUBSCRIBE, "session.subscribe");
        assert_eq!(methods::WRITE, "session.write");
        assert_eq!(methods::STOP, "session.stop");
        assert_eq!(events::OUTPUT, "session.output");
        assert_eq!(events::OUTPUT_GAP, "session.output_gap");
        assert_eq!(events::STATUS_CHANGED, "session.status_changed");
        assert_eq!(events::EXITED, "session.exited");
    }

    #[test]
    fn output_payload_round_trips_binary_bytes() {
        let session_id = SessionId::new();
        let payload = SessionOutputPayload::encode(session_id, 7, &[0, 1, 255, 10]);
        let json = serde_json::to_value(&payload).expect("payload should serialize");

        assert_eq!(json["sessionId"], session_id.to_string());
        assert_eq!(json["sequence"], 7);
        assert_eq!(
            payload.decode_bytes().expect("bytes should decode"),
            [0, 1, 255, 10]
        );
    }

    #[test]
    fn status_reason_uses_snake_case_wire_values() {
        let encoded =
            serde_json::to_string(&StatusReason::StopRequested).expect("reason should serialize");
        assert_eq!(encoded, "\"stop_requested\"");
        let decoded: SessionStatusChangedPayload = serde_json::from_value(serde_json::json!({
            "sessionId": SessionId::new(),
            "previous": "running",
            "current": "stopping",
            "atMs": 1,
            "reason": "stop_requested"
        }))
        .expect("status payload should deserialize");
        assert_eq!(decoded.current, SessionStatus::Stopping);
        assert_eq!(decoded.reason, StatusReason::StopRequested);
    }
}
