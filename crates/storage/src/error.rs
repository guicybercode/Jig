use std::error::Error;
use std::fmt::{self, Display, Formatter};

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
}

impl Display for StorageError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "SQLite storage error: {error}"),
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
        }
    }
}

impl Error for StorageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::UnsupportedSchemaVersion(_)
            | Self::InvalidSchemaVersion(_)
            | Self::InvalidInput { .. }
            | Self::NotFound { .. }
            | Self::AlreadyExists { .. }
            | Self::RelationshipViolation { .. }
            | Self::CorruptData { .. } => None,
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
