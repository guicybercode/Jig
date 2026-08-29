use std::{io, path::PathBuf};

use cli_master_core::{SessionId, SessionStatus};
use thiserror::Error;

/// Failure returned by PTY session operations.
#[derive(Debug, Error)]
pub enum SessionError {
    /// A manager resource limit or deadline is invalid.
    #[error("invalid session manager configuration `{field}`: {reason}")]
    InvalidConfiguration {
        /// Invalid configuration field.
        field: &'static str,
        /// Expected constraint.
        reason: &'static str,
    },
    /// Rows and columns must both be non-zero.
    #[error("invalid terminal size rows={rows}, cols={cols}: both must be greater than zero")]
    InvalidTerminalSize {
        /// Requested row count.
        rows: u16,
        /// Requested column count.
        cols: u16,
    },
    /// The command working directory is missing or is not a directory.
    #[error("working directory is unavailable: {path:?}")]
    WorkingDirectoryUnavailable {
        /// Requested working directory.
        path: PathBuf,
    },
    /// The native PTY could not be opened.
    #[error("failed to open a native pseudo-terminal")]
    OpenPty {
        /// Underlying PTY error.
        #[source]
        source: anyhow::Error,
    },
    /// A PTY reader or writer could not be acquired.
    #[error("failed to acquire the pseudo-terminal {stream} stream")]
    OpenPtyStream {
        /// Stream direction.
        stream: &'static str,
        /// Underlying PTY error.
        #[source]
        source: anyhow::Error,
    },
    /// The structured executable could not be launched.
    #[error("failed to spawn executable {executable:?} in the pseudo-terminal")]
    Spawn {
        /// Executable name or path; arguments and environment are omitted.
        executable: String,
        /// Underlying process error.
        #[source]
        source: anyhow::Error,
    },
    /// The PTY backend launched a process without a usable Unix identifier.
    #[error("spawned executable {executable:?} did not expose a usable process identifier")]
    ProcessIdUnavailable {
        /// Executable name or path; arguments and environment are omitted.
        executable: String,
    },
    /// The operating-system process tree could not be inspected safely.
    #[error("failed to inspect the process tree for session {session_id}")]
    ProcessInspection {
        /// Target session identifier.
        session_id: SessionId,
        /// Underlying operating-system error.
        #[source]
        source: io::Error,
    },
    /// A required background worker could not be created.
    #[error("failed to start session worker `{role}`")]
    WorkerStart {
        /// Worker responsibility.
        role: &'static str,
        /// Underlying operating-system error.
        #[source]
        source: io::Error,
    },
    /// A background worker panicked during explicit cleanup.
    #[error("session worker `{role}` terminated unexpectedly")]
    WorkerPanicked {
        /// Worker responsibility.
        role: &'static str,
    },
    /// A background worker did not finish before bounded cleanup elapsed.
    #[error("session worker `{role}` did not stop before the cleanup deadline")]
    WorkerJoinTimedOut {
        /// Worker responsibility.
        role: &'static str,
    },
    /// No runtime exists for the requested identifier.
    #[error("session {session_id} was not found")]
    NotFound {
        /// Requested session identifier.
        session_id: SessionId,
    },
    /// A runtime is already registered for the requested identifier.
    #[error("session {session_id} is already registered")]
    DuplicateSessionId {
        /// Identifier that is already owned by this manager.
        session_id: SessionId,
    },
    /// The manager has begun its one-way shutdown and accepts no new sessions.
    #[error("session manager is shutting down")]
    ManagerShuttingDown,
    /// The operation requires a live process.
    #[error("session {session_id} is not live (status: {status:?})")]
    NotLive {
        /// Target session identifier.
        session_id: SessionId,
        /// Current terminal state.
        status: SessionStatus,
    },
    /// The runtime is still being torn down but no longer accepts interaction.
    #[error("interaction with session {session_id} is unavailable")]
    InteractionUnavailable {
        /// Target session identifier.
        session_id: SessionId,
    },
    /// One input payload exceeded the configured bound.
    #[error("input for session {session_id} has {actual} bytes; maximum is {maximum}")]
    InputTooLarge {
        /// Target session identifier.
        session_id: SessionId,
        /// Received byte count.
        actual: usize,
        /// Configured byte limit.
        maximum: usize,
    },
    /// The bounded writer queue is full.
    #[error("input queue for session {session_id} is full")]
    InputBackpressure {
        /// Target session identifier.
        session_id: SessionId,
    },
    /// The PTY writer is no longer available.
    #[error("input stream for session {session_id} is unavailable")]
    InputUnavailable {
        /// Target session identifier.
        session_id: SessionId,
    },
    /// A queued write was cancelled before any byte reached the PTY.
    #[error("input write for session {session_id} exceeded its deadline before delivery")]
    InputTimedOut {
        /// Target session identifier.
        session_id: SessionId,
    },
    /// A write deadline or I/O failure occurred after delivery may have begun.
    ///
    /// The runtime is invalidated and proven process-tree termination is attempted
    /// before this error is returned. Callers must not retry the input because
    /// any prefix, or the complete payload, may already have reached the child.
    #[error("input delivery for session {session_id} is ambiguous; the session was invalidated")]
    InputDeliveryAmbiguous {
        /// Target session identifier.
        session_id: SessionId,
    },
    /// The operating system rejected a PTY write.
    #[error("input write for session {session_id} failed")]
    WriteFailed {
        /// Target session identifier.
        session_id: SessionId,
    },
    /// The operating system rejected a PTY resize.
    #[error("failed to resize pseudo-terminal for session {session_id}")]
    Resize {
        /// Target session identifier.
        session_id: SessionId,
        /// Underlying PTY error.
        #[source]
        source: anyhow::Error,
    },
    /// A process-tree signal could not be delivered.
    #[error("failed to deliver {signal} to session {session_id}")]
    Signal {
        /// Target session identifier.
        session_id: SessionId,
        /// Signal name.
        signal: &'static str,
        /// Underlying operating-system error.
        #[source]
        source: nix::errno::Errno,
    },
    /// The child process could not be polled or reaped reliably.
    #[error("failed to supervise child process for session {session_id}")]
    Supervision {
        /// Target session identifier.
        session_id: SessionId,
        /// Underlying operating-system error.
        #[source]
        source: io::Error,
    },
    /// The force-kill deadline expired before the child was reaped.
    #[error("session {session_id} did not exit before the kill deadline")]
    StopTimedOut {
        /// Target session identifier.
        session_id: SessionId,
    },
    /// Only completed sessions may be removed from the manager.
    #[error("session {session_id} cannot be removed while status is {status:?}")]
    RemoveLive {
        /// Target session identifier.
        session_id: SessionId,
        /// Current live state.
        status: SessionStatus,
    },
    /// A reconnect cursor points beyond all output produced by the session.
    #[error(
        "replay cursor {requested} for session {session_id} is ahead of latest sequence {latest}"
    )]
    ReplayCursorAhead {
        /// Target session identifier.
        session_id: SessionId,
        /// Requested last-seen sequence.
        requested: u64,
        /// Latest sequence produced by the session.
        latest: u64,
    },
    /// The output sequence counter reached its representable maximum.
    #[error("output sequence for session {session_id} is exhausted")]
    SequenceExhausted {
        /// Target session identifier.
        session_id: SessionId,
    },
}
