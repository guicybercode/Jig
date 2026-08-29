use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io;
use std::path::PathBuf;

use cli_master_core::SessionId;

/// Failure while starting, signaling, or reading a live session.
#[derive(Debug)]
pub enum SessionError {
    /// No live session exists for the supplied identifier.
    UnknownSession(SessionId),
    /// The PTY layer rejected a spawn, resize, or IO operation.
    Pty(String),
    /// Writing to the PTY master failed.
    Write(io::Error),
    /// A wait for expected output expired.
    Timeout {
        /// Session that did not produce the expected output.
        session_id: SessionId,
        /// UTF-8 snippet from the replay buffer.
        observed: String,
    },
    /// The working directory is missing.
    InvalidWorkingDirectory(PathBuf),
}

impl Display for SessionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownSession(id) => write!(formatter, "unknown session {id}"),
            Self::Pty(message) => write!(formatter, "PTY error: {message}"),
            Self::Write(error) => write!(formatter, "failed to write to PTY: {error}"),
            Self::Timeout {
                session_id,
                observed,
            } => write!(
                formatter,
                "timed out waiting for session {session_id} output; observed: {observed:?}"
            ),
            Self::InvalidWorkingDirectory(path) => {
                write!(
                    formatter,
                    "session working directory is not a directory: {}",
                    path.display()
                )
            }
        }
    }
}

impl Error for SessionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Write(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for SessionError {
    fn from(value: io::Error) -> Self {
        Self::Write(value)
    }
}
