use std::collections::BTreeSet;
use std::path::Path;

use rusqlite::{Connection, TransactionBehavior, params};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::LATEST_SCHEMA_VERSION;
use crate::error::{EntityKind, StorageError, StorageErrorKind};

struct Migration {
    version: u32,
    name: &'static str,
    sql: &'static str,
    destructive: bool,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "initial",
        sql: include_str!("../migrations/0001_initial.sql"),
        destructive: false,
    },
    Migration {
        version: 2,
        name: "recovery_metadata",
        sql: include_str!("../migrations/0002_recovery_metadata.sql"),
        destructive: false,
    },
];

const REQUIRED_TABLES: &[(&str, &[&str])] = &[
    (
        "projects",
        &[
            "id",
            "name",
            "path",
            "repository_root",
            "created_at",
            "updated_at",
            "last_opened_at",
        ],
    ),
    (
        "agents",
        &[
            "id",
            "source",
            "name",
            "executable",
            "args_json",
            "env_json",
            "enabled",
            "created_at",
            "updated_at",
        ],
    ),
    (
        "sessions",
        &[
            "id",
            "project_id",
            "agent_id",
            "name",
            "cwd",
            "status",
            "runtime_pid",
            "daemon_instance_id",
            "exit_code",
            "error_code",
            "created_at",
            "updated_at",
            "last_activity_at",
            "branch",
            "worktree_path",
            "started_at",
            "exited_at",
        ],
    ),
    (
        "worktrees",
        &[
            "id",
            "project_id",
            "session_id",
            "path",
            "branch",
            "state",
            "created_at",
            "updated_at",
        ],
    ),
    ("settings", &["key", "value_json", "updated_at"]),
    ("schema_migrations", &["version", "name", "applied_at"]),
];

pub(crate) fn ensure_migration_table(connection: &Connection) -> Result<(), StorageError> {
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version     INTEGER PRIMARY KEY CHECK (version > 0),
                name        TEXT NOT NULL CHECK (length(trim(name)) > 0),
                applied_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );",
        )
        .map_err(|error| {
            StorageError::from_sqlite("prepare migrations", EntityKind::Migration, &error)
        })
}

pub(crate) fn has_migration_table(connection: &Connection) -> Result<bool, StorageError> {
    connection
        .query_row(
            "SELECT EXISTS(
                SELECT 1
                FROM sqlite_schema
                WHERE type = 'table' AND name = 'schema_migrations'
            )",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::from_sqlite("inspect schema", EntityKind::Schema, &error))
}

pub(crate) fn schema_version(connection: &Connection) -> Result<u32, StorageError> {
    if !has_migration_table(connection)? {
        return Ok(0);
    }

    let version = connection
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| {
            StorageError::from_sqlite("read schema version", EntityKind::Schema, &error)
        })?;

    u32::try_from(version).map_err(|_| {
        StorageError::new(
            "read schema version",
            EntityKind::Schema,
            StorageErrorKind::InvalidSchemaVersion(version),
            "Restore a backup created by this version, or create a new empty database.",
        )
    })
}

pub(crate) fn applied_migrations(connection: &Connection) -> Result<BTreeSet<u32>, StorageError> {
    let mut statement = connection
        .prepare("SELECT version FROM schema_migrations ORDER BY version")
        .map_err(|error| {
            StorageError::from_sqlite("list migrations", EntityKind::Migration, &error)
        })?;
    let rows = statement
        .query_map([], |row| row.get::<_, i64>(0))
        .map_err(|error| {
            StorageError::from_sqlite("list migrations", EntityKind::Migration, &error)
        })?;
    let mut versions = BTreeSet::new();

    for row in rows {
        let raw_version = row.map_err(|error| {
            StorageError::from_sqlite("list migrations", EntityKind::Migration, &error)
        })?;
        let version = u32::try_from(raw_version).map_err(|_| {
            StorageError::new(
                "list migrations",
                EntityKind::Schema,
                StorageErrorKind::InvalidSchemaVersion(raw_version),
                "Restore a backup created by this version, or create a new empty database.",
            )
        })?;
        versions.insert(version);
    }

    Ok(versions)
}

pub(crate) fn needs_destructive_backup(connection: &Connection) -> Result<bool, StorageError> {
    ensure_migration_table(connection)?;
    let applied = applied_migrations(connection)?;
    Ok(MIGRATIONS
        .iter()
        .any(|migration| migration.destructive && !applied.contains(&migration.version)))
}

pub(crate) fn backup_database(
    connection: &Connection,
    source: &Path,
) -> Result<std::path::PathBuf, StorageError> {
    let timestamp = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "unknown".to_owned())
        .replace(':', "");
    let file_name = format!(
        "{}-{timestamp}.bak",
        source
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("cli-master")
    );
    let destination = source
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(file_name);

    connection
        .backup(rusqlite::MAIN_DB, &destination, None)
        .map_err(|error| {
            StorageError::from_sqlite("backup database", EntityKind::Database, &error)
        })?;
    Ok(destination)
}

pub(crate) fn migrate(connection: &mut Connection) -> Result<(), StorageError> {
    ensure_migration_table(connection)?;
    let applied = applied_migrations(connection)?;
    validate_applied(&applied)?;

    for migration in MIGRATIONS {
        if applied.contains(&migration.version) {
            continue;
        }

        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| StorageError::from_sqlite("migrate", EntityKind::Migration, &error))?;
        transaction
            .execute_batch(migration.sql)
            .map_err(|error| StorageError::from_sqlite("migrate", EntityKind::Migration, &error))?;
        transaction
            .execute(
                "INSERT INTO schema_migrations (version, name) VALUES (?1, ?2)",
                params![migration.version, migration.name],
            )
            .map_err(|error| StorageError::from_sqlite("migrate", EntityKind::Migration, &error))?;
        transaction
            .commit()
            .map_err(|error| StorageError::from_sqlite("migrate", EntityKind::Migration, &error))?;
    }

    verify_schema(connection)
}

pub(crate) fn verify_schema(connection: &Connection) -> Result<(), StorageError> {
    for (table, columns) in REQUIRED_TABLES {
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?1
                )",
                [table],
                |row| row.get(0),
            )
            .map_err(|error| {
                StorageError::from_sqlite("verify schema", EntityKind::Schema, &error)
            })?;
        if !exists {
            return Err(incompatible(table));
        }

        let mut statement = connection
            .prepare("SELECT name FROM pragma_table_info(?1)")
            .map_err(|error| {
                StorageError::from_sqlite("verify schema", EntityKind::Schema, &error)
            })?;
        let present = statement
            .query_map([table], |row| row.get::<_, String>(0))
            .map_err(|error| {
                StorageError::from_sqlite("verify schema", EntityKind::Schema, &error)
            })?
            .collect::<Result<BTreeSet<_>, _>>()
            .map_err(|error| {
                StorageError::from_sqlite("verify schema", EntityKind::Schema, &error)
            })?;

        for column in *columns {
            if !present.contains(*column) {
                return Err(incompatible(table));
            }
        }
    }
    Ok(())
}

fn validate_applied(applied: &BTreeSet<u32>) -> Result<(), StorageError> {
    for version in applied {
        if *version > LATEST_SCHEMA_VERSION
            || !MIGRATIONS
                .iter()
                .any(|migration| migration.version == *version)
        {
            return Err(StorageError::new(
                "migrate",
                EntityKind::Schema,
                StorageErrorKind::UnsupportedSchema {
                    found: *version,
                    supported: LATEST_SCHEMA_VERSION,
                },
                "Upgrade CLI Master to a version that understands this database.",
            ));
        }
    }
    Ok(())
}

fn incompatible(table: &'static str) -> StorageError {
    let reason = match table {
        "projects" => "projects table is missing required recovery columns",
        "agents" => "agents table is missing required columns",
        "sessions" => "sessions table is missing required recovery columns",
        "worktrees" => "worktrees table is missing required columns",
        "settings" => "settings table is missing required columns",
        "schema_migrations" => "schema_migrations table is missing required columns",
        _ => "database schema is incompatible with this version",
    };
    StorageError::new(
        "verify schema",
        EntityKind::Schema,
        StorageErrorKind::IncompatibleSchema(reason),
        "Restore a backup created by this version. Metadata was not deleted.",
    )
}
