use cli_master_core::{Session, SessionId, SessionStatus};

/// Stable process-level reason for an in-memory lifecycle transition.
///
/// The daemon adapter may expose [`Self::code`] as the optional wire
/// `reasonCode`; this enum is not a second IPC contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusReason {
    /// The child process was spawned in a PTY.
    Spawned,
    /// Validation or spawn failed before a process existed.
    SpawnFailed,
    /// Recent PTY input or output.
    Activity,
    /// The process exists but no PTY activity occurred for the idle interval.
    IdleTimeout,
    /// The daemon requested a graceful stop.
    StopRequested,
    /// Daemon shutdown requested immediate termination.
    KillRequested,
    /// The session is being started again with the same identity.
    RestartRequested,
    /// `wait` reported that the child ended normally.
    ProcessExited,
    /// `wait` failed or the child ended without a successful exit status.
    ProcessFailed,
}

impl StatusReason {
    /// Returns the stable safe code used by logs and wire adapters.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Spawned => "spawned",
            Self::SpawnFailed => "spawn_failed",
            Self::Activity => "activity",
            Self::IdleTimeout => "idle_timeout",
            Self::StopRequested => "stop_requested",
            Self::KillRequested => "kill_requested",
            Self::RestartRequested => "restart_requested",
            Self::ProcessExited => "process_exited",
            Self::ProcessFailed => "process_failed",
        }
    }
}
use tokio::sync::broadcast;

/// One flushed output batch with a per-session sequence number.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputChunk {
    /// Monotonic sequence assigned when the batch was flushed.
    pub sequence: u64,
    /// Raw PTY bytes. Not logged by default.
    pub data: Vec<u8>,
}

/// Recent output retained so a reloading UI can rebuild the terminal view.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputSnapshot {
    /// Sequence of the oldest retained chunk, or `next_sequence` when empty.
    pub first_sequence: u64,
    /// Next sequence that will be assigned.
    pub next_sequence: u64,
    /// Whether earlier bytes were dropped to stay within the memory limit.
    pub truncated: bool,
    /// Retained chunks in sequence order.
    pub chunks: Vec<OutputChunk>,
}

impl OutputSnapshot {
    /// Concatenates retained bytes for tests and small snapshots.
    #[must_use]
    pub fn concatenated(&self) -> Vec<u8> {
        self.chunks
            .iter()
            .flat_map(|chunk| chunk.data.iter().copied())
            .collect()
    }
}

/// In-memory events emitted by [`crate::SessionManager`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionEvent {
    /// A session record was inserted.
    Created(Session),
    /// Public metadata changed.
    Updated(Session),
    /// A session was removed after it stopped.
    Deleted(Session),
    /// Lifecycle status changed.
    StatusChanged {
        /// Session whose status changed.
        session_id: SessionId,
        /// Previous status.
        previous: SessionStatus,
        /// New status.
        current: SessionStatus,
        /// Process-level reason.
        reason: StatusReason,
        /// Transition time as Unix epoch milliseconds.
        at_ms: i64,
    },
    /// Batched PTY output. Subscribers that cannot keep up should request a
    /// snapshot rather than blocking the reader.
    Output {
        /// Session that produced the bytes.
        session_id: SessionId,
        /// Output batch.
        chunk: OutputChunk,
    },
    /// The child process exited.
    Exited {
        /// Session that exited.
        session_id: SessionId,
        /// Exit code when reported by the OS.
        exit_code: Option<i32>,
        /// Final status.
        status: SessionStatus,
        /// Exit observation time as Unix epoch milliseconds.
        at_ms: i64,
    },
}

/// Live output subscription created without stopping the process.
pub struct SessionSubscription {
    /// Metadata at subscribe time.
    pub session: Session,
    /// Bounded recent history.
    pub snapshot: OutputSnapshot,
    receiver: broadcast::Receiver<OutputChunk>,
}

impl SessionSubscription {
    pub(crate) fn new(
        session: Session,
        snapshot: OutputSnapshot,
        receiver: broadcast::Receiver<OutputChunk>,
    ) -> Self {
        Self {
            session,
            snapshot,
            receiver,
        }
    }

    /// Waits for the next live chunk after the snapshot.
    ///
    /// Chunks with a sequence below [`OutputSnapshot::next_sequence`] are
    /// duplicates of snapshot data and are skipped.
    ///
    /// # Errors
    ///
    /// Returns [`SubscribeError::Lagged`] when this subscriber fell behind the
    /// bounded live queue, or [`SubscribeError::Closed`] when the session's
    /// output channel is closed.
    pub async fn next_chunk(&mut self) -> Result<OutputChunk, SubscribeError> {
        let snapshot_next = self.snapshot.next_sequence;
        loop {
            match self.receiver.recv().await {
                Ok(chunk) if chunk.sequence >= snapshot_next => return Ok(chunk),
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    return Err(SubscribeError::Lagged { skipped });
                }
                Err(broadcast::error::RecvError::Closed) => return Err(SubscribeError::Closed),
            }
        }
    }
}

/// Failure while reading a live output subscription.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SubscribeError {
    /// The subscriber was too slow and missed `skipped` messages.
    Lagged {
        /// Number of broadcast messages dropped for this subscriber.
        skipped: u64,
    },
    /// The session output channel is closed.
    Closed,
}

impl std::fmt::Display for SubscribeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lagged { skipped } => {
                write!(formatter, "output subscriber lagged by {skipped} chunks")
            }
            Self::Closed => formatter.write_str("session output channel closed"),
        }
    }
}

impl std::error::Error for SubscribeError {}
