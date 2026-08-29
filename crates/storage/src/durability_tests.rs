use std::collections::BTreeMap;
use std::fs;
use std::sync::Arc;
use std::thread;

use cli_master_core::{AgentId, AgentSource, Project, ProjectId, SessionId, SessionStatus};
use rusqlite::{Connection, params};
use serde_json::json;
use tempfile::TempDir;

use crate::error::invalid_input;
use crate::migrate;
use crate::{
    LATEST_SCHEMA_VERSION, ReconciliationReason, RecoveryContext, Storage, StorageError,
    StoredAgent, StoredSession, connection_for_tests,
};

const CREATED_AT_MS: i64 = 1_787_940_000_123;

#[test]
fn configured_file_database_uses_full_wal_and_verified_schema() {
    let directory = TempDir::new().expect("temporary directory should be created");
    let path = directory.path().join("cli-master.db");
    let storage = Storage::open(&path).expect("database should open");

    assert_eq!(storage.schema_version().expect("version should load"), 0);
    storage.migrate().expect("database should migrate");
    assert_eq!(
        storage.schema_version().expect("version should load"),
        LATEST_SCHEMA_VERSION
    );
    assert_eq!(
        migrate::migration_names(),
        [
            (1, "initial"),
            (2, "worktree_dirty_state"),
            (3, "recovery_metadata"),
        ]
    );

    let connection = connection_for_tests(&storage).expect("connection lock should be available");
    let foreign_keys: bool = connection
        .pragma_query_value(None, "foreign_keys", |row| row.get(0))
        .expect("foreign key mode should load");
    let journal_mode: String = connection
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .expect("journal mode should load");
    let busy_timeout: i64 = connection
        .pragma_query_value(None, "busy_timeout", |row| row.get(0))
        .expect("busy timeout should load");
    let synchronous: u32 = connection
        .pragma_query_value(None, "synchronous", |row| row.get(0))
        .expect("synchronous mode should load");

    assert!(foreign_keys);
    assert_eq!(journal_mode, "wal");
    assert_eq!(busy_timeout, 5_000);
    assert_eq!(synchronous, 2, "SQLite FULL mode is numeric value 2");
}

#[test]
fn version_two_database_upgrades_additively_without_losing_dirty_state() {
    let directory = TempDir::new().expect("temporary directory should be created");
    let path = directory.path().join("v2.db");
    {
        let connection = Connection::open(&path).expect("raw database should open");
        migrate::ensure_migration_table(&connection).expect("migration table should exist");
        connection
            .execute_batch(include_str!("../migrations/0001_initial.sql"))
            .expect("version one should apply");
        connection
            .execute(
                "INSERT INTO schema_migrations (version, name) VALUES (1, 'initial')",
                [],
            )
            .expect("version one should be recorded");
        connection
            .execute_batch(include_str!("../migrations/0002_worktree_dirty_state.sql"))
            .expect("version two should apply");
        connection
            .execute(
                "INSERT INTO schema_migrations (version, name)
                 VALUES (2, 'worktree_dirty_state')",
                [],
            )
            .expect("version two should be recorded");
        connection
            .execute(
                "INSERT INTO projects (id, name, path, created_at, last_opened_at)
                 VALUES ('project-v2', 'Project', '/tmp/project-v2', ?1, ?1)",
                [CREATED_AT_MS],
            )
            .expect("project should insert");
        connection
            .execute(
                "INSERT INTO worktrees (
                    id, project_id, path, branch, state, is_dirty, created_at, updated_at
                 ) VALUES (
                    'worktree-v2', 'project-v2', '/tmp/worktree-v2', 'agent/v2',
                    'active', 1, ?1, ?1
                 )",
                [CREATED_AT_MS],
            )
            .expect("worktree should insert");
    }

    let storage = Storage::open_migrated(&path).expect("version two should upgrade");
    assert_eq!(
        storage.schema_version().expect("version should load"),
        LATEST_SCHEMA_VERSION
    );
    let connection = connection_for_tests(&storage).expect("connection lock should be available");
    let dirty: bool = connection
        .query_row(
            "SELECT is_dirty FROM worktrees WHERE id = 'worktree-v2'",
            [],
            |row| row.get(0),
        )
        .expect("dirty state should survive");
    let started_type: String = connection
        .query_row(
            "SELECT type FROM pragma_table_info('sessions') WHERE name = 'started_at'",
            [],
            |row| row.get(0),
        )
        .expect("recovery timestamp column should exist");
    let trigger_count: u32 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema
             WHERE type = 'trigger' AND name LIKE 'worktrees_same_project_on_%'",
            [],
            |row| row.get(0),
        )
        .expect("triggers should load");

    assert!(dirty);
    assert_eq!(started_type, "INTEGER");
    assert_eq!(trigger_count, 2);
}

#[test]
fn colliding_migration_name_is_rejected_before_schema_changes() {
    let directory = TempDir::new().expect("temporary directory should be created");
    let path = directory.path().join("collision.db");
    {
        let connection = Connection::open(&path).expect("raw database should open");
        migrate::ensure_migration_table(&connection).expect("migration table should exist");
        connection
            .execute_batch(include_str!("../migrations/0001_initial.sql"))
            .expect("version one should apply");
        connection
            .execute(
                "INSERT INTO schema_migrations (version, name)
                 VALUES (1, 'initial'), (2, 'recovery_metadata')",
                [],
            )
            .expect("collision fixture should insert");
    }

    let error = Storage::open_migrated(&path).expect_err("collision must be rejected");
    assert!(matches!(
        error,
        StorageError::IncompatibleMigration {
            version: 2,
            expected: "worktree_dirty_state",
            ..
        }
    ));
}

#[test]
fn connection_mutex_serializes_concurrent_writes() {
    let storage = Arc::new(Storage::open_in_memory_migrated().expect("database should migrate"));
    let mut threads = Vec::new();

    for index in 0..8 {
        let storage = Arc::clone(&storage);
        threads.push(thread::spawn(move || {
            storage.insert_project(&project(
                ProjectId::new(),
                &format!("/tmp/concurrent-project-{index}"),
            ))
        }));
    }

    for thread in threads {
        thread
            .join()
            .expect("writer thread should not panic")
            .expect("writer should succeed");
    }
    assert_eq!(
        storage.list_projects().expect("projects should list").len(),
        8
    );
}

#[test]
fn immediate_transaction_rolls_back_all_writes_on_error() {
    let storage = Storage::open_in_memory_migrated().expect("database should migrate");
    let project_id = ProjectId::new();
    let result: Result<(), StorageError> = storage.transaction(|transaction| {
        transaction.execute(
            "INSERT INTO projects (id, name, path, created_at, last_opened_at)
             VALUES (?1, 'Rollback', '/tmp/rollback-project', ?2, ?2)",
            params![project_id.to_string(), CREATED_AT_MS],
        )?;
        Err(invalid_input("transaction fixture", "force rollback"))
    });

    assert!(matches!(result, Err(StorageError::InvalidInput { .. })));
    assert!(
        storage
            .get_project(project_id)
            .expect("project lookup should succeed")
            .is_none()
    );
}

#[test]
fn settings_use_epoch_milliseconds_and_reject_secret_keys() {
    let storage = Storage::open_in_memory_migrated().expect("database should migrate");
    storage
        .put_setting("terminal.scrollbackBytes", &json!(8_388_608), CREATED_AT_MS)
        .expect("safe setting should persist");
    assert_eq!(
        storage
            .get_setting("terminal.scrollbackBytes")
            .expect("setting should load"),
        Some(json!(8_388_608))
    );
    let connection = connection_for_tests(&storage).expect("connection lock should be available");
    let stored_timestamp: i64 = connection
        .query_row(
            "SELECT CAST(updated_at AS INTEGER)
             FROM settings WHERE key = 'terminal.scrollbackBytes'",
            [],
            |row| row.get(0),
        )
        .expect("timestamp should load");
    drop(connection);
    assert_eq!(stored_timestamp, CREATED_AT_MS);
    assert!(matches!(
        storage.put_setting("provider.api_token", &json!("not-stored"), CREATED_AT_MS),
        Err(StorageError::InvalidInput { .. })
    ));
    storage
        .remove_setting("terminal.scrollbackBytes")
        .expect("setting should remove");
}

#[test]
fn reconciliation_clears_stale_pid_without_touching_live_runtime() {
    let storage = Storage::open_in_memory_migrated().expect("database should migrate");
    let project_id = ProjectId::new();
    let agent_id = AgentId::new();
    storage
        .insert_project(&project(project_id, "/tmp/recovery-project"))
        .expect("project should insert");
    storage
        .insert_agent(&agent(agent_id))
        .expect("agent should insert");

    let live_id = SessionId::new();
    let stale_id = SessionId::new();
    let gone_id = SessionId::new();
    let exited_id = SessionId::new();
    for session in [
        session(live_id, project_id, agent_id, "daemon-current"),
        session(stale_id, project_id, agent_id, "daemon-old"),
        session(gone_id, project_id, agent_id, "daemon-current"),
        StoredSession {
            id: exited_id,
            project_id,
            agent_id,
            name: "Exited".to_owned(),
            cwd: "/tmp/recovery-project".into(),
            status: SessionStatus::Exited,
            runtime_pid: None,
            daemon_instance_id: None,
            exit_code: Some(0),
            error_code: None,
            created_at_ms: CREATED_AT_MS,
            updated_at_ms: CREATED_AT_MS,
            last_activity_at_ms: Some(CREATED_AT_MS),
        },
    ] {
        storage
            .insert_session(&session)
            .expect("session should insert");
    }

    let events = storage
        .reconcile_sessions(&RecoveryContext {
            current_daemon_instance_id: "daemon-current",
            live_session_ids: &[live_id],
            updated_at_ms: CREATED_AT_MS + 1,
        })
        .expect("reconciliation should succeed");
    let event_for = |id| {
        events
            .iter()
            .find(|event| event.session_id == id)
            .expect("session event should exist")
    };
    assert_eq!(event_for(live_id).reason, ReconciliationReason::Running);
    assert_eq!(
        event_for(stale_id).reason,
        ReconciliationReason::DaemonRestarted
    );
    assert_eq!(event_for(gone_id).reason, ReconciliationReason::ProcessGone);
    assert_eq!(
        event_for(exited_id).reason,
        ReconciliationReason::ExitedNormally
    );

    let live = storage
        .get_session(live_id)
        .expect("live session should load")
        .expect("live session should exist");
    let stale = storage
        .get_session(stale_id)
        .expect("stale session should load")
        .expect("stale session should exist");
    let gone = storage
        .get_session(gone_id)
        .expect("gone session should load")
        .expect("gone session should exist");
    assert_eq!(live.status, SessionStatus::Running);
    assert_eq!(live.runtime_pid, Some(4242));
    for recovered in [&stale, &gone] {
        assert_eq!(recovered.status, SessionStatus::Unknown);
        assert_eq!(recovered.runtime_pid, None);
        assert_eq!(recovered.daemon_instance_id, None);
    }
    assert_eq!(stale.error_code.as_deref(), Some("daemon_restarted"));
    assert_eq!(gone.error_code.as_deref(), Some("process_gone"));
}

#[test]
fn user_facing_database_error_does_not_expose_sql() {
    let storage = Storage::open_in_memory_migrated().expect("database should migrate");
    let error = storage
        .with_connection("invalid query", |connection| {
            connection.execute("SELECT deliberately_invalid secret_payload", [])?;
            Ok(())
        })
        .expect_err("invalid SQL should fail");
    let displayed = error.to_string();
    let api = error.to_api_error();

    assert!(!displayed.contains("deliberately_invalid"));
    assert!(!displayed.contains("secret_payload"));
    assert!(!api.message.contains("secret_payload"));
}

#[test]
fn explicit_backup_is_readable_and_close_allows_reopen() {
    let directory = TempDir::new().expect("temporary directory should be created");
    let path = directory.path().join("cli-master.db");
    let storage = Storage::open_migrated(&path).expect("database should migrate");
    storage
        .insert_project(&project(ProjectId::new(), "/tmp/backup-project"))
        .expect("project should insert");
    let backup_path = {
        let connection =
            connection_for_tests(&storage).expect("connection lock should be available");
        migrate::backup_database(&connection, &path).expect("backup should succeed")
    };
    storage.close().expect("checkpoint should succeed");

    assert!(backup_path.exists());
    let backup = Storage::open_migrated(&backup_path).expect("backup should be readable");
    assert_eq!(
        backup.list_projects().expect("projects should list").len(),
        1
    );
    drop(backup);
    fs::metadata(path).expect("original database should remain");
}

fn project(id: ProjectId, path: &str) -> Project {
    Project {
        id,
        name: format!("Project {id}"),
        path: path.into(),
        repository_root: None,
        current_branch: None,
        created_at_ms: CREATED_AT_MS,
        last_opened_at_ms: CREATED_AT_MS,
    }
}

fn agent(id: AgentId) -> StoredAgent {
    StoredAgent {
        id,
        source: AgentSource::BuiltIn,
        display_name: "Codex".to_owned(),
        executable: "codex".to_owned(),
        args: Vec::new(),
        env: BTreeMap::new(),
        enabled: true,
        created_at_ms: CREATED_AT_MS,
        updated_at_ms: CREATED_AT_MS,
    }
}

fn session(
    id: SessionId,
    project_id: ProjectId,
    agent_id: AgentId,
    daemon_instance_id: &str,
) -> StoredSession {
    StoredSession {
        id,
        project_id,
        agent_id,
        name: format!("Session {id}"),
        cwd: "/tmp/recovery-project".into(),
        status: SessionStatus::Running,
        runtime_pid: Some(4242),
        daemon_instance_id: Some(daemon_instance_id.to_owned()),
        exit_code: None,
        error_code: None,
        created_at_ms: CREATED_AT_MS,
        updated_at_ms: CREATED_AT_MS,
        last_activity_at_ms: Some(CREATED_AT_MS),
    }
}
