use std::io;
use std::path::PathBuf;

use thiserror::Error;

/// Failure while configuring, starting, or serving the local daemon.
#[derive(Debug, Error)]
pub enum DaemonError {
    /// No absolute home or XDG data directory was available.
    #[error("could not resolve a per-user home directory")]
    MissingHomeDirectory,
    /// Another process currently owns the per-user daemon lock.
    #[error("another CLI Master daemon already owns {path}")]
    AlreadyRunning {
        /// Lock file that is already held.
        path: PathBuf,
    },
    /// A non-socket filesystem entry occupied the configured socket path.
    #[error("refusing to replace non-socket entry at {path}")]
    SocketPathOccupied {
        /// Path containing the unexpected entry.
        path: PathBuf,
    },
    /// A filesystem or Unix socket operation failed.
    #[error("could not {operation} at {path}: {source}")]
    Io {
        /// Short description of the attempted operation.
        operation: &'static str,
        /// Path involved in the failed operation.
        path: PathBuf,
        /// Underlying operating-system error.
        #[source]
        source: io::Error,
    },
    /// A candidate private directory is a symlink, a non-directory, or owned by
    /// another user.
    #[error("refusing to use untrusted directory {path}: {reason}")]
    UntrustedDirectory {
        /// Path that failed the ownership and type checks.
        path: PathBuf,
        /// Safe explanation of why the path was rejected.
        reason: &'static str,
    },
    /// Opening or migrating the local database failed.
    #[error("could not prepare daemon storage: {0}")]
    Storage(#[from] cli_master_storage::StorageError),
    /// Serializing a protocol response failed.
    #[error("could not serialize daemon response: {0}")]
    Serialize(#[from] serde_json::Error),
}

impl DaemonError {
    pub(crate) fn io(operation: &'static str, path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            operation,
            path: path.into(),
            source,
        }
    }
}
