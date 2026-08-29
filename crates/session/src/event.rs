use std::fmt;

use cli_master_core::{SessionId, SessionStatus};
use tokio::sync::broadcast;

use crate::TerminalSize;

/// Stable reference returned after a child process is launched.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionHandle {
    /// Stable session identifier.
    pub id: SessionId,
    /// Operating-system child process identifier, when available.
    pub pid: Option<u32>,
}

/// One ordered binary output fragment read from a PTY.
#[derive(Clone, Eq, PartialEq)]
pub struct OutputChunk {
    /// Session that produced the bytes.
    pub session_id: SessionId,
    /// Strictly increasing per-session sequence, beginning at one.
    pub sequence: u64,
    /// Raw terminal bytes; no UTF-8 or ANSI interpretation is applied.
    pub bytes: Vec<u8>,
    /// Wall-clock activity time as Unix epoch milliseconds.
    pub occurred_at_ms: i64,
}

impl fmt::Debug for OutputChunk {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OutputChunk")
            .field("session_id", &self.session_id)
            .field("sequence", &self.sequence)
            .field("byte_len", &self.bytes.len())
            .field("occurred_at_ms", &self.occurred_at_ms)
            .finish()
    }
}

/// Public runtime state used for reconnect and persistence adapters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionSnapshot {
    /// Session identifier.
    pub id: SessionId,
    /// Child process identifier, when available.
    pub pid: Option<u32>,
    /// Current inferred lifecycle state.
    pub status: SessionStatus,
    /// Process exit code once the child is reaped.
    pub exit_code: Option<i32>,
    /// Session creation time as Unix epoch milliseconds.
    pub created_at_ms: i64,
    /// Most recent input or output activity as Unix epoch milliseconds.
    pub last_activity_at_ms: i64,
    /// Current PTY dimensions.
    pub terminal_size: TerminalSize,
}

/// Reason for a public lifecycle transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusChangeReason {
    /// New input or output changed an idle session back to running.
    Activity,
    /// The configured inactivity threshold elapsed.
    IdleTimeout,
    /// The process exited without an explicit stop request.
    ProcessExited,
    /// The process exited after an explicit stop or kill request.
    StopRequested,
    /// The process could no longer be supervised reliably.
    SupervisionLost,
}

/// PTY operation associated with a non-secret runtime I/O failure event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IoOperation {
    /// Reading child output.
    Read,
    /// Writing child input.
    Write,
}

/// Live event emitted by one managed session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionEvent {
    /// Ordered raw terminal output.
    Output(OutputChunk),
    /// A lifecycle state changed.
    StatusChanged {
        /// Target session.
        session_id: SessionId,
        /// State before the transition.
        previous: SessionStatus,
        /// State after the transition.
        current: SessionStatus,
        /// Transition time as Unix epoch milliseconds.
        occurred_at_ms: i64,
        /// Safe machine-readable transition reason.
        reason: StatusChangeReason,
    },
    /// The process reached a terminal state.
    Exited {
        /// Target session.
        session_id: SessionId,
        /// `Exited` for success/stops, otherwise `Failed`.
        status: SessionStatus,
        /// Portable exit code reported by the PTY implementation.
        exit_code: i32,
        /// Reap time as Unix epoch milliseconds.
        occurred_at_ms: i64,
    },
    /// A PTY stream failed without exposing input, output, arguments, or env.
    IoFailure {
        /// Target session.
        session_id: SessionId,
        /// Failed stream operation.
        operation: IoOperation,
        /// Failure time as Unix epoch milliseconds.
        occurred_at_ms: i64,
    },
}

/// Consistent reconnect state plus bounded output after a client cursor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconnectSnapshot {
    /// Current process state.
    pub session: SessionSnapshot,
    /// Retained chunks whose sequence is greater than the requested cursor.
    pub output: Vec<OutputChunk>,
    /// Oldest retained sequence, or `None` before the first output.
    pub first_available_sequence: Option<u64>,
    /// Sequence that will be assigned to the next output chunk.
    pub next_sequence: u64,
    /// Whether output between the cursor and retained history was evicted.
    pub gap: bool,
}

/// Reconnect snapshot paired with a receiver for subsequent live events.
pub struct SessionSubscription {
    /// Snapshot captured after the receiver was registered.
    pub snapshot: ReconnectSnapshot,
    /// Bounded broadcast receiver for subsequent live events.
    pub receiver: broadcast::Receiver<SessionEvent>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_debug_reports_length_without_exposing_terminal_bytes() {
        let chunk = OutputChunk {
            session_id: SessionId::new(),
            sequence: 7,
            bytes: b"terminal-secret".to_vec(),
            occurred_at_ms: 11,
        };

        let debug = format!("{chunk:?}");

        assert!(debug.contains("byte_len: 15"));
        assert!(!debug.contains("terminal-secret"));
    }
}
