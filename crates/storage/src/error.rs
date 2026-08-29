use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io;

use cli_master_core::ApiError;
use rusqlite::{ErrorCode, ffi};

use crate::LATEST_SCHEMA_VERSION;

/// A failure raised by CLI Master's metadata store.
#[derive(Debug)]
pub enum StorageError {
    /// `SQLite` rejected an operation that was not a recognized constraint failure.
    Database(rusqlite::Error),
    /// The database contains a migration this binary does not understand.
    UnsupportedSchemaVersion(u32),
    /// A migration version stored in `SQLite` cannot be represented by this crate.
    InvalidSchemaVersion(i64),
    /// A recorded migration version has a name that does not match this binary.
    IncompatibleMigration {
        /// Recorded migration version.
        version: u32,
        /// Immutable name expected by this binary.
        expected: &'static str,
        /// Name stored in the database.
        found: String,
    },
    /// The schema version is known but a required object is missing.
    IncompatibleSchema {
        /// Table, column, index, or trigger that failed verification.
        object: &'static str,
    },
    /// A caller supplied a value that cannot be persisted safely.
    InvalidInput {
        /// Input field that failed validation.
        field: &'static str,
        /// Expected shape or invariant.
        reason: String,
    },
    /// A requested metadata row does not exist.
    NotFound {
        /// Kind of metadata being accessed.
        entity: &'static str,
        /// Stable identifier supplied by the caller.
        id: String,
    },
    /// A row conflicts with an existing primary key or unique value.
    AlreadyExists {
        /// Kind of metadata being inserted.
        entity: &'static str,
    },
    /// A referenced row does not exist or is still in use.
    RelationshipViolation {
        /// Kind of metadata being changed.
        entity: &'static str,
        /// Relationship the caller must resolve.
        relationship: &'static str,
    },
    /// Persisted metadata cannot be decoded into the current strongly typed model.
    CorruptData {
        /// Kind of metadata being decoded.
        entity: &'static str,
        /// Column that contains the invalid value.
        field: &'static str,
        /// Decode failure without secret-bearing payloads.
        reason: String,
    },
    /// A filesystem operation required by storage failed.
    Io {
        /// Short operation name without user-controlled content.
        operation: &'static str,
        /// Underlying filesystem error.
        source: io::Error,
    },
    /// Another thread panicked while holding the database connection lock.
    LockPoisoned,
}

impl StorageError {
    /// Converts this failure to a stable IPC error without SQL or bind values.
    #[must_use]
    pub fn to_api_error(&self) -> ApiError {
        let (code, action) = match self {
            Self::UnsupportedSchemaVersion(_)
            | Self::InvalidSchemaVersion(_)
            | Self::IncompatibleMigration { .. }
            | Self::IncompatibleSchema { .. } => (
                "STORAGE_SCHEMA_INCOMPATIBLE",
                "Upgrade CLI Master or restore a database backup created by this version",
            ),
            Self::InvalidInput { .. } => ("STORAGE_INVALID_INPUT", "Correct the input and retry"),
            Self::NotFound { .. } => (
                "STORAGE_NOT_FOUND",
                "Reload the current snapshot and select an existing item",
            ),
            Self::AlreadyExists { .. } => (
                "STORAGE_CONFLICT",
                "Reload the current snapshot and retry with a unique value",
            ),
            Self::RelationshipViolation { .. } => (
                "STORAGE_RELATIONSHIP",
                "Resolve dependent metadata before retrying",
            ),
            Self::CorruptData { .. } => (
                "STORAGE_CORRUPT_DATA",
                "Restore the latest known-good database backup",
            ),
            Self::Io { .. } => (
                "STORAGE_IO",
                "Check filesystem permissions and available disk space, then retry",
            ),
            Self::LockPoisoned => ("STORAGE_LOCK_POISONED", "Restart the daemon"),
            Self::Database(error)
                if matches!(
                    error.sqlite_error_code(),
                    Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked)
                ) =>
            {
                (
                    "STORAGE_BUSY",
                    "Retry after the current database write completes",
                )
            }
            Self::Database(_) => (
                "STORAGE_DATABASE",
                "Restart the daemon; restore a backup if the error persists",
            ),
        };

        ApiError::new(code, self.to_string()).with_action(action)
    }

    /// Returns whether retrying or correcting caller input can recover safely.
    #[must_use]
    pub fn is_recoverable(&self) -> bool {
        !matches!(
            self,
            Self::UnsupportedSchemaVersion(_)
                | Self::InvalidSchemaVersion(_)
                | Self::IncompatibleMigration { .. }
                | Self::IncompatibleSchema { .. }
                | Self::CorruptData { .. }
                | Self::LockPoisoned
        )
    }

    pub(crate) fn io(operation: &'static str, source: io::Error) -> Self {
        Self::Io { operation, source }
    }
}

impl Display for StorageError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => match error.sqlite_error_code() {
                Some(code) => write!(formatter, "SQLite storage operation failed ({code:?})"),
                None => formatter.write_str("SQLite storage operation failed"),
            },
            Self::UnsupportedSchemaVersion(version) => write!(
                formatter,
                "database schema version {version} is newer than supported version {LATEST_SCHEMA_VERSION}"
            ),
            Self::InvalidSchemaVersion(version) => {
                write!(
                    formatter,
                    "database contains invalid schema version {version}"
                )
            }
            Self::IncompatibleMigration {
                version,
                expected,
                found,
            } => write!(
                formatter,
                "database migration {version} is named {found:?}; expected {expected:?}"
            ),
            Self::IncompatibleSchema { object } => {
                write!(formatter, "database schema is missing required {object}")
            }
            Self::InvalidInput { field, reason } => {
                write!(formatter, "invalid {field}: {reason}")
            }
            Self::NotFound { entity, id } => {
                write!(formatter, "{entity} metadata was not found for id {id}")
            }
            Self::AlreadyExists { entity } => {
                write!(
                    formatter,
                    "{entity} metadata already exists with that id or unique value"
                )
            }
            Self::RelationshipViolation {
                entity,
                relationship,
            } => write!(
                formatter,
                "cannot change {entity} metadata until {relationship} is resolved"
            ),
            Self::CorruptData {
                entity,
                field,
                reason,
            } => write!(
                formatter,
                "stored {entity} metadata has invalid {field}: {reason}"
            ),
            Self::Io { operation, source } => {
                write!(formatter, "could not {operation} storage: {source}")
            }
            Self::LockPoisoned => formatter.write_str("database connection lock was poisoned"),
        }
    }
}

impl Error for StorageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::Io { source, .. } => Some(source),
            Self::UnsupportedSchemaVersion(_)
            | Self::InvalidSchemaVersion(_)
            | Self::IncompatibleMigration { .. }
            | Self::IncompatibleSchema { .. }
            | Self::InvalidInput { .. }
            | Self::NotFound { .. }
            | Self::AlreadyExists { .. }
            | Self::RelationshipViolation { .. }
            | Self::CorruptData { .. }
            | Self::LockPoisoned => None,
        }
    }
}

impl From<rusqlite::Error> for StorageError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

pub(crate) fn invalid_input(field: &'static str, reason: impl Into<String>) -> StorageError {
    StorageError::InvalidInput {
        field,
        reason: reason.into(),
    }
}

pub(crate) fn corrupt_data(
    entity: &'static str,
    field: &'static str,
    reason: impl Into<String>,
) -> StorageError {
    StorageError::CorruptData {
        entity,
        field,
        reason: reason.into(),
    }
}

pub(crate) fn persisted_validation(
    entity: &'static str,
    validation: Result<(), StorageError>,
) -> Result<(), StorageError> {
    match validation {
        Ok(()) => Ok(()),
        Err(StorageError::InvalidInput { field, reason }) => Err(StorageError::CorruptData {
            entity,
            field,
            reason,
        }),
        Err(error) => Err(error),
    }
}

pub(crate) fn map_write_error(error: rusqlite::Error, entity: &'static str) -> StorageError {
    match sqlite_extended_code(&error) {
        Some(code)
            if code == ffi::SQLITE_CONSTRAINT_PRIMARYKEY
                || code == ffi::SQLITE_CONSTRAINT_UNIQUE =>
        {
            StorageError::AlreadyExists { entity }
        }
        Some(ffi::SQLITE_CONSTRAINT_FOREIGNKEY) => StorageError::RelationshipViolation {
            entity,
            relationship: "the referenced parent metadata",
        },
        Some(ffi::SQLITE_CONSTRAINT_TRIGGER) => StorageError::RelationshipViolation {
            entity,
            relationship: "the session and worktree must belong to the same project",
        },
        _ => StorageError::Database(error),
    }
}

pub(crate) fn map_delete_error(
    error: rusqlite::Error,
    entity: &'static str,
    relationship: &'static str,
) -> StorageError {
    if sqlite_extended_code(&error).is_some() {
        StorageError::RelationshipViolation {
            entity,
            relationship,
        }
    } else {
        StorageError::Database(error)
    }
}

fn sqlite_extended_code(error: &rusqlite::Error) -> Option<i32> {
    match error {
        rusqlite::Error::SqliteFailure(code, _) if code.code == ErrorCode::ConstraintViolation => {
            Some(code.extended_code)
        }
        _ => None,
    }
}
