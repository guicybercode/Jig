use std::{error::Error, fmt, io, path::PathBuf};

use cli_master_core::{ApiError, SessionId};

/// Failure while creating, I/O-ing, or stopping a session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionError {
    /// No session exists for the requested identifier.
    NotFound(SessionId),
    /// The session already has a live process.
    AlreadyRunning(SessionId),
    /// The session has no live PTY to receive input or resize.
    NotRunning(SessionId),
    /// Metadata cannot be deleted while the process is still live.
    StillRunning(SessionId),
    /// Rows or columns were zero.
    InvalidSize,
    /// The session name was empty after trimming.
    InvalidName,
    /// The working directory is missing or not a directory.
    InvalidWorkingDirectory(PathBuf),
    /// Opening or configuring the PTY failed.
    Pty(String),
    /// The operating system rejected the spawn.
    Spawn(String),
    /// A PTY read or write failed.
    Io(String),
    /// The writer queue stayed full past the write timeout.
    WriteTimeout,
    /// The process group was signaled through SIGKILL but did not exit in time.
    StopTimeout(SessionId),
    /// Signaling the process group failed.
    Signal(String),
}

impl SessionError {
    pub(crate) fn io(error: &io::Error) -> Self {
        Self::Io(error.to_string())
    }

    /// Stable IPC error code for this failure.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "session_not_found",
            Self::AlreadyRunning(_) => "session_already_running",
            Self::NotRunning(_) => "session_not_running",
            Self::StillRunning(_) => "session_still_running",
            Self::InvalidSize => "session_invalid_size",
            Self::InvalidName => "session_invalid_name",
            Self::InvalidWorkingDirectory(_) => "session_invalid_cwd",
            Self::Pty(_) => "session_pty_failed",
            Self::Spawn(_) => "session_spawn_failed",
            Self::Io(_) => "session_io_failed",
            Self::WriteTimeout => "session_write_timeout",
            Self::StopTimeout(_) => "session_stop_timeout",
            Self::Signal(_) => "session_signal_failed",
        }
    }
}

impl fmt::Display for SessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(id) => write!(formatter, "session {id} was not found"),
            Self::AlreadyRunning(id) => write!(formatter, "session {id} is already running"),
            Self::NotRunning(id) => write!(formatter, "session {id} is not running"),
            Self::StillRunning(id) => {
                write!(
                    formatter,
                    "session {id} is still running and cannot be deleted"
                )
            }
            Self::InvalidSize => {
                formatter.write_str("PTY rows and columns must be greater than zero")
            }
            Self::InvalidName => formatter.write_str("session name must not be empty"),
            Self::InvalidWorkingDirectory(path) => {
                write!(
                    formatter,
                    "working directory is not a directory: {}",
                    path.display()
                )
            }
            Self::Pty(message) => write!(formatter, "PTY error: {message}"),
            Self::Spawn(message) => write!(formatter, "failed to spawn process: {message}"),
            Self::Io(message) => write!(formatter, "session I/O error: {message}"),
            Self::WriteTimeout => formatter.write_str("timed out writing to the PTY"),
            Self::StopTimeout(id) => {
                write!(
                    formatter,
                    "session {id} did not exit after signal escalation"
                )
            }
            Self::Signal(message) => write!(formatter, "failed to signal process group: {message}"),
        }
    }
}

impl Error for SessionError {}

impl From<SessionError> for ApiError {
    fn from(error: SessionError) -> Self {
        let code = error.code();
        let message = error.to_string();
        let api = Self::new(code, message);
        match error {
            SessionError::InvalidWorkingDirectory(path) => {
                api.with_detail("cwd", path.display().to_string())
            }
            SessionError::NotFound(id)
            | SessionError::AlreadyRunning(id)
            | SessionError::NotRunning(id)
            | SessionError::StillRunning(id)
            | SessionError::StopTimeout(id) => api.with_detail("sessionId", id.to_string()),
            _ => api,
        }
    }
}
