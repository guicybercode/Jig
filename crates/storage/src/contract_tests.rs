#![allow(clippy::too_many_lines)]

use std::collections::BTreeMap;
use std::fs;
use std::sync::Arc;
use std::thread;

use cli_master_core::{AgentId, AgentSource, ProjectId, SessionId, SessionStatus, WorktreeId};
use rusqlite::{Connection, ErrorCode};
use serde_json::json;
use tempfile::TempDir;

use crate::{
    LATEST_SCHEMA_VERSION, LiveSessionIndex, NewCustomAgent, NewProject, NewSession, NewWorktree,
    PathStatus, ReconciliationReason, RecoveryContext, Storage, StorageErrorKind, WorktreeState,
    connection_for_tests,
};

const TIMESTAMP: &str = "2026-08-28T18:00:00Z";

fn migrated_memory() -> Storage {
    Storage::open_in_memory_migrated().expect("in-memory database should migrate")
}

fn temp_db() -> (TempDir, Storage) {
    let directory = TempDir::new().expect("temporary directory");
    let path = directory.path().join("cli-master.db");
    let storage = Storage::open_migrated(&path).expect("database should open and migrate");
    (directory, storage)
}

fn existing_dir() -> (TempDir, std::path::PathBuf) {
    let directory = TempDir::new().expect("temporary directory");
    let path = directory.path().join("project");
    fs::create_dir(&path).expect("project directory");
    (directory, path)
}

fn add_project(storage: &Storage) -> (TempDir, crate::StoredProject) {
    let (directory, path) = existing_dir();
    let project = storage
        .add_project(&NewProject {
            id: ProjectId::new(),
            name: "Core".to_owned(),
            path,
            repository_root: None,
        })
        .expect("project should insert");
    (directory, project)
}

fn add_agent(storage: &Storage) -> crate::StoredAgent {
    storage
        .create_custom_agent(&NewCustomAgent {
            id: AgentId::new(),
            name: "Local Codex".to_owned(),
            executable: "codex".to_owned(),
            args: vec!["--interactive".to_owned()],
            env: BTreeMap::from([("PATH".to_owned(), "/usr/bin".to_owned())]),
        })
        .expect("custom agent should insert")
}

fn schema_objects(
    connection: &Connection,
    object_type: &str,
) -> std::collections::BTreeSet<String> {
    let mut statement = connection
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

#[test]
fn migrates_an_empty_database() {
    let directory = TempDir::new().expect("temporary directory");
    let path = directory.path().join("cli-master.db");
    let storage = Storage::open(&path).expect("database should open");
    assert!(storage.database_path().is_some());

    assert_eq!(storage.schema_version().expect("version"), 0);
    storage.migrate().expect("empty database should migrate");
    assert_eq!(
        storage.schema_version().expect("version"),
        LATEST_SCHEMA_VERSION
    );

    let connection = connection_for_tests(&storage);
    let foreign_keys: bool = connection
        .pragma_query_value(None, "foreign_keys", |row| row.get(0))
        .expect("foreign keys");
    let journal_mode: String = connection
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .expect("journal mode");
    let busy_timeout: i64 = connection
        .pragma_query_value(None, "busy_timeout", |row| row.get(0))
        .expect("busy timeout");
    let synchronous: u32 = connection
        .pragma_query_value(None, "synchronous", |row| row.get(0))
        .expect("synchronous");

    assert!(foreign_keys);
    assert_eq!(journal_mode, "wal");
    assert_eq!(busy_timeout, 5_000);
    assert_eq!(synchronous, 2, "SQLite FULL mode is numeric value 2");
}

#[test]
fn migration_is_idempotent_after_reopen() {
    let directory = TempDir::new().expect("temporary directory");
    let path = directory.path().join("cli-master.db");

    {
        let storage = Storage::open_migrated(&path).expect("first open");
        storage.migrate().expect("second migrate is a no-op");
    }

    let reopened = Storage::open_migrated(&path).expect("reopen");
    let connection = connection_for_tests(&reopened);
    let migration_count: u32 = connection
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .expect("migration count");
    drop(connection);
    assert_eq!(migration_count, LATEST_SCHEMA_VERSION);
    assert_eq!(
        reopened.schema_version().expect("version"),
        LATEST_SCHEMA_VERSION
    );
}

#[test]
fn upgrades_from_version_one_fixture() {
    let directory = TempDir::new().expect("temporary directory");
    let path = directory.path().join("cli-master.db");
    {
        let mut connection = Connection::open(&path).expect("open raw sqlite");
        connection
            .execute_batch(
                "CREATE TABLE schema_migrations (
                    version INTEGER PRIMARY KEY,
                    name TEXT NOT NULL,
                    applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );",
            )
            .expect("migration table");
        let tx = connection.transaction().expect("tx");
        tx.execute_batch(include_str!("../migrations/0001_initial.sql"))
            .expect("apply v1");
        tx.execute(
            "INSERT INTO schema_migrations (version, name) VALUES (1, 'initial')",
            [],
        )
        .expect("record v1");
        tx.execute(
            "INSERT INTO projects (id, name, path, created_at, last_opened_at)
             VALUES ('11111111-1111-7111-8111-111111111111', 'Legacy', '/tmp/legacy', ?1, ?1)",
            [TIMESTAMP],
        )
        .expect("legacy project");
        tx.commit().expect("commit v1");
    }

    let storage = Storage::open_migrated(&path).expect("upgrade should apply v2");
    assert_eq!(
        storage.schema_version().expect("version"),
        LATEST_SCHEMA_VERSION
    );

    let connection = connection_for_tests(&storage);
    let updated_at: String = connection
        .query_row(
            "SELECT updated_at FROM projects WHERE name = 'Legacy'",
            [],
            |row| row.get(0),
        )
        .expect("backfilled updated_at");
    assert_eq!(updated_at, TIMESTAMP);

    let columns = schema_objects(&connection, "table");
    assert!(columns.contains("sessions"));
    let has_started: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'started_at'",
            [],
            |row| row.get(0),
        )
        .expect("started_at column");
    assert_eq!(has_started, 1);
}

#[test]
fn migration_creates_expected_tables_and_indexes() {
    let storage = migrated_memory();
    let connection = connection_for_tests(&storage);
    let tables = schema_objects(&connection, "table");
    let indexes = schema_objects(&connection, "index");

    let expected_tables = [
        "agents",
        "projects",
        "schema_migrations",
        "sessions",
        "settings",
        "worktrees",
    ];
    for table in expected_tables {
        assert!(tables.contains(table), "missing table {table}");
    }
    for index in [
        "sessions_by_project_updated",
        "sessions_by_status",
        "worktrees_by_project",
        "projects_by_last_opened",
        "agents_by_source_enabled",
        "sessions_by_agent",
    ] {
        assert!(indexes.contains(index), "missing index {index}");
    }
}

#[test]
fn project_crud_survives_reopen() {
    let directory = TempDir::new().expect("temporary directory");
    let db_path = directory.path().join("cli-master.db");
    let project_dir = directory.path().join("repo");
    fs::create_dir(&project_dir).expect("repo");
    let canonical_project_dir = fs::canonicalize(&project_dir).expect("canonical repo path");
    let id = ProjectId::new();

    {
        let storage = Storage::open_migrated(&db_path).expect("open");
        storage
            .add_project(&NewProject {
                id,
                name: "Repo".to_owned(),
                path: project_dir.clone(),
                repository_root: Some(project_dir.clone()),
            })
            .expect("insert");
        storage.rename_project(id, "Renamed").expect("rename");
        storage.touch_project_opened(id).expect("touch");
    }

    let reopened = Storage::open_migrated(&db_path).expect("reopen");
    let loaded = reopened.get_project(id).expect("reload");
    assert_eq!(loaded.project.name, "Renamed");
    assert_eq!(loaded.path_status, PathStatus::Available);
    assert_eq!(loaded.project.path, canonical_project_dir);
    assert_eq!(
        loaded.project.repository_root.as_ref(),
        Some(&loaded.project.path)
    );

    reopened.remove_project(id).expect("remove metadata only");
    assert!(project_dir.exists(), "repository files must remain");
    assert!(reopened.get_project(id).is_err());
}

#[test]
fn missing_project_path_is_reported_not_deleted() {
    let directory = TempDir::new().expect("temporary directory");
    let db_path = directory.path().join("cli-master.db");
    let project_dir = directory.path().join("vanished");
    fs::create_dir(&project_dir).expect("dir");
    let id = ProjectId::new();
    let storage = Storage::open_migrated(&db_path).expect("open");
    storage
        .add_project(&NewProject {
            id,
            name: "Vanishing".to_owned(),
            path: project_dir.clone(),
            repository_root: None,
        })
        .expect("insert");
    fs::remove_dir_all(&project_dir).expect("remove dir");

    let loaded = storage.get_project(id).expect("metadata remains");
    assert_eq!(loaded.path_status, PathStatus::Missing);
    assert!(!loaded.path_is_usable());
}

#[test]
fn add_project_rejects_missing_and_relative_paths() {
    let storage = migrated_memory();
    let missing = storage
        .add_project(&NewProject {
            id: ProjectId::new(),
            name: "Missing".to_owned(),
            path: std::path::PathBuf::from("/definitely/not/here-cli-master"),
            repository_root: None,
        })
        .expect_err("missing path");
    assert!(matches!(missing.kind(), StorageErrorKind::PathMissing(_)));

    let relative = storage
        .add_project(&NewProject {
            id: ProjectId::new(),
            name: "Relative".to_owned(),
            path: std::path::PathBuf::from("relative/path"),
            repository_root: None,
        })
        .expect_err("relative path");
    assert!(matches!(relative.kind(), StorageErrorKind::InvalidInput(_)));
}

#[test]
fn custom_agent_crud_rejects_secrets_and_full_env() {
    let storage = migrated_memory();
    let id = AgentId::new();
    storage
        .create_custom_agent(&NewCustomAgent {
            id,
            name: "Helper".to_owned(),
            executable: "helper".to_owned(),
            args: vec!["--flag".to_owned()],
            env: BTreeMap::from([("PATH".to_owned(), "/usr/bin".to_owned())]),
        })
        .expect("insert");

    storage
        .update_custom_agent(
            id,
            "Helper 2",
            "helper",
            &["--other".to_owned()],
            &BTreeMap::from([("PATH".to_owned(), "/opt/bin".to_owned())]),
        )
        .expect("update");
    let loaded = storage.get_agent(id).expect("get");
    assert_eq!(loaded.name, "Helper 2");
    assert_eq!(loaded.args, ["--other"]);
    assert_eq!(loaded.source, AgentSource::Custom);

    let secret = storage
        .create_custom_agent(&NewCustomAgent {
            id: AgentId::new(),
            name: "Leaky".to_owned(),
            executable: "leaky".to_owned(),
            args: Vec::new(),
            env: BTreeMap::from([("OPENAI_API_KEY".to_owned(), "sk-test".to_owned())]),
        })
        .expect_err("secret env");
    assert!(matches!(secret.kind(), StorageErrorKind::SecretRejected));
    assert!(!secret.to_string().contains("sk-test"));

    let full_env = BTreeMap::from([
        ("HOME".to_owned(), "/home/dev".to_owned()),
        ("USER".to_owned(), "dev".to_owned()),
        ("SHELL".to_owned(), "/bin/zsh".to_owned()),
    ]);
    let dumped = storage
        .create_custom_agent(&NewCustomAgent {
            id: AgentId::new(),
            name: "Dump".to_owned(),
            executable: "dump".to_owned(),
            args: Vec::new(),
            env: full_env,
        })
        .expect_err("full env");
    assert!(matches!(
        dumped.kind(),
        StorageErrorKind::FullEnvironmentRejected
    ));

    storage
        .remove_custom_agent(id)
        .expect("remove unused agent");
    assert!(storage.get_agent(id).is_err());
}

#[test]
fn builtin_agent_cannot_be_mutated_or_deleted() {
    let storage = migrated_memory();
    let id = AgentId::new();
    storage
        .upsert_builtin_agent(id, "Codex", "codex", &[])
        .expect("seed");
    storage
        .upsert_builtin_agent(id, "Codex CLI", "codex", &["--interactive".to_owned()])
        .expect("idempotent upsert");
    let error = storage
        .update_custom_agent(id, "Nope", "codex", &[], &BTreeMap::new())
        .expect_err("built-in is immutable");
    assert!(matches!(error.kind(), StorageErrorKind::Conflict(_)));
    storage
        .set_agent_enabled(id, false)
        .expect("disable is allowed");
    let delete = storage.remove_custom_agent(id).expect_err("cannot delete");
    assert!(matches!(delete.kind(), StorageErrorKind::Conflict(_)));
}

#[test]
fn session_create_reload_status_and_exit() {
    let storage = migrated_memory();
    let (_dir, project) = add_project(&storage);
    let agent = add_agent(&storage);
    let session_id = SessionId::new();
    storage
        .create_session(&NewSession {
            id: session_id,
            project_id: project.project.id,
            agent_id: agent.id,
            name: "Implement auth".to_owned(),
            cwd: project.project.path.clone(),
            branch: Some("agent/auth".to_owned()),
            worktree_path: None,
        })
        .expect("create");

    let started = storage
        .record_session_started(session_id, Some(4242), "daemon-a")
        .expect("started");
    assert_eq!(started.session.status, SessionStatus::Running);
    assert_eq!(started.session.pid, Some(4242));
    assert!(started.session.started_at_ms.is_some());

    storage
        .update_session_status(session_id, SessionStatus::Idle)
        .expect("idle");
    storage
        .set_session_branch_and_worktree(
            session_id,
            Some("agent/auth-2"),
            Some(project.project.path.as_path()),
        )
        .expect("branch");
    let exited = storage
        .record_session_exit(session_id, SessionStatus::Exited, Some(0), None)
        .expect("exit");
    assert_eq!(exited.session.status, SessionStatus::Exited);
    assert_eq!(exited.session.exit_code, Some(0));
    assert_eq!(exited.session.pid, Some(4242), "last pid is historical");
    assert!(exited.session.exited_at_ms.is_some());
    assert_eq!(exited.session.branch.as_deref(), Some("agent/auth-2"));
}

#[test]
fn reconcile_after_simulated_daemon_restart() {
    let (_temp, storage) = temp_db();
    let (_dir, project) = add_project(&storage);
    let agent = add_agent(&storage);
    let live_id = SessionId::new();
    let gone_id = SessionId::new();
    let other_daemon_id = SessionId::new();
    let exited_id = SessionId::new();

    for (id, status, daemon) in [
        (live_id, SessionStatus::Running, "daemon-new"),
        (gone_id, SessionStatus::Idle, "daemon-new"),
        (other_daemon_id, SessionStatus::Starting, "daemon-old"),
        (exited_id, SessionStatus::Exited, "daemon-old"),
    ] {
        storage
            .create_session(&NewSession {
                id,
                project_id: project.project.id,
                agent_id: agent.id,
                name: id.to_string(),
                cwd: project.project.path.clone(),
                branch: None,
                worktree_path: None,
            })
            .expect("create");
        storage
            .record_session_started(id, Some(99), daemon)
            .expect("start");
        if status == SessionStatus::Exited {
            storage
                .record_session_exit(id, SessionStatus::Exited, Some(0), None)
                .expect("exit");
        } else if status != SessionStatus::Running {
            storage.update_session_status(id, status).expect("status");
        }
    }

    let events = storage
        .reconcile_sessions(&RecoveryContext {
            current_daemon_instance_id: "daemon-new",
            live_session_ids: &[live_id],
        })
        .expect("reconcile");

    let by_id: BTreeMap<_, _> = events
        .into_iter()
        .map(|event| (event.session_id, event))
        .collect();

    assert_eq!(by_id[&live_id].reason, ReconciliationReason::Running);
    assert_eq!(by_id[&live_id].new_status, SessionStatus::Running);
    assert_eq!(by_id[&gone_id].reason, ReconciliationReason::ProcessGone);
    assert_eq!(by_id[&gone_id].new_status, SessionStatus::Unknown);
    assert_eq!(
        by_id[&other_daemon_id].reason,
        ReconciliationReason::DaemonRestarted
    );
    assert_eq!(by_id[&other_daemon_id].new_status, SessionStatus::Unknown);
    assert_eq!(
        by_id[&exited_id].reason,
        ReconciliationReason::ExitedNormally
    );
    assert_eq!(by_id[&exited_id].new_status, SessionStatus::Exited);

    let gone = storage.get_session(gone_id).expect("history kept");
    assert_eq!(gone.session.status, SessionStatus::Unknown);
    assert_eq!(gone.session.pid, Some(99), "pid remains historical");
    assert!(
        !storage
            .list_sessions()
            .expect("list")
            .iter()
            .any(|row| row.session.status == SessionStatus::Running && row.session.id != live_id)
    );
}

#[test]
fn persisted_pid_is_not_a_liveness_signal() {
    let storage = migrated_memory();
    let (_dir, project) = add_project(&storage);
    let agent = add_agent(&storage);
    let id = SessionId::new();
    storage
        .create_session(&NewSession {
            id,
            project_id: project.project.id,
            agent_id: agent.id,
            name: "pid trap".to_owned(),
            cwd: project.project.path.clone(),
            branch: None,
            worktree_path: None,
        })
        .expect("create");
    storage
        .record_session_started(id, Some(1), "dead-daemon")
        .expect("start");

    storage
        .reconcile_sessions(&RecoveryContext {
            current_daemon_instance_id: "fresh-daemon",
            live_session_ids: &[],
        })
        .expect("reconcile");

    let loaded = storage.get_session(id).expect("loaded");
    assert_eq!(loaded.session.pid, Some(1));
    assert_eq!(loaded.session.status, SessionStatus::Unknown);
}

#[test]
fn foreign_keys_are_enforced() {
    let storage = migrated_memory();
    let agent = add_agent(&storage);
    let error = storage
        .create_session(&NewSession {
            id: SessionId::new(),
            project_id: ProjectId::new(),
            agent_id: agent.id,
            name: "orphan".to_owned(),
            cwd: std::env::temp_dir(),
            branch: None,
            worktree_path: None,
        })
        .expect_err("missing project");
    assert!(
        matches!(
            error.kind(),
            StorageErrorKind::Conflict(_)
                | StorageErrorKind::Sqlite {
                    code: Some(ErrorCode::ConstraintViolation),
                    ..
                }
        ),
        "{error}"
    );
}

#[test]
fn domain_constraints_reject_invalid_values() {
    let storage = migrated_memory();
    let connection = connection_for_tests(&storage);
    let blank = connection.execute(
        "INSERT INTO projects (id, name, path, created_at, updated_at, last_opened_at)
         VALUES ('project-blank', '  ', '/tmp/blank', ?1, ?1, ?1)",
        [TIMESTAMP],
    );
    assert_eq!(
        blank.expect_err("blank name").sqlite_error_code(),
        Some(ErrorCode::ConstraintViolation)
    );
}

#[test]
fn rollback_after_error_leaves_no_partial_session() {
    let storage = migrated_memory();
    let (_dir, project) = add_project(&storage);
    let agent = add_agent(&storage);
    let session_id = SessionId::new();
    let worktree_id = WorktreeId::new();

    let error = storage
        .create_session_with_worktree(
            &NewSession {
                id: session_id,
                project_id: project.project.id,
                agent_id: agent.id,
                name: "partial".to_owned(),
                cwd: project.project.path.clone(),
                branch: None,
                worktree_path: None,
            },
            &NewWorktree {
                id: worktree_id,
                project_id: ProjectId::new(),
                session_id: Some(session_id),
                path: project.project.path.join("worktree"),
                branch: "agent/partial".to_owned(),
                state: WorktreeState::Creating,
            },
        )
        .expect_err("bad worktree project should fail");
    assert!(
        error.to_string().contains("worktree")
            || error.to_string().contains("constraint")
            || matches!(error.kind(), StorageErrorKind::Conflict(_))
    );
    assert!(storage.get_session(session_id).is_err());
    assert!(storage.get_worktree(worktree_id).is_err());
}

#[test]
fn remove_project_is_blocked_while_sessions_exist() {
    let storage = migrated_memory();
    let (_dir, project) = add_project(&storage);
    let agent = add_agent(&storage);
    storage
        .create_session(&NewSession {
            id: SessionId::new(),
            project_id: project.project.id,
            agent_id: agent.id,
            name: "open".to_owned(),
            cwd: project.project.path.clone(),
            branch: None,
            worktree_path: None,
        })
        .expect("session");
    let error = storage
        .remove_project(project.project.id)
        .expect_err("still referenced");
    assert!(matches!(error.kind(), StorageErrorKind::Conflict(_)));
    assert!(project.project.path.exists());
}

#[test]
fn incompatible_schema_version_is_actionable() {
    let directory = TempDir::new().expect("temporary directory");
    let path = directory.path().join("cli-master.db");
    let storage = Storage::open_migrated(&path).expect("migrate");
    {
        let connection = connection_for_tests(&storage);
        connection
            .execute(
                "INSERT INTO schema_migrations (version, name) VALUES (99, 'future')",
                [],
            )
            .expect("future version");
    }
    drop(storage);

    let error = Storage::open_migrated(&path).expect_err("future schema");
    assert!(matches!(
        error.kind(),
        StorageErrorKind::UnsupportedSchema { found: 99, .. }
    ));
    assert!(error.recovery().contains("Upgrade"));
    assert!(!error.to_string().contains("INSERT"));
}

#[test]
fn corrupted_database_is_reported_without_deleting_data() {
    let directory = TempDir::new().expect("temporary directory");
    let path = directory.path().join("cli-master.db");
    fs::write(&path, b"this is not sqlite").expect("write garbage");
    let error = Storage::open_migrated(&path).expect_err("corrupt");
    assert!(
        matches!(error.kind(), StorageErrorKind::Corrupted)
            || matches!(error.kind(), StorageErrorKind::Sqlite { .. }),
        "{error:?}"
    );
    assert_eq!(fs::read(&path).expect("file kept"), b"this is not sqlite");
    assert!(!error.to_string().contains("SELECT"));
}

#[test]
fn concurrent_project_inserts_are_serialized() {
    let directory = TempDir::new().expect("temporary directory");
    let db_path = directory.path().join("cli-master.db");
    let storage = Arc::new(Storage::open_migrated(&db_path).expect("open"));
    let mut joins = Vec::new();

    for index in 0..8 {
        let storage = Arc::clone(&storage);
        let parent = directory.path().join(format!("p{index}"));
        fs::create_dir(&parent).expect("dir");
        joins.push(thread::spawn(move || {
            storage.add_project(&NewProject {
                id: ProjectId::new(),
                name: format!("P{index}"),
                path: parent,
                repository_root: None,
            })
        }));
    }

    for join in joins {
        join.join()
            .expect("thread")
            .expect("concurrent insert should succeed");
    }
    assert_eq!(storage.list_projects().expect("list").len(), 8);
}

#[test]
fn settings_reject_token_keys() {
    let storage = migrated_memory();
    storage
        .put_setting("terminal.scrollback_bytes", &json!(8_388_608))
        .expect("safe setting");
    assert_eq!(
        storage
            .get_setting("terminal.scrollback_bytes")
            .expect("get")
            .expect("present"),
        json!(8_388_608)
    );
    storage
        .remove_setting("terminal.scrollback_bytes")
        .expect("remove");
    assert!(
        storage
            .get_setting("terminal.scrollback_bytes")
            .expect("get after remove")
            .is_none()
    );
    let error = storage
        .put_setting("openai_api_token", &json!("secret"))
        .expect_err("token key");
    assert!(matches!(error.kind(), StorageErrorKind::SecretRejected));
}

#[test]
fn sql_is_not_exposed_on_user_facing_errors() {
    let storage = migrated_memory();
    let error = storage.get_project(ProjectId::new()).expect_err("missing");
    assert_eq!(error.operation(), "get");
    assert!(!error.to_string().contains("SELECT"));
    let api = error.to_api_error();
    assert_eq!(api.code, "STORAGE_NOT_FOUND");
    assert!(api.action.is_some());
}

#[test]
fn duplicate_project_path_is_rejected() {
    let storage = migrated_memory();
    let (_dir, project) = add_project(&storage);
    let error = storage
        .add_project(&NewProject {
            id: ProjectId::new(),
            name: "Copy".to_owned(),
            path: project.project.path.clone(),
            repository_root: None,
        })
        .expect_err("duplicate path");
    assert!(matches!(error.kind(), StorageErrorKind::Conflict(_)));
}

#[test]
fn live_session_index_trait_is_implemented_for_slices() {
    let id = SessionId::new();
    let live = [id];
    assert!(live.is_live(id));
    assert!(!live.is_live(SessionId::new()));
}

#[test]
fn worktree_unique_branch_per_project() {
    let storage = migrated_memory();
    let (_dir, project) = add_project(&storage);
    let first = storage
        .create_worktree(&NewWorktree {
            id: WorktreeId::new(),
            project_id: project.project.id,
            session_id: None,
            path: project.project.path.join("wt-a"),
            branch: "agent/one".to_owned(),
            state: WorktreeState::Creating,
        })
        .expect("first");
    storage
        .set_worktree_state(first.worktree.id, WorktreeState::Active)
        .expect("activate");
    storage
        .attach_worktree_session(first.worktree.id, None)
        .expect("detach");
    let error = storage
        .create_worktree(&NewWorktree {
            id: WorktreeId::new(),
            project_id: project.project.id,
            session_id: None,
            path: project.project.path.join("wt-b"),
            branch: "agent/one".to_owned(),
            state: WorktreeState::Creating,
        })
        .expect_err("duplicate branch");
    assert!(matches!(error.kind(), StorageErrorKind::Conflict(_)));
    storage
        .remove_worktree(first.worktree.id)
        .expect("remove metadata");
}
