use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, TransactionBehavior, params};

use crate::error::StorageError;

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
        name: "worktree_dirty_state",
        sql: include_str!("../migrations/0002_worktree_dirty_state.sql"),
        destructive: false,
    },
    Migration {
        version: 3,
        name: "recovery_metadata",
        sql: include_str!("../migrations/0003_recovery_metadata.sql"),
        destructive: false,
    },
];

const REQUIRED_TABLES: &[(&str, &[&str])] = &[
    (
        "projects",
        &["id", "name", "path", "created_at", "last_opened_at"],
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
            "is_dirty",
            "created_at",
            "updated_at",
        ],
    ),
    ("settings", &["key", "value_json", "updated_at"]),
    ("schema_migrations", &["version", "name", "applied_at"]),
];

const REQUIRED_INDEXES: &[&str] = &[
    "sessions_by_project_updated",
    "sessions_by_status",
    "worktrees_by_project",
    "projects_by_last_opened",
    "agents_by_source_enabled",
    "sessions_by_agent",
    "sessions_by_daemon_status",
    "custom_agents_by_updated",
];

const REQUIRED_TRIGGERS: &[&str] = &[
    "worktrees_same_project_on_insert",
    "worktrees_same_project_on_update",
];

pub(crate) fn ensure_migration_table(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version     INTEGER PRIMARY KEY CHECK (version > 0),
            name        TEXT NOT NULL CHECK (length(trim(name)) > 0),
            applied_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );",
    )?;
    Ok(())
}

pub(crate) fn has_migration_table(connection: &Connection) -> Result<bool, StorageError> {
    Ok(connection.query_row(
        "SELECT EXISTS(
            SELECT 1
            FROM sqlite_schema
            WHERE type = 'table' AND name = 'schema_migrations'
        )",
        [],
        |row| row.get(0),
    )?)
}

pub(crate) fn schema_version(connection: &Connection) -> Result<u32, StorageError> {
    if !has_migration_table(connection)? {
        return Ok(0);
    }

    let version = connection.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get::<_, i64>(0),
    )?;

    u32::try_from(version).map_err(|_| StorageError::InvalidSchemaVersion(version))
}

fn applied_migrations(connection: &Connection) -> Result<BTreeMap<u32, String>, StorageError> {
    let mut statement =
        connection.prepare("SELECT version, name FROM schema_migrations ORDER BY version")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut applied = BTreeMap::new();

    for row in rows {
        let (raw_version, name) = row?;
        let version = u32::try_from(raw_version)
            .map_err(|_| StorageError::InvalidSchemaVersion(raw_version))?;
        applied.insert(version, name);
    }

    Ok(applied)
}

fn validate_applied(applied: &BTreeMap<u32, String>) -> Result<(), StorageError> {
    for (&version, found_name) in applied {
        let Some(expected) = MIGRATIONS
            .iter()
            .find(|migration| migration.version == version)
        else {
            return Err(StorageError::UnsupportedSchemaVersion(version));
        };
        if found_name != expected.name {
            return Err(StorageError::IncompatibleMigration {
                version,
                expected: expected.name,
                found: found_name.clone(),
            });
        }
    }

    let applied_count =
        u32::try_from(applied.len()).map_err(|_| StorageError::IncompatibleSchema {
            object: "migration history length",
        })?;
    for version in 1..=applied_count {
        if !applied.contains_key(&version) {
            return Err(StorageError::IncompatibleSchema {
                object: "contiguous migration history",
            });
        }
    }

    Ok(())
}

pub(crate) fn migrate(connection: &mut Connection) -> Result<(), StorageError> {
    ensure_migration_table(connection)?;
    let applied = applied_migrations(connection)?;
    validate_applied(&applied)?;

    for migration in MIGRATIONS {
        if applied.contains_key(&migration.version) {
            continue;
        }

        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute_batch(migration.sql)?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, name) VALUES (?1, ?2)",
            params![migration.version, migration.name],
        )?;
        transaction.commit()?;
    }

    verify_schema(connection)
}

pub(crate) fn needs_destructive_backup(connection: &Connection) -> Result<bool, StorageError> {
    ensure_migration_table(connection)?;
    let applied = applied_migrations(connection)?;
    validate_applied(&applied)?;
    Ok(MIGRATIONS
        .iter()
        .any(|migration| migration.destructive && !applied.contains_key(&migration.version)))
}

pub(crate) fn backup_database(
    connection: &Connection,
    source: &Path,
) -> Result<PathBuf, StorageError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            StorageError::io("read system clock for backup", io::Error::other(error))
        })?;
    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("cli-master");
    let file_name = format!("{stem}-{}-{}.bak", elapsed.as_millis(), std::process::id());
    let destination = source
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .join(file_name);

    connection.backup(rusqlite::MAIN_DB, &destination, None)?;
    Ok(destination)
}

pub(crate) fn verify_schema(connection: &Connection) -> Result<(), StorageError> {
    for (table, columns) in REQUIRED_TABLES {
        require_schema_object(connection, "table", table)?;
        let mut statement = connection.prepare("SELECT name FROM pragma_table_info(?1)")?;
        let present = statement
            .query_map([table], |row| row.get::<_, String>(0))?
            .collect::<Result<BTreeSet<_>, _>>()?;
        for column in *columns {
            if !present.contains(*column) {
                return Err(StorageError::IncompatibleSchema { object: column });
            }
        }
    }

    for index in REQUIRED_INDEXES {
        require_schema_object(connection, "index", index)?;
    }
    for trigger in REQUIRED_TRIGGERS {
        require_schema_object(connection, "trigger", trigger)?;
    }
    Ok(())
}

fn require_schema_object(
    connection: &Connection,
    object_type: &str,
    name: &'static str,
) -> Result<(), StorageError> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM sqlite_schema WHERE type = ?1 AND name = ?2
        )",
        params![object_type, name],
        |row| row.get(0),
    )?;
    if exists {
        Ok(())
    } else {
        Err(StorageError::IncompatibleSchema { object: name })
    }
}

#[cfg(test)]
pub(crate) fn migration_names() -> Vec<(u32, &'static str)> {
    MIGRATIONS
        .iter()
        .map(|migration| (migration.version, migration.name))
        .collect()
}
