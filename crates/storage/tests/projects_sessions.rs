mod common;

use cli_master_core::{AgentSource, ProjectId, SessionStatus};
use cli_master_storage::{SessionRuntimeUpdate, Storage, StorageError};
use tempfile::TempDir;

use common::{CREATED_AT_MS, agent, project, session};

#[test]
fn project_and_session_survive_database_reopen() {
    let temporary_directory = TempDir::new().expect("temporary directory should be created");
    let database_path = temporary_directory.path().join("cli-master.db");
    let project_path = temporary_directory.path().join("project");
    let project = project("CLI Master", project_path);
    let agent = agent(AgentSource::BuiltIn, "Codex");
    let session = session(
        project.id,
        agent.id,
        SessionStatus::Starting,
        Some("daemon-before-reopen"),
        None,
    );

    {
        let mut storage = Storage::open(&database_path).expect("database should open");
        storage.migrate().expect("database should migrate");
        storage
            .insert_project(&project)
            .expect("project should insert");
        storage.insert_agent(&agent).expect("agent should insert");
        storage
            .insert_session(&session)
            .expect("session should insert");
        let projects = storage.list_projects().expect("projects should list");
        assert_eq!(projects.as_slice(), std::slice::from_ref(&project));
        let sessions = storage
            .list_sessions_for_project(project.id)
            .expect("project sessions should list");
        assert_eq!(sessions.as_slice(), std::slice::from_ref(&session));
    }

    let mut reopened = Storage::open(&database_path).expect("database should reopen");
    reopened.migrate().expect("migrations should be idempotent");
    assert_eq!(
        reopened
            .get_project(project.id)
            .expect("project should load"),
        Some(project)
    );
    assert_eq!(
        reopened
            .get_session(session.id)
            .expect("session should load"),
        Some(session)
    );
}

#[test]
fn project_and_session_mutations_are_persisted() {
    let mut storage = Storage::open_in_memory().expect("database should open");
    storage.migrate().expect("database should migrate");
    let project = project("CLI Master", "/tmp/cli-master-project");
    let agent = agent(AgentSource::BuiltIn, "Codex");
    let session = session(
        project.id,
        agent.id,
        SessionStatus::Starting,
        Some("daemon-before-update"),
        None,
    );
    storage
        .insert_project(&project)
        .expect("project should insert");
    storage.insert_agent(&agent).expect("agent should insert");
    storage
        .insert_session(&session)
        .expect("session should insert");

    storage
        .rename_project(project.id, "Renamed project")
        .expect("project should rename");
    storage
        .touch_project(project.id, CREATED_AT_MS + 10)
        .expect("project timestamp should update");
    storage
        .rename_session(session.id, "Renamed session", CREATED_AT_MS + 20)
        .expect("session should rename");
    assert!(matches!(
        storage.update_session_runtime(
            session.id,
            &SessionRuntimeUpdate {
                status: SessionStatus::Running,
                runtime_pid: Some(0),
                daemon_instance_id: Some("daemon-after-reopen".to_owned()),
                exit_code: None,
                error_code: None,
                last_activity_at_ms: Some(CREATED_AT_MS + 30),
                updated_at_ms: CREATED_AT_MS + 30,
            },
        ),
        Err(StorageError::InvalidInput { .. })
    ));
    storage
        .update_session_runtime(
            session.id,
            &SessionRuntimeUpdate {
                status: SessionStatus::Running,
                runtime_pid: Some(4_242),
                daemon_instance_id: Some("daemon-after-reopen".to_owned()),
                exit_code: None,
                error_code: None,
                last_activity_at_ms: Some(CREATED_AT_MS + 30),
                updated_at_ms: CREATED_AT_MS + 30,
            },
        )
        .expect("runtime should update");

    let renamed_project = storage
        .get_project(project.id)
        .expect("project should load")
        .expect("project should exist");
    assert_eq!(renamed_project.name, "Renamed project");
    assert_eq!(renamed_project.last_opened_at_ms, CREATED_AT_MS + 10);
    let running_session = storage
        .get_session(session.id)
        .expect("session should load")
        .expect("session should exist");
    assert_eq!(running_session.name, "Renamed session");
    assert_eq!(running_session.status, SessionStatus::Running);
    assert_eq!(
        storage.list_sessions().expect("sessions should list").len(),
        1
    );

    storage
        .remove_session_metadata(session.id)
        .expect("session metadata should delete");
    storage
        .remove_agent_metadata(agent.id)
        .expect("agent metadata should delete");
    storage
        .remove_project_metadata(project.id)
        .expect("project metadata should delete");
    assert!(
        storage
            .get_project(project.id)
            .expect("project lookup should work")
            .is_none()
    );
}

#[test]
fn foreign_keys_and_missing_rows_are_actionable() {
    let mut storage = Storage::open_in_memory().expect("database should open");
    storage.migrate().expect("database should migrate");
    let agent = agent(AgentSource::BuiltIn, "Codex");
    storage.insert_agent(&agent).expect("agent should insert");
    let original_project = project("Original", "/tmp/duplicate-project-path");
    storage
        .insert_project(&original_project)
        .expect("project should insert");
    let mut duplicate_path = project("Duplicate", "/tmp/duplicate-project-path");
    duplicate_path.id = ProjectId::new();
    assert!(matches!(
        storage.insert_project(&duplicate_path),
        Err(StorageError::AlreadyExists { entity: "project" })
    ));
    let missing_project_session = session(
        ProjectId::new(),
        agent.id,
        SessionStatus::Starting,
        Some("daemon"),
        None,
    );

    assert!(matches!(
        storage.insert_session(&missing_project_session),
        Err(StorageError::RelationshipViolation {
            entity: "session",
            ..
        })
    ));
    assert!(matches!(
        storage.rename_project(ProjectId::new(), "Missing"),
        Err(StorageError::NotFound {
            entity: "project",
            ..
        })
    ));
}

#[test]
fn daemon_restart_only_recovers_stale_live_sessions() {
    let mut storage = Storage::open_in_memory().expect("database should open");
    storage.migrate().expect("database should migrate");
    let project = project("Recovery", "/tmp/recovery-project");
    let agent = agent(AgentSource::BuiltIn, "Codex");
    storage
        .insert_project(&project)
        .expect("project should insert");
    storage.insert_agent(&agent).expect("agent should insert");
    let stale = session(
        project.id,
        agent.id,
        SessionStatus::Running,
        Some("old-daemon"),
        Some(100),
    );
    let current = session(
        project.id,
        agent.id,
        SessionStatus::Idle,
        Some("current-daemon"),
        Some(101),
    );
    let exited = session(
        project.id,
        agent.id,
        SessionStatus::Exited,
        Some("old-daemon"),
        None,
    );
    for stored_session in [&stale, &current, &exited] {
        storage
            .insert_session(stored_session)
            .expect("session should insert");
    }

    assert!(matches!(
        storage.recover_stale_sessions_for_daemon(&"d".repeat(129), CREATED_AT_MS + 40),
        Err(StorageError::InvalidInput { .. })
    ));
    assert!(matches!(
        storage.recover_stale_sessions_for_daemon("daemon\0invalid", CREATED_AT_MS + 40),
        Err(StorageError::InvalidInput { .. })
    ));

    assert_eq!(
        storage
            .recover_stale_sessions_for_daemon("current-daemon", CREATED_AT_MS + 50)
            .expect("stale recovery should succeed"),
        1
    );
    let recovered = storage
        .get_session(stale.id)
        .expect("session should load")
        .expect("session should exist");
    assert_eq!(recovered.status, SessionStatus::Unknown);
    assert_eq!(recovered.runtime_pid, None);
    assert_eq!(recovered.daemon_instance_id, None);
    assert_eq!(recovered.error_code.as_deref(), Some("daemon_restarted"));
    assert_eq!(
        storage
            .get_session(current.id)
            .expect("session should load")
            .expect("session should exist")
            .status,
        SessionStatus::Idle
    );
    assert_eq!(
        storage
            .get_session(exited.id)
            .expect("session should load")
            .expect("session should exist")
            .status,
        SessionStatus::Exited
    );
    assert_eq!(
        storage
            .recover_stale_sessions_for_daemon("current-daemon", CREATED_AT_MS + 60)
            .expect("repeat recovery should succeed"),
        0
    );
}

#[test]
fn persisted_paths_must_be_absolute_and_nul_free() {
    let mut storage = Storage::open_in_memory().expect("database should open");
    storage.migrate().expect("database should migrate");
    let relative = project("Relative", "relative/project");
    assert!(matches!(
        storage.insert_project(&relative),
        Err(StorageError::InvalidInput { .. })
    ));

    let valid_project = project("Valid", "/tmp/valid-path-project");
    let stored_agent = agent(AgentSource::BuiltIn, "Codex");
    storage
        .insert_project(&valid_project)
        .expect("valid project should insert");
    storage
        .insert_agent(&stored_agent)
        .expect("agent should insert");
    let mut relative_session = session(
        valid_project.id,
        stored_agent.id,
        SessionStatus::Starting,
        Some("daemon-path"),
        None,
    );
    relative_session.cwd = "relative/session".into();
    assert!(matches!(
        storage.insert_session(&relative_session),
        Err(StorageError::InvalidInput { .. })
    ));

    #[cfg(unix)]
    {
        let nul_path = project("NUL", std::path::PathBuf::from("/tmp/project\0invalid"));
        assert!(matches!(
            storage.insert_project(&nul_path),
            Err(StorageError::InvalidInput { .. })
        ));
    }
}

#[cfg(unix)]
#[test]
fn non_utf8_project_paths_round_trip_on_linux_and_macos() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::path::PathBuf;

    let mut storage = Storage::open_in_memory().expect("database should open");
    storage.migrate().expect("database should migrate");
    let native_path = PathBuf::from(OsString::from_vec(b"/tmp/project-\xFF".to_vec()));
    let project = project("Native path", native_path.clone());

    storage
        .insert_project(&project)
        .expect("native path should insert");
    let reloaded = storage
        .get_project(project.id)
        .expect("project should load")
        .expect("project should exist");
    assert_eq!(reloaded.path, native_path);
}
