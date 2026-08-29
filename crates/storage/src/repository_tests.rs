use cli_master_core::{AgentId, AgentSource, ProjectId, SessionId, SessionStatus, WorktreeId};
use rusqlite::{ErrorCode, params};
use tempfile::TempDir;

use crate::{
    LATEST_SCHEMA_VERSION, Storage, StorageError, WorktreeState, connection_for_tests, migrate,
};

const RFC3339_TIMESTAMP: &str = "2026-08-28T18:00:00.123Z";
const RFC3339_TIMESTAMP_MS: i64 = 1_787_940_000_123;

#[test]
fn v1_rfc3339_rows_upgrade_and_load_through_typed_apis() {
    let temporary_directory = TempDir::new().expect("temporary directory should be created");
    let database_path = temporary_directory.path().join("v1.db");
    let project_id = ProjectId::new();
    let agent_id = AgentId::new();
    let session_id = SessionId::new();
    let worktree_id = WorktreeId::new();
    {
        let storage = Storage::open(&database_path).expect("v1 database should open");
        install_v1_schema(&storage);
        insert_v1_graph(&storage, project_id, agent_id, session_id, worktree_id);
    }

    let storage = Storage::open(&database_path).expect("v1 database should reopen");
    storage.migrate().expect("v1 database should upgrade");

    let project = storage
        .get_project(project_id)
        .expect("project should load")
        .expect("project should exist");
    let agent = storage
        .get_agent(agent_id)
        .expect("agent should load")
        .expect("agent should exist");
    let session = storage
        .get_session(session_id)
        .expect("session should load")
        .expect("session should exist");
    let worktree = storage
        .get_worktree(worktree_id)
        .expect("worktree should load")
        .expect("worktree should exist");
    assert_eq!(project.created_at_ms, RFC3339_TIMESTAMP_MS);
    assert_eq!(project.last_opened_at_ms, RFC3339_TIMESTAMP_MS);
    assert_eq!(agent.source, AgentSource::Custom);
    assert_eq!(agent.args, ["--quiet"]);
    assert_eq!(agent.env["AGENT_MODE"], "review");
    assert_eq!(agent.created_at_ms, RFC3339_TIMESTAMP_MS);
    assert_eq!(agent.updated_at_ms, RFC3339_TIMESTAMP_MS);
    assert_eq!(session.status, SessionStatus::Running);
    assert_eq!(session.created_at_ms, RFC3339_TIMESTAMP_MS);
    assert_eq!(session.updated_at_ms, RFC3339_TIMESTAMP_MS);
    assert_eq!(session.last_activity_at_ms, Some(RFC3339_TIMESTAMP_MS));
    assert_eq!(worktree.state, WorktreeState::Active);
    assert!(!worktree.is_dirty);
    assert_eq!(worktree.created_at_ms, RFC3339_TIMESTAMP_MS);
    assert_eq!(worktree.updated_at_ms, RFC3339_TIMESTAMP_MS);
    assert_eq!(
        storage.schema_version().expect("version should load"),
        LATEST_SCHEMA_VERSION
    );
}

#[test]
fn migration_triggers_reject_cross_project_insert_and_update() {
    let storage = Storage::open_in_memory().expect("database should open");
    storage.migrate().expect("database should migrate");
    let first_project_id = ProjectId::new();
    let second_project_id = ProjectId::new();
    let agent_id = AgentId::new();
    let first_session_id = SessionId::new();
    let second_session_id = SessionId::new();
    insert_runtime_graph(
        &storage,
        first_project_id,
        second_project_id,
        agent_id,
        first_session_id,
        second_session_id,
    );

    let insert_error = insert_raw_worktree(
        &storage,
        WorktreeId::new(),
        first_project_id,
        second_session_id,
    )
    .expect_err("cross-project insert should fail");
    assert_trigger_constraint(&insert_error);

    let valid_worktree_id = WorktreeId::new();
    insert_raw_worktree(
        &storage,
        valid_worktree_id,
        first_project_id,
        first_session_id,
    )
    .expect("same-project worktree should insert");
    let connection = connection_for_tests(&storage).expect("connection lock should be available");
    let update_error = connection
        .execute(
            "UPDATE worktrees SET session_id = ?1 WHERE id = ?2",
            params![second_session_id.to_string(), valid_worktree_id.to_string()],
        )
        .expect_err("cross-project update should fail");
    assert_trigger_constraint(&update_error);
}

#[test]
fn session_decoder_rejects_pid_outside_u32() {
    let storage = Storage::open_in_memory().expect("database should open");
    storage.migrate().expect("database should migrate");
    let project_id = ProjectId::new();
    let agent_id = AgentId::new();
    let session_id = SessionId::new();
    insert_raw_project(&storage, project_id, "/tmp/pid-project");
    insert_raw_agent(&storage, agent_id);
    connection_for_tests(&storage)
        .expect("connection lock should be available")
        .execute(
            "INSERT INTO sessions (
                id, project_id, agent_id, name, cwd, status, runtime_pid,
                daemon_instance_id, created_at, updated_at
             ) VALUES (?1, ?2, ?3, 'Overflow', '/tmp/pid-session', 'running',
                 ?4, 'daemon-pid', ?5, ?5)",
            params![
                session_id.to_string(),
                project_id.to_string(),
                agent_id.to_string(),
                i64::from(u32::MAX) + 1,
                RFC3339_TIMESTAMP,
            ],
        )
        .expect("corrupt session fixture should insert");

    assert!(matches!(
        storage.get_session(session_id),
        Err(StorageError::CorruptData {
            entity: "session",
            field: "runtime_pid",
            ..
        })
    ));
}

fn install_v1_schema(storage: &Storage) {
    let connection = connection_for_tests(storage).expect("connection lock should be available");
    migrate::ensure_migration_table(&connection).expect("migration table should exist");
    connection
        .execute_batch(include_str!("../migrations/0001_initial.sql"))
        .expect("initial schema should apply");
    connection
        .execute(
            "INSERT INTO schema_migrations (version, name) VALUES (1, 'initial')",
            [],
        )
        .expect("initial migration should be recorded");
}

fn insert_v1_graph(
    storage: &Storage,
    project_id: ProjectId,
    agent_id: AgentId,
    session_id: SessionId,
    worktree_id: WorktreeId,
) {
    insert_raw_project(storage, project_id, "/tmp/v1-project");
    connection_for_tests(storage)
        .expect("connection lock should be available")
        .execute(
            "INSERT INTO agents (
                id, source, name, executable, args_json, env_json,
                enabled, created_at, updated_at
             ) VALUES (?1, 'custom', 'V1 agent', 'codex', '[\"--quiet\"]',
                 '{\"AGENT_MODE\":\"review\"}', 1, ?2, ?2)",
            params![agent_id.to_string(), RFC3339_TIMESTAMP],
        )
        .expect("v1 agent should insert");
    connection_for_tests(storage)
        .expect("connection lock should be available")
        .execute(
            "INSERT INTO sessions (
                id, project_id, agent_id, name, cwd, status, runtime_pid,
                daemon_instance_id, created_at, updated_at, last_activity_at
             ) VALUES (?1, ?2, ?3, 'V1 session', '/tmp/v1-session', 'running',
                 4242, 'daemon-v1', ?4, ?4, ?4)",
            params![
                session_id.to_string(),
                project_id.to_string(),
                agent_id.to_string(),
                RFC3339_TIMESTAMP,
            ],
        )
        .expect("v1 session should insert");
    connection_for_tests(storage)
        .expect("connection lock should be available")
        .execute(
            "INSERT INTO worktrees (
                id, project_id, session_id, path, branch, state, created_at, updated_at
             ) VALUES (?1, ?2, ?3, '/tmp/v1-worktree', 'agent/v1', 'active', ?4, ?4)",
            params![
                worktree_id.to_string(),
                project_id.to_string(),
                session_id.to_string(),
                RFC3339_TIMESTAMP,
            ],
        )
        .expect("v1 worktree should insert");
}

fn insert_runtime_graph(
    storage: &Storage,
    first_project_id: ProjectId,
    second_project_id: ProjectId,
    agent_id: AgentId,
    first_session_id: SessionId,
    second_session_id: SessionId,
) {
    insert_raw_project(storage, first_project_id, "/tmp/first-trigger-project");
    insert_raw_project(storage, second_project_id, "/tmp/second-trigger-project");
    insert_raw_agent(storage, agent_id);
    insert_raw_session(storage, first_session_id, first_project_id, agent_id);
    insert_raw_session(storage, second_session_id, second_project_id, agent_id);
}

fn insert_raw_project(storage: &Storage, id: ProjectId, path: &str) {
    connection_for_tests(storage)
        .expect("connection lock should be available")
        .execute(
            "INSERT INTO projects (id, name, path, created_at, last_opened_at)
             VALUES (?1, 'Project', ?2, ?3, ?3)",
            params![id.to_string(), path, RFC3339_TIMESTAMP],
        )
        .expect("raw project should insert");
}

fn insert_raw_agent(storage: &Storage, id: AgentId) {
    connection_for_tests(storage)
        .expect("connection lock should be available")
        .execute(
            "INSERT INTO agents (id, source, name, executable, created_at, updated_at)
             VALUES (?1, 'built_in', 'Agent', 'codex', ?2, ?2)",
            params![id.to_string(), RFC3339_TIMESTAMP],
        )
        .expect("raw agent should insert");
}

fn insert_raw_session(storage: &Storage, id: SessionId, project_id: ProjectId, agent_id: AgentId) {
    connection_for_tests(storage)
        .expect("connection lock should be available")
        .execute(
            "INSERT INTO sessions (
                id, project_id, agent_id, name, cwd, status,
                daemon_instance_id, created_at, updated_at
             ) VALUES (?1, ?2, ?3, 'Session', '/tmp/trigger-session', 'starting',
                 'daemon-trigger', ?4, ?4)",
            params![
                id.to_string(),
                project_id.to_string(),
                agent_id.to_string(),
                RFC3339_TIMESTAMP,
            ],
        )
        .expect("raw session should insert");
}

fn insert_raw_worktree(
    storage: &Storage,
    id: WorktreeId,
    project_id: ProjectId,
    session_id: SessionId,
) -> rusqlite::Result<usize> {
    connection_for_tests(storage)
        .expect("connection lock should be available")
        .execute(
            "INSERT INTO worktrees (
            id, project_id, session_id, path, branch, state, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6, ?6)",
            params![
                id.to_string(),
                project_id.to_string(),
                session_id.to_string(),
                format!("/tmp/worktree-{id}"),
                format!("agent/{id}"),
                RFC3339_TIMESTAMP,
            ],
        )
}

fn assert_trigger_constraint(error: &rusqlite::Error) {
    assert_eq!(
        error.sqlite_error_code(),
        Some(ErrorCode::ConstraintViolation)
    );
    let rusqlite::Error::SqliteFailure(code, _) = error else {
        panic!("expected SQLite failure, received {error:?}");
    };
    assert_eq!(code.extended_code, rusqlite::ffi::SQLITE_CONSTRAINT_TRIGGER);
}
