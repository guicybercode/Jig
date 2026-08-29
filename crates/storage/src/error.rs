use std::error::Error as StdError;
use std::fmt::{self, Display, Formatter};
use std::io;
use std::path::Path;

use cli_master_core::ApiError;
use rusqlite::{Error as SqliteError, ErrorCode};

/// Domain entity involved in a failed storage operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntityKind {
    /// The database file or connection itself.
    Database,
    /// Schema history or expected columns.
    Schema,
    /// A registered project.
    Project,
    /// A built-in or custom agent definition.
    Agent,
    /// A persisted session.
    Session,
    /// A managed worktree record.
    Worktree,
    /// An application setting row.
    Setting,
    /// An embedded migration.
    Migration,
}

impl EntityKind {
    /// Stable machine-readable entity name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Database => "database",
            Self::Schema => "schema",
            Self::Project => "project",
            Self::Agent => "agent",
            Self::Session => "session",
            Self::Worktree => "worktree",
            Self::Setting => "setting",
            Self::Migration => "migration",
        }
    }
}

/// Classified failure while reading or writing persisted metadata.
#[derive(Debug)]
pub enum StorageErrorKind {
    /// `SQLite` rejected the operation. The summary never includes SQL text.
    Sqlite {
        /// `SQLite` primary error code, when available.
        code: Option<ErrorCode>,
        /// Safe, user-facing summary.
        summary: String,
    },
    /// The database was migrated by a newer binary.
    UnsupportedSchema {
        /// Version recorded in the database.
        found: u32,
        /// Newest version this binary understands.
        supported: u32,
    },
    /// A stored migration version cannot be represented.
    InvalidSchemaVersion(i64),
    /// The schema version matches but required objects are missing or wrong.
    IncompatibleSchema(&'static str),
    /// The file is not a readable `SQLite` database.
    Corrupted,
    /// The requested row does not exist.
    NotFound {
        /// Identifier that was looked up. Never a secret.
        id: String,
    },
    /// A uniqueness or state conflict prevented the write.
    Conflict(&'static str),
    /// Caller-supplied data failed validation.
    InvalidInput(&'static str),
    /// A stored filesystem path does not currently exist.
    PathMissing(String),
    /// An environment key looks like a secret and was refused.
    SecretRejected,
    /// Persisting a full process environment was refused.
    FullEnvironmentRejected,
    /// JSON or timestamp conversion failed.
    Serialization(&'static str),
    /// Filesystem failure outside `SQLite`.
    Io(io::Error),
    /// A poisoned connection lock. The daemon must restart.
    LockPoisoned,
}

/// An error raised while opening, migrating, or querying CLI Master's database.
///
/// Display and [`Self::to_api_error`] omit SQL, bind parameters, and secret
/// values. Callers should log [`Self::operation`], [`Self::entity`], and
/// [`Self::cause`] instead of interpolating `rusqlite` errors.
#[derive(Debug)]
pub struct StorageError {
    operation: &'static str,
    entity: EntityKind,
    kind: StorageErrorKind,
    recovery: &'static str,
}

impl StorageError {
    pub(crate) fn new(
        operation: &'static str,
        entity: EntityKind,
        kind: StorageErrorKind,
        recovery: &'static str,
    ) -> Self {
        Self {
            operation,
            entity,
            kind,
            recovery,
        }
    }

    pub(crate) fn from_sqlite(
        operation: &'static str,
        entity: EntityKind,
        error: &SqliteError,
    ) -> Self {
        let kind = classify_sqlite(error);
        let recovery = recovery_for_kind(&kind);
        Self::new(operation, entity, kind, recovery)
    }

    pub(crate) fn io(operation: &'static str, entity: EntityKind, error: io::Error) -> Self {
        Self::new(
            operation,
            entity,
            StorageErrorKind::Io(error),
            "Check filesystem permissions and available disk space, then retry.",
        )
    }

    pub(crate) fn not_found(operation: &'static str, entity: EntityKind, id: impl Display) -> Self {
        Self::new(
            operation,
            entity,
            StorageErrorKind::NotFound { id: id.to_string() },
            "Reload the application snapshot and select an existing item.",
        )
    }

    pub(crate) fn conflict(
        operation: &'static str,
        entity: EntityKind,
        reason: &'static str,
        recovery: &'static str,
    ) -> Self {
        Self::new(
            operation,
            entity,
            StorageErrorKind::Conflict(reason),
            recovery,
        )
    }

    pub(crate) fn invalid_input(
        operation: &'static str,
        entity: EntityKind,
        reason: &'static str,
    ) -> Self {
        Self::new(
            operation,
            entity,
            StorageErrorKind::InvalidInput(reason),
            "Correct the highlighted field and retry.",
        )
    }

    pub(crate) fn serialization(operation: &'static str, reason: &'static str) -> Self {
        Self::new(
            operation,
            EntityKind::Database,
            StorageErrorKind::Serialization(reason),
            "Restart the daemon. If the error persists, restore the latest database backup.",
        )
    }

    pub(crate) fn path_missing(operation: &'static str, entity: EntityKind, path: &Path) -> Self {
        Self::new(
            operation,
            entity,
            StorageErrorKind::PathMissing(path.display().to_string()),
            "Restore the directory or re-register the project with its new path. Metadata was not deleted.",
        )
    }

    pub(crate) fn lock_poisoned(operation: &'static str) -> Self {
        Self::new(
            operation,
            EntityKind::Database,
            StorageErrorKind::LockPoisoned,
            "Restart the daemon. In-memory state may need reconciliation on startup.",
        )
    }

    /// Operation that failed, such as `insert` or `migrate`.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    /// Entity kind involved in the failure.
    #[must_use]
    pub const fn entity(&self) -> EntityKind {
        self.entity
    }

    /// Classified cause of the failure.
    #[must_use]
    pub const fn kind(&self) -> &StorageErrorKind {
        &self.kind
    }

    /// Suggested recovery action.
    #[must_use]
    pub const fn recovery(&self) -> &'static str {
        self.recovery
    }

    /// Safe, SQL-free explanation of the cause.
    #[must_use]
    pub fn cause(&self) -> String {
        match &self.kind {
            StorageErrorKind::Sqlite { summary, code } => match code {
                Some(code) => format!("{summary} ({code:?})"),
                None => summary.clone(),
            },
            StorageErrorKind::UnsupportedSchema { found, supported } => {
                format!("schema version {found} is newer than supported version {supported}")
            }
            StorageErrorKind::InvalidSchemaVersion(version) => {
                format!("database contains invalid schema version {version}")
            }
            StorageErrorKind::IncompatibleSchema(reason)
            | StorageErrorKind::Conflict(reason)
            | StorageErrorKind::InvalidInput(reason)
            | StorageErrorKind::Serialization(reason) => (*reason).to_owned(),
            StorageErrorKind::Corrupted => {
                "the database file is corrupted or is not SQLite".to_owned()
            }
            StorageErrorKind::NotFound { id } => {
                format!("{} {id} was not found", self.entity.as_str())
            }
            StorageErrorKind::PathMissing(path) => format!("path is missing: {path}"),
            StorageErrorKind::SecretRejected => {
                "refusing to persist a token-like environment variable".to_owned()
            }
            StorageErrorKind::FullEnvironmentRejected => {
                "refusing to persist a full process environment".to_owned()
            }
            StorageErrorKind::Io(error) => error.to_string(),
            StorageErrorKind::LockPoisoned => "database lock was poisoned".to_owned(),
        }
    }

    /// Whether the caller can retry or repair without replacing the database.
    #[must_use]
    pub fn is_recoverable(&self) -> bool {
        match &self.kind {
            StorageErrorKind::Corrupted
            | StorageErrorKind::UnsupportedSchema { .. }
            | StorageErrorKind::InvalidSchemaVersion(_)
            | StorageErrorKind::IncompatibleSchema(_)
            | StorageErrorKind::LockPoisoned => false,
            StorageErrorKind::Sqlite { code, .. } => !matches!(
                code,
                Some(ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase)
            ),
            StorageErrorKind::Io(error) => error.kind() != io::ErrorKind::PermissionDenied,
            _ => true,
        }
    }

    /// Converts this error into a stable IPC error without SQL or secrets.
    #[must_use]
    pub fn to_api_error(&self) -> ApiError {
        ApiError::new(self.api_code(), self.to_string())
            .with_action(self.recovery)
            .with_detail("operation", self.operation)
            .with_detail("entity", self.entity.as_str())
            .with_detail("recoverable", self.is_recoverable())
    }

    fn api_code(&self) -> &'static str {
        match &self.kind {
            StorageErrorKind::Sqlite {
                code: Some(ErrorCode::ConstraintViolation),
                ..
            }
            | StorageErrorKind::Conflict(_) => "STORAGE_CONFLICT",
            StorageErrorKind::Sqlite {
                code: Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked),
                ..
            } => "STORAGE_BUSY",
            StorageErrorKind::Sqlite { .. } => "STORAGE_DATABASE",
            StorageErrorKind::UnsupportedSchema { .. } => "STORAGE_SCHEMA_UNSUPPORTED",
            StorageErrorKind::InvalidSchemaVersion(_) | StorageErrorKind::IncompatibleSchema(_) => {
                "STORAGE_SCHEMA_INCOMPATIBLE"
            }
            StorageErrorKind::Corrupted => "STORAGE_CORRUPTED",
            StorageErrorKind::NotFound { .. } => "STORAGE_NOT_FOUND",
            StorageErrorKind::InvalidInput(_) => "STORAGE_INVALID_INPUT",
            StorageErrorKind::PathMissing(_) => "STORAGE_PATH_MISSING",
            StorageErrorKind::SecretRejected | StorageErrorKind::FullEnvironmentRejected => {
                "STORAGE_SECRET_REJECTED"
            }
            StorageErrorKind::Serialization(_) => "STORAGE_SERIALIZATION",
            StorageErrorKind::Io(_) => "STORAGE_IO",
            StorageErrorKind::LockPoisoned => "STORAGE_LOCK_POISONED",
        }
    }
}

impl Display for StorageError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "failed to {} {}: {}",
            self.operation,
            self.entity.as_str(),
            self.cause()
        )
    }
}

impl StdError for StorageError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match &self.kind {
            StorageErrorKind::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<SqliteError> for StorageError {
    fn from(error: SqliteError) -> Self {
        Self::from_sqlite("database", EntityKind::Database, &error)
    }
}

fn classify_sqlite(error: &SqliteError) -> StorageErrorKind {
    match error {
        SqliteError::QueryReturnedNoRows => StorageErrorKind::NotFound {
            id: "row".to_owned(),
        },
        SqliteError::SqliteFailure(native, _)
            if matches!(
                native.code,
                ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase
            ) =>
        {
            StorageErrorKind::Corrupted
        }
        SqliteError::SqliteFailure(native, message) => StorageErrorKind::Sqlite {
            code: Some(native.code),
            summary: sanitize_sqlite_message(native.code, message.as_deref()),
        },
        SqliteError::SqlInputError { msg, .. } => StorageErrorKind::Sqlite {
            code: None,
            summary: sanitize_free_text(msg),
        },
        _ => StorageErrorKind::Sqlite {
            code: error.sqlite_error_code(),
            summary: "database operation failed".to_owned(),
        },
    }
}

fn sanitize_sqlite_message(code: ErrorCode, message: Option<&str>) -> String {
    match code {
        ErrorCode::ConstraintViolation => "a database constraint was violated".to_owned(),
        ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked => {
            "the database is busy; retry shortly".to_owned()
        }
        ErrorCode::CannotOpen => "the database file could not be opened".to_owned(),
        ErrorCode::ReadOnly => "the database is read-only".to_owned(),
        ErrorCode::DiskFull => "the filesystem is full".to_owned(),
        _ => message
            .map(sanitize_free_text)
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| "database operation failed".to_owned()),
    }
}

fn sanitize_free_text(message: &str) -> String {
    let first_line = message.lines().next().unwrap_or(message).trim();
    if first_line.is_empty()
        || first_line.contains("INSERT")
        || first_line.contains("UPDATE")
        || first_line.contains("DELETE")
        || first_line.contains("SELECT")
        || first_line.contains("CREATE")
    {
        "database operation failed".to_owned()
    } else {
        first_line.to_owned()
    }
}

const fn recovery_for_kind(kind: &StorageErrorKind) -> &'static str {
    match kind {
        StorageErrorKind::UnsupportedSchema { .. } => {
            "Upgrade CLI Master to a version that understands this database."
        }
        StorageErrorKind::InvalidSchemaVersion(_) | StorageErrorKind::IncompatibleSchema(_) => {
            "Restore a backup created by this version, or create a new empty database."
        }
        StorageErrorKind::Corrupted => {
            "Restore the latest timestamped database backup. Metadata was not deleted automatically."
        }
        StorageErrorKind::NotFound { .. } => {
            "Reload the application snapshot and select an existing item."
        }
        StorageErrorKind::Conflict(_) => "Resolve the conflict and retry.",
        StorageErrorKind::InvalidInput(_) => "Correct the highlighted field and retry.",
        StorageErrorKind::PathMissing(_) => {
            "Restore the directory or re-register the project. Metadata was not deleted."
        }
        StorageErrorKind::SecretRejected | StorageErrorKind::FullEnvironmentRejected => {
            "Remove secret or inherited environment values. Agents keep their local authentication."
        }
        StorageErrorKind::Serialization(_) => {
            "Restart the daemon. If the error persists, restore the latest database backup."
        }
        StorageErrorKind::Io(_) => {
            "Check filesystem permissions and available disk space, then retry."
        }
        StorageErrorKind::LockPoisoned => "Restart the daemon so storage can reopen cleanly.",
        StorageErrorKind::Sqlite {
            code: Some(ErrorCode::ConstraintViolation),
            ..
        } => "Correct the conflicting value or remove the blocking record, then retry.",
        StorageErrorKind::Sqlite {
            code: Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked),
            ..
        } => "Retry the operation. The daemon serializes database access.",
        StorageErrorKind::Sqlite { .. } => {
            "Retry the operation. If it persists, inspect diagnostics."
        }
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::*;

    #[test]
    fn sqlite_errors_do_not_include_sql_text() {
        let connection = Connection::open_in_memory().expect("memory db");
        let error = connection
            .execute(
                "INSERT INTO missing_table (token) VALUES ('super-secret')",
                [],
            )
            .expect_err("missing table should fail");
        let storage_error = StorageError::from_sqlite("insert", EntityKind::Session, &error);
        let rendered = storage_error.to_string();
        let api = storage_error.to_api_error();

        assert!(!rendered.contains("INSERT"));
        assert!(!rendered.contains("super-secret"));
        assert_eq!(storage_error.operation(), "insert");
        assert_eq!(storage_error.entity(), EntityKind::Session);
        assert!(storage_error.recovery().contains("Retry"));
        assert_eq!(api.code, "STORAGE_DATABASE");
        assert!(!api.message.contains("INSERT"));
    }
}
