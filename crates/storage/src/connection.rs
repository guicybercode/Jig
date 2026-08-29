use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use rusqlite::{Connection, Transaction, TransactionBehavior};

use crate::error::{EntityKind, StorageError};
use crate::migrate;

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub(crate) enum StorageLocation {
    Memory,
    File(PathBuf),
}

/// An owned, configured connection to CLI Master's `SQLite` database.
///
/// Every connection enables foreign-key enforcement, a 5 second busy timeout,
/// and `FULL` synchronous writes. File-backed databases use WAL journal mode;
/// in-memory databases keep `SQLite`'s memory journal because WAL requires a
/// file. The daemon holds one connection behind a mutex, matching the v0.1
/// single-writer ownership model.
#[derive(Debug)]
pub struct Storage {
    pub(crate) location: StorageLocation,
    pub(crate) connection: Mutex<Connection>,
}

impl Storage {
    pub(crate) fn from_connection(
        connection: Connection,
        location: StorageLocation,
    ) -> Result<Self, StorageError> {
        configure_connection(&connection)?;
        Ok(Self {
            location,
            connection: Mutex::new(connection),
        })
    }

    pub(crate) fn with_connection<T, F>(
        &self,
        operation: &'static str,
        function: F,
    ) -> Result<T, StorageError>
    where
        F: FnOnce(&Connection) -> Result<T, StorageError>,
    {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StorageError::lock_poisoned(operation))?;
        function(&connection)
    }

    pub(crate) fn with_connection_mut<T, F>(
        &self,
        operation: &'static str,
        function: F,
    ) -> Result<T, StorageError>
    where
        F: FnOnce(&mut Connection) -> Result<T, StorageError>,
    {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StorageError::lock_poisoned(operation))?;
        function(&mut connection)
    }

    /// Runs `function` inside an immediate write transaction.
    ///
    /// The transaction is committed only when `function` returns `Ok`. Any
    /// error rolls the partial writes back.
    ///
    /// # Errors
    ///
    /// Returns an error if the lock is poisoned, the transaction cannot start,
    /// `function` fails, or commit fails.
    pub fn transaction<T, F>(&self, function: F) -> Result<T, StorageError>
    where
        F: FnOnce(&Transaction<'_>) -> Result<T, StorageError>,
    {
        self.with_connection_mut("transaction", |connection| {
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|error| {
                    StorageError::from_sqlite("transaction", EntityKind::Database, &error)
                })?;
            match function(&transaction) {
                Ok(value) => {
                    transaction.commit().map_err(|error| {
                        StorageError::from_sqlite("commit", EntityKind::Database, &error)
                    })?;
                    Ok(value)
                }
                Err(error) => Err(error),
            }
        })
    }

    pub(crate) fn file_path(&self) -> Option<&Path> {
        match &self.location {
            StorageLocation::File(path) => Some(path),
            StorageLocation::Memory => None,
        }
    }

    pub(crate) fn checkpoint(&self) -> Result<(), StorageError> {
        self.with_connection("checkpoint", |connection| {
            let journal_mode: String = connection
                .pragma_query_value(None, "journal_mode", |row| row.get(0))
                .map_err(|error| {
                    StorageError::from_sqlite("checkpoint", EntityKind::Database, &error)
                })?;
            if journal_mode.eq_ignore_ascii_case("wal") {
                connection
                    .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
                    .map_err(|error| {
                        StorageError::from_sqlite("checkpoint", EntityKind::Database, &error)
                    })?;
            }
            Ok(())
        })
    }
}

impl Drop for Storage {
    fn drop(&mut self) {
        if let Ok(connection) = self.connection.lock() {
            let journal_mode = connection
                .pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))
                .unwrap_or_default();
            if journal_mode.eq_ignore_ascii_case("wal") {
                let _ = connection.execute_batch("PRAGMA wal_checkpoint(PASSIVE);");
            }
        }
    }
}

pub(crate) fn configure_connection(connection: &Connection) -> Result<(), StorageError> {
    connection
        .busy_timeout(BUSY_TIMEOUT)
        .map_err(|error| StorageError::from_sqlite("configure", EntityKind::Database, &error))?;
    connection
        .pragma_update(None, "foreign_keys", true)
        .map_err(|error| StorageError::from_sqlite("configure", EntityKind::Database, &error))?;
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .map_err(|error| StorageError::from_sqlite("configure", EntityKind::Database, &error))?;
    // FULL is used because this database stores metadata, not PTY output.
    // Writes are infrequent session/project transitions. See session-03-report.
    connection
        .pragma_update(None, "synchronous", "FULL")
        .map_err(|error| StorageError::from_sqlite("configure", EntityKind::Database, &error))?;
    Ok(())
}

pub(crate) fn maybe_backup_before_migrate(
    connection: &Connection,
    location: &StorageLocation,
) -> Result<Option<PathBuf>, StorageError> {
    let StorageLocation::File(path) = location else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    if migrate::needs_destructive_backup(connection)? {
        Ok(Some(migrate::backup_database(connection, path)?))
    } else {
        Ok(None)
    }
}
