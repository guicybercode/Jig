mod common;

use std::collections::BTreeMap;

use cli_master_core::{AgentId, AgentSource, ProjectId, SessionId, SessionStatus};
use cli_master_storage::{Storage, StorageError, StoredWorktree, WorktreeState};

use common::{CREATED_AT_MS, agent, project, session, worktree};

#[test]
fn stored_agent_debug_redacts_argument_and_environment_values() {
    let mut stored_agent = agent(AgentSource::Custom, "Internal agent");
    stored_agent.args = vec![
        "--token=argument-secret-7f3a".to_owned(),
        "prompt-secret-5b91".to_owned(),
    ];
    stored_agent.env = BTreeMap::from([
        ("API_TOKEN".to_owned(), "environment-secret-2c84".to_owned()),
        ("MODE".to_owned(), "environment-secret-f166".to_owned()),
    ]);

    let debug = format!("{stored_agent:?}");

    assert!(debug.contains("StoredAgent"));
    assert!(debug.contains("args_count: 2"));
    assert!(debug.contains("API_TOKEN"));
    assert!(debug.contains("MODE"));
    assert!(!debug.contains("argument-secret-7f3a"));
    assert!(!debug.contains("prompt-secret-5b91"));
    assert!(!debug.contains("environment-secret-2c84"));
    assert!(!debug.contains("environment-secret-f166"));
}

#[test]
fn built_in_and_custom_agent_crud_preserves_structured_commands() {
    let mut storage = Storage::open_in_memory().expect("database should open");
    storage.migrate().expect("database should migrate");
    let built_in = agent(AgentSource::BuiltIn, "Codex");
    let mut custom = agent(AgentSource::Custom, "Internal agent");
    custom.executable = "/opt/internal agent/bin/agent".to_owned();
    custom.args = vec!["--profile".to_owned(), "review mode".to_owned()];
    custom.env = BTreeMap::from([
        ("AGENT_MODE".to_owned(), "strict".to_owned()),
        ("MODEL_PROFILE".to_owned(), "review".to_owned()),
    ]);
    storage
        .insert_agent(&built_in)
        .expect("built-in agent should insert");
    storage
        .insert_agent(&custom)
        .expect("custom agent should insert");

    let loaded = storage
        .get_agent(custom.id)
        .expect("custom agent should load")
        .expect("custom agent should exist");
    assert_eq!(loaded, custom);
    let command = loaded
        .command_for_cwd("/tmp/project with spaces")
        .expect("stored command should validate");
    assert_eq!(command.executable(), "/opt/internal agent/bin/agent");
    assert_eq!(command.args(), ["--profile", "review mode"]);
    assert_eq!(command.env()["AGENT_MODE"], "strict");
    let definition = loaded
        .definition_for_cwd("/tmp/project with spaces")
        .expect("core definition should build");
    assert_eq!(definition.id, custom.id);
    assert_eq!(definition.source, AgentSource::Custom);
    assert!(matches!(
        loaded.command_for_cwd(""),
        Err(StorageError::InvalidInput { .. })
    ));
    assert_eq!(storage.list_agents().expect("agents should list").len(), 2);

    let mut wrong_source = custom.clone();
    wrong_source.source = AgentSource::BuiltIn;
    assert!(matches!(
        storage.update_custom_agent(&wrong_source),
        Err(StorageError::InvalidInput { .. })
    ));

    custom.display_name = "Updated internal agent".to_owned();
    custom.enabled = false;
    custom.updated_at_ms += 1;
    storage
        .update_custom_agent(&custom)
        .expect("custom agent should update");
    assert_eq!(
        storage
            .get_agent(custom.id)
            .expect("custom agent should load")
            .expect("custom agent should exist"),
        custom
    );
    custom.enabled = true;
    custom.updated_at_ms += 1;
    storage
        .set_agent_enabled(custom.id, true, custom.updated_at_ms)
        .expect("custom enabled state should update");
    assert_eq!(
        storage
            .get_agent(custom.id)
            .expect("custom agent should load")
            .expect("custom agent should exist"),
        custom
    );
    assert!(matches!(
        storage.insert_agent(&custom),
        Err(StorageError::AlreadyExists { entity: "agent" })
    ));
    storage
        .remove_agent_metadata(custom.id)
        .expect("custom agent should delete");
    assert!(
        storage
            .get_agent(custom.id)
            .expect("agent lookup should work")
            .is_none()
    );
}

#[test]
fn secret_environment_keys_are_rejected_without_persisting() {
    let mut storage = Storage::open_in_memory().expect("database should open");
    storage.migrate().expect("database should migrate");
    for key in [
        "API_TOKEN",
        "CLIENT_SECRET",
        "DB_PASSWORD",
        "PRIVATE_KEY",
        "OPENAI_API_KEY",
        "SERVICE_CREDENTIAL",
    ] {
        let mut invalid = agent(AgentSource::Custom, "Invalid agent");
        invalid
            .env
            .insert(key.to_owned(), "secret-value-91f2".to_owned());
        let error = storage
            .insert_agent(&invalid)
            .expect_err("secret environment key should fail");
        assert!(matches!(&error, StorageError::InvalidInput { .. }));
        let error_text = format!("{error:?} {error}");
        assert!(!error_text.contains(key));
        assert!(!error_text.contains("secret-value-91f2"));
    }
    assert!(
        storage
            .list_agents()
            .expect("agents should list")
            .is_empty()
    );
}

#[test]
fn blank_agent_executable_is_rejected() {
    let mut storage = Storage::open_in_memory().expect("database should open");
    storage.migrate().expect("database should migrate");
    let mut invalid = agent(AgentSource::Custom, "Invalid agent");
    invalid.executable = "   ".to_owned();

    assert!(matches!(
        storage.insert_agent(&invalid),
        Err(StorageError::InvalidInput { .. })
    ));
    assert!(
        storage
            .list_agents()
            .expect("agents should list")
            .is_empty()
    );
}

#[test]
fn built_in_command_is_immutable_but_enabled_state_can_change() {
    let mut storage = Storage::open_in_memory().expect("database should open");
    storage.migrate().expect("database should migrate");
    let built_in = agent(AgentSource::BuiltIn, "Codex");
    storage
        .insert_agent(&built_in)
        .expect("built-in agent should insert");
    let mut attempted_replacement = built_in.clone();
    attempted_replacement.source = AgentSource::Custom;
    attempted_replacement.executable = "replacement".to_owned();

    assert!(matches!(
        storage.update_custom_agent(&attempted_replacement),
        Err(StorageError::InvalidInput { .. })
    ));
    storage
        .set_agent_enabled(built_in.id, false, CREATED_AT_MS + 1)
        .expect("built-in enabled state should update");
    let reloaded = storage
        .get_agent(built_in.id)
        .expect("built-in agent should load")
        .expect("built-in agent should exist");
    assert_eq!(reloaded.executable, built_in.executable);
    assert_eq!(reloaded.source, AgentSource::BuiltIn);
    assert!(!reloaded.enabled);
}

#[test]
fn worktree_state_dirty_flag_and_session_association_are_consistent() {
    let scenario = seeded_worktree_scenario();
    let worktrees = scenario
        .storage
        .list_worktrees()
        .expect("worktrees should list");
    assert_eq!(
        worktrees.as_slice(),
        std::slice::from_ref(&scenario.worktree)
    );
    let project_worktrees = scenario
        .storage
        .list_worktrees_for_project(scenario.first_project_id)
        .expect("project worktrees should list");
    assert_eq!(
        project_worktrees.as_slice(),
        std::slice::from_ref(&scenario.worktree)
    );
    let mut duplicate_path = worktree(
        scenario.first_project_id,
        None,
        &scenario.worktree.path,
        "agent/different-5678",
    );
    duplicate_path.id = cli_master_core::WorktreeId::new();
    assert!(matches!(
        scenario.storage.insert_worktree(&duplicate_path),
        Err(StorageError::AlreadyExists { entity: "worktree" })
    ));
    let relative_path = worktree(
        scenario.first_project_id,
        None,
        std::path::Path::new("relative/worktree"),
        "agent/relative-9012",
    );
    assert!(matches!(
        scenario.storage.insert_worktree(&relative_path),
        Err(StorageError::InvalidInput { .. })
    ));
    scenario
        .storage
        .update_worktree_state(
            scenario.worktree.id,
            WorktreeState::Active,
            true,
            Some(scenario.first_session_id),
            CREATED_AT_MS + 1,
        )
        .expect("worktree should update");
    let active = scenario
        .storage
        .get_worktree(scenario.worktree.id)
        .expect("worktree should load")
        .expect("worktree should exist");
    assert_eq!(active.state, WorktreeState::Active);
    assert!(active.is_dirty);
    assert!(matches!(
        scenario.storage.update_worktree_state(
            scenario.worktree.id,
            WorktreeState::Active,
            true,
            Some(scenario.second_session_id),
            CREATED_AT_MS + 2,
        ),
        Err(StorageError::RelationshipViolation {
            entity: "worktree",
            ..
        })
    ));
}

#[test]
fn foreign_keys_protect_worktree_parents_and_session_delete_disassociates() {
    let scenario = seeded_worktree_scenario();
    let project_removal_error = scenario
        .storage
        .remove_project_metadata(scenario.first_project_id)
        .expect_err("referenced project should be protected");
    assert!(
        matches!(
            project_removal_error,
            StorageError::RelationshipViolation {
                entity: "project",
                ..
            }
        ),
        "unexpected error: {project_removal_error:?}"
    );
    let agent_removal_error = scenario
        .storage
        .remove_agent_metadata(scenario.agent_id)
        .expect_err("referenced agent should be protected");
    assert!(
        matches!(
            agent_removal_error,
            StorageError::RelationshipViolation {
                entity: "agent",
                ..
            }
        ),
        "unexpected error: {agent_removal_error:?}"
    );
    scenario
        .storage
        .remove_session_metadata(scenario.first_session_id)
        .expect("session metadata should delete");
    assert_eq!(
        scenario
            .storage
            .get_worktree(scenario.worktree.id)
            .expect("worktree should load")
            .expect("worktree should exist")
            .session_id,
        None
    );
    scenario
        .storage
        .remove_worktree_metadata(scenario.worktree.id)
        .expect("worktree metadata should delete");
    assert!(
        scenario
            .storage
            .get_worktree(scenario.worktree.id)
            .expect("worktree lookup should work")
            .is_none()
    );
}

struct WorktreeScenario {
    storage: Storage,
    first_project_id: ProjectId,
    agent_id: AgentId,
    first_session_id: SessionId,
    second_session_id: SessionId,
    worktree: StoredWorktree,
}

fn seeded_worktree_scenario() -> WorktreeScenario {
    let mut storage = Storage::open_in_memory().expect("database should open");
    storage.migrate().expect("database should migrate");
    let first_project = project("First", "/tmp/cli-master-first");
    let second_project = project("Second", "/tmp/cli-master-second");
    let agent = agent(AgentSource::BuiltIn, "Codex");
    storage
        .insert_project(&first_project)
        .expect("first project should insert");
    storage
        .insert_project(&second_project)
        .expect("second project should insert");
    storage.insert_agent(&agent).expect("agent should insert");
    let first_session = session(
        first_project.id,
        agent.id,
        SessionStatus::Starting,
        Some("daemon"),
        None,
    );
    let second_session = session(
        second_project.id,
        agent.id,
        SessionStatus::Starting,
        Some("daemon"),
        None,
    );
    storage
        .insert_session(&first_session)
        .expect("first session should insert");
    storage
        .insert_session(&second_session)
        .expect("second session should insert");
    let worktree = worktree(
        first_project.id,
        Some(first_session.id),
        std::path::Path::new("/tmp/cli-master-worktree"),
        "agent/first-1234",
    );
    storage
        .insert_worktree(&worktree)
        .expect("worktree should insert");
    WorktreeScenario {
        storage,
        first_project_id: first_project.id,
        agent_id: agent.id,
        first_session_id: first_session.id,
        second_session_id: second_session.id,
        worktree,
    }
}
