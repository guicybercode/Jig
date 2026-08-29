//! `SQLite` persistence and embedded schema migrations for CLI Master.

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, TransactionBehavior, params};

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

struct Migration {
    version: u32,
    name: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "initial",
    sql: include_str!("../migrations/0001_initial.sql"),
}];

/// The newest schema version understood by this crate.
pub const LATEST_SCHEMA_VERSION: u32 = 1;

/// An error raised while opening or migrating CLI Master's database.
#[derive(Debug)]
pub enum StorageError {
    /// `SQLite` rejected an operation.
    Database(rusqlite::Error),
    /// The database contains a migration this binary does not understand.
    UnsupportedSchemaVersion(u32),
    /// A migration version stored in `SQLite` cannot be represented by this crate.
    InvalidSchemaVersion(i64),
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
        }
    }
}

impl Error for StorageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::UnsupportedSchemaVersion(_) | Self::InvalidSchemaVersion(_) => None,
        }
    }
}

impl From<rusqlite::Error> for StorageError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

/// An owned, configured connection to CLI Master's `SQLite` database.
///
/// Every connection enables foreign-key enforcement, WAL journal mode where
/// `SQLite` supports it, `NORMAL` synchronous writes, and a bounded busy timeout.
#[derive(Debug)]
pub struct Storage {
    connection: Connection,
}

impl Storage {
    /// Opens and configures a file-backed `SQLite` database.
    ///
    /// This does not apply migrations; call [`Self::migrate`] before using the
    /// database.
    ///
    /// # Errors
    ///
    /// Returns an error if `SQLite` cannot open or configure the database.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let connection = Connection::open(path)?;
        Self::from_connection(connection)
    }

    /// Opens and configures an isolated in-memory `SQLite` database.
    ///
    /// This is primarily useful for callers that need an ephemeral store.
    /// `SQLite` keeps its in-memory journal rather than switching it to WAL.
    ///
    /// # Errors
    ///
    /// Returns an error if `SQLite` cannot create or configure the database.
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let connection = Connection::open_in_memory()?;
        Self::from_connection(connection)
    }

    fn from_connection(connection: Connection) -> Result<Self, StorageError> {
        connection.busy_timeout(BUSY_TIMEOUT)?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;

        Ok(Self { connection })
    }

    /// Applies every pending embedded migration in ascending version order.
    ///
    /// Each migration and its history record are committed in one immediate
    /// transaction. Reapplying migrations is safe and has no effect.
    ///
    /// # Errors
    ///
    /// Returns an error when `SQLite` rejects a migration or when the database
    /// contains an unknown or invalid schema version.
    pub fn migrate(&mut self) -> Result<(), StorageError> {
        self.ensure_migration_table()?;

        let applied = self.applied_migrations()?;
        for version in &applied {
            if *version > LATEST_SCHEMA_VERSION
                || !MIGRATIONS
                    .iter()
                    .any(|migration| migration.version == *version)
            {
                return Err(StorageError::UnsupportedSchemaVersion(*version));
            }
        }

        for migration in MIGRATIONS {
            if applied.contains(&migration.version) {
                continue;
            }

            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch(migration.sql)?;
            transaction.execute(
                "INSERT INTO schema_migrations (version, name) VALUES (?1, ?2)",
                params![migration.version, migration.name],
            )?;
            transaction.commit()?;
        }

        Ok(())
    }

    /// Returns the greatest migration version recorded in the database.
    ///
    /// An unmigrated database reports version zero.
    ///
    /// # Errors
    ///
    /// Returns an error if `SQLite` cannot inspect migration history or if the
    /// stored version cannot be represented by this crate.
    pub fn schema_version(&self) -> Result<u32, StorageError> {
        if !self.has_migration_table()? {
            return Ok(0);
        }

        let version = self.connection.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get::<_, i64>(0),
        )?;

        u32::try_from(version).map_err(|_| StorageError::InvalidSchemaVersion(version))
    }

    fn ensure_migration_table(&self) -> Result<(), StorageError> {
        self.connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version     INTEGER PRIMARY KEY CHECK (version > 0),
                name        TEXT NOT NULL CHECK (length(trim(name)) > 0),
                applied_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );",
        )?;
        Ok(())
    }

    fn has_migration_table(&self) -> Result<bool, StorageError> {
        let exists = self.connection.query_row(
            "SELECT EXISTS(
                SELECT 1
                FROM sqlite_schema
                WHERE type = 'table' AND name = 'schema_migrations'
            )",
            [],
            |row| row.get(0),
        )?;
        Ok(exists)
    }

    fn applied_migrations(&self) -> Result<BTreeSet<u32>, StorageError> {
        let mut statement = self
            .connection
            .prepare("SELECT version FROM schema_migrations ORDER BY version")?;
        let rows = statement.query_map([], |row| row.get::<_, i64>(0))?;
        let mut versions = BTreeSet::new();

        for row in rows {
            let raw_version = row?;
            let version = u32::try_from(raw_version)
                .map_err(|_| StorageError::InvalidSchemaVersion(raw_version))?;
            versions.insert(version);
        }

        Ok(versions)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use rusqlite::{ErrorCode, params};
    use tempfile::TempDir;

    use super::{LATEST_SCHEMA_VERSION, Storage};

    const TIMESTAMP: &str = "2026-08-28T18:00:00Z";

    #[test]
    fn migrates_an_empty_database() {
        let temporary_directory = TempDir::new().expect("temporary directory should be created");
        let database_path = temporary_directory.path().join("cli-master.db");
        let mut storage = Storage::open(&database_path).expect("database should open");

        assert_eq!(storage.schema_version().expect("version should load"), 0);
        storage.migrate().expect("empty database should migrate");
        assert_eq!(
            storage.schema_version().expect("version should load"),
            LATEST_SCHEMA_VERSION
        );

        let foreign_keys: bool = storage
            .connection
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .expect("foreign-key setting should load");
        let journal_mode: String = storage
            .connection
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .expect("journal mode should load");
        let busy_timeout: i64 = storage
            .connection
            .pragma_query_value(None, "busy_timeout", |row| row.get(0))
            .expect("busy timeout should load");
        let synchronous: u32 = storage
            .connection
            .pragma_query_value(None, "synchronous", |row| row.get(0))
            .expect("synchronous setting should load");

        assert!(foreign_keys);
        assert_eq!(journal_mode, "wal");
        assert_eq!(busy_timeout, 5_000);
        assert_eq!(synchronous, 1, "SQLite NORMAL mode is numeric value 1");
    }

    #[test]
    fn migration_is_idempotent_after_reopen() {
        let temporary_directory = TempDir::new().expect("temporary directory should be created");
        let database_path = temporary_directory.path().join("cli-master.db");

        {
            let mut storage = Storage::open(&database_path).expect("database should open");
            storage.migrate().expect("first migration should succeed");
            storage
                .migrate()
                .expect("second migration should be a no-op");
        }

        let mut reopened = Storage::open(&database_path).expect("database should reopen");
        reopened
            .migrate()
            .expect("reopen migration should be a no-op");
        let migration_count: u32 = reopened
            .connection
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("migration count should load");

        assert_eq!(migration_count, 1);
        assert_eq!(
            reopened.schema_version().expect("version should load"),
            LATEST_SCHEMA_VERSION
        );
    }

    #[test]
    fn foreign_keys_are_enforced() {
        let storage = migrated_memory_storage();
        insert_agent(&storage);

        let error = storage
            .connection
            .execute(
                "INSERT INTO sessions (
                    id, project_id, agent_id, name, cwd, status, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                params![
                    "session-1",
                    "missing-project",
                    "agent-1",
                    "Session",
                    "/tmp/project",
                    "starting",
                    TIMESTAMP
                ],
            )
            .expect_err("missing project must violate its foreign key");

        assert_eq!(
            error.sqlite_error_code(),
            Some(ErrorCode::ConstraintViolation)
        );
    }

    #[test]
    fn domain_constraints_reject_invalid_values() {
        let storage = migrated_memory_storage();

        assert_constraint_violation(storage.connection.execute(
            "INSERT INTO projects (id, name, path, created_at, last_opened_at)
             VALUES ('project-blank', '  ', '/tmp/blank', ?1, ?1)",
            [TIMESTAMP],
        ));
        assert_constraint_violation(storage.connection.execute(
            "INSERT INTO agents (
                id, source, name, executable, created_at, updated_at
             ) VALUES ('agent-invalid', 'remote', 'Agent', 'agent', ?1, ?1)",
            [TIMESTAMP],
        ));

        insert_project(&storage);
        insert_agent(&storage);
        assert_constraint_violation(storage.connection.execute(
            "INSERT INTO sessions (
                id, project_id, agent_id, name, cwd, status, created_at, updated_at
             ) VALUES (
                'session-invalid', 'project-1', 'agent-1', 'Session', '/tmp/project',
                'paused', ?1, ?1
             )",
            [TIMESTAMP],
        ));
        assert_constraint_violation(storage.connection.execute(
            "INSERT INTO worktrees (
                id, project_id, path, branch, state, created_at, updated_at
             ) VALUES (
                'worktree-invalid', 'project-1', '/tmp/worktree', 'agent/test',
                'deleted', ?1, ?1
             )",
            [TIMESTAMP],
        ));
    }

    #[test]
    fn migration_creates_expected_tables_and_indexes() {
        let storage = migrated_memory_storage();
        let tables = schema_objects(&storage, "table");
        let indexes = schema_objects(&storage, "index");

        let expected_tables = BTreeSet::from([
            "agents".to_owned(),
            "projects".to_owned(),
            "schema_migrations".to_owned(),
            "sessions".to_owned(),
            "settings".to_owned(),
            "worktrees".to_owned(),
        ]);
        let expected_indexes = BTreeSet::from([
            "sessions_by_project_updated".to_owned(),
            "sessions_by_status".to_owned(),
            "worktrees_by_project".to_owned(),
        ]);

        assert_eq!(tables, expected_tables);
        assert!(expected_indexes.is_subset(&indexes));
    }

    #[test]
    fn session_status_accepts_created_and_stopping() {
        let storage = migrated_memory_storage();
        insert_project(&storage);
        insert_agent(&storage);

        for status in ["created", "stopping"] {
            storage
                .connection
                .execute(
                    "INSERT INTO sessions (
                        id, project_id, agent_id, name, cwd, status, created_at, updated_at
                     ) VALUES (?1, 'project-1', 'agent-1', 'Session', '/tmp/project', ?2, ?3, ?3)",
                    params![format!("session-{status}"), status, TIMESTAMP],
                )
                .unwrap_or_else(|_| panic!("{status} should be a legal session status"));
        }
    }

    fn migrated_memory_storage() -> Storage {
        let mut storage = Storage::open_in_memory().expect("in-memory database should open");
        storage.migrate().expect("database should migrate");
        storage
    }

    fn insert_project(storage: &Storage) {
        storage
            .connection
            .execute(
                "INSERT INTO projects (id, name, path, created_at, last_opened_at)
                 VALUES ('project-1', 'Project', '/tmp/project', ?1, ?1)",
                [TIMESTAMP],
            )
            .expect("project fixture should insert");
    }

    fn insert_agent(storage: &Storage) {
        storage
            .connection
            .execute(
                "INSERT INTO agents (
                    id, source, name, executable, created_at, updated_at
                 ) VALUES ('agent-1', 'built_in', 'Agent', 'agent', ?1, ?1)",
                [TIMESTAMP],
            )
            .expect("agent fixture should insert");
    }

    fn assert_constraint_violation(result: rusqlite::Result<usize>) {
        let error = result.expect_err("invalid value must violate a constraint");
        assert_eq!(
            error.sqlite_error_code(),
            Some(ErrorCode::ConstraintViolation)
        );
    }

    fn schema_objects(storage: &Storage, object_type: &str) -> BTreeSet<String> {
        let mut statement = storage
            .connection
            .prepare(
                "SELECT name FROM sqlite_schema
                 WHERE type = ?1 AND name NOT LIKE 'sqlite_%'
                 ORDER BY name",
            )
            .expect("schema query should prepare");
        statement
            .query_map([object_type], |row| row.get(0))
            .expect("schema query should execute")
            .collect::<rusqlite::Result<_>>()
            .expect("schema rows should load")
    }
}
