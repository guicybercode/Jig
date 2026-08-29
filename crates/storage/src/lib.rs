//! `SQLite` persistence and startup reconciliation for CLI Master.
//!
//! Persisted paths must be absolute and NUL-free. This layer preserves native
//! path bytes while leaving repository canonicalization to the Git and
//! application service layers. Runtime process ownership remains exclusively
//! in the daemon's in-memory session manager.

mod agents;
mod connection;
mod error;
mod migrate;
mod models;
mod paths;
mod projects;
mod recovery;
mod sessions;
mod settings;
mod values;
mod worktrees;

#[cfg(test)]
mod durability_tests;
#[cfg(test)]
mod repository_tests;

use std::path::Path;

use rusqlite::Connection;

use crate::connection::{StorageLocation, maybe_backup_before_migrate};

pub use connection::Storage;
pub use error::StorageError;
pub use models::{SessionRuntimeUpdate, StoredAgent, StoredSession, StoredWorktree, WorktreeState};
pub use recovery::{ReconciliationEvent, ReconciliationReason, RecoveryContext};

/// The newest schema version understood by this crate.
pub const LATEST_SCHEMA_VERSION: u32 = 3;

impl Storage {
    /// Opens and configures a file-backed `SQLite` database.
    ///
    /// This does not apply migrations; call [`Self::migrate`] or
    /// [`Self::open_migrated`] before using repositories.
    ///
    /// # Errors
    ///
    /// Returns an error if `SQLite` cannot open or configure the database.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = path.as_ref().to_path_buf();
        let connection = Connection::open(&path)?;
        Self::from_connection(connection, StorageLocation::File(path))
    }

    /// Opens, safeguards when needed, and migrates a file-backed database.
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be opened, backed up, migrated,
    /// or verified.
    pub fn open_migrated(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let storage = Self::open(path)?;
        storage.prepare()?;
        Ok(storage)
    }

    /// Opens and configures an isolated in-memory `SQLite` database.
    ///
    /// # Errors
    ///
    /// Returns an error if `SQLite` cannot create or configure the database.
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let connection = Connection::open_in_memory()?;
        Self::from_connection(connection, StorageLocation::Memory)
    }

    /// Opens and migrates an isolated in-memory database.
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be created or migrated.
    pub fn open_in_memory_migrated() -> Result<Self, StorageError> {
        let storage = Self::open_in_memory()?;
        storage.migrate()?;
        Ok(storage)
    }

    fn prepare(&self) -> Result<(), StorageError> {
        self.with_connection_mut("prepare storage", |connection| {
            maybe_backup_before_migrate(connection, &self.location)?;
            migrate::migrate(connection)
        })
    }

    /// Applies every pending embedded migration in ascending version order.
    ///
    /// Each migration and its history row commit in one immediate transaction.
    /// The resulting schema is verified before this method succeeds.
    ///
    /// # Errors
    ///
    /// Returns an error when a migration fails or migration history/schema is
    /// incompatible with this binary.
    pub fn migrate(&self) -> Result<(), StorageError> {
        self.with_connection_mut("migrate", migrate::migrate)
    }

    /// Returns the greatest migration version recorded in the database.
    ///
    /// An unmigrated database reports version zero.
    ///
    /// # Errors
    ///
    /// Returns an error if migration history cannot be read.
    pub fn schema_version(&self) -> Result<u32, StorageError> {
        self.with_connection("schema version", migrate::schema_version)
    }

    /// Checkpoints the WAL while keeping the connection available.
    ///
    /// Dropping [`Storage`] also attempts a passive checkpoint.
    ///
    /// # Errors
    ///
    /// Returns an error when the connection lock or checkpoint fails.
    pub fn close(&self) -> Result<(), StorageError> {
        self.checkpoint()
    }

    /// Returns the database path for file-backed storage.
    #[must_use]
    pub fn database_path(&self) -> Option<&Path> {
        self.file_path()
    }
}

#[cfg(test)]
pub(crate) fn connection_for_tests(
    storage: &Storage,
) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, StorageError> {
    storage
        .connection
        .lock()
        .map_err(|_| StorageError::LockPoisoned)
}
