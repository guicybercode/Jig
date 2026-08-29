use std::collections::BTreeMap;
use std::fs;
use std::time::Duration;

use cli_master_agents::{
    AgentRegistry, CustomAgentAdapter, CustomAgentDefinition, LaunchContext, LaunchEnvironment,
};
use cli_master_core::{CommandSpec, SessionId};
use cli_master_fake_agent::compiled_executable;
use cli_master_git::{GitService, WorktreeCreate};
use cli_master_session::{SessionManager, TerminalSize};
use cli_master_storage::{AgentRecord, ProjectRecord, SessionRecord, Storage, WorktreeRecord};
use tempfile::TempDir;

const TIMESTAMP: &str = "2026-08-29T01:00:00Z";

#[allow(clippy::too_many_lines, clippy::similar_names)]
#[test]
fn main_flow_registers_repo_runs_fake_agent_and_protects_dirty_worktrees() {
    let temp = TempDir::new().expect("temporary directory");
    let git = GitService::from_path_env().expect("system git");
    let repo = git
        .init_with_commit(&temp.path().join("repo"), "initial")
        .expect("fixture repository");

    let database_path = temp.path().join("cli-master.db");
    let mut storage = Storage::open(&database_path).expect("storage opens");
    storage.migrate().expect("migrations apply");
    storage
        .insert_project(&ProjectRecord {
            id: "project-1".to_owned(),
            name: "Demo".to_owned(),
            path: repo.to_string_lossy().into_owned(),
            created_at: TIMESTAMP.to_owned(),
            last_opened_at: TIMESTAMP.to_owned(),
        })
        .expect("project");

    let fake_agent = compiled_executable();
    storage
        .insert_agent(&AgentRecord {
            id: "fake".to_owned(),
            source: "custom".to_owned(),
            name: "Fake Agent".to_owned(),
            executable: fake_agent.to_string_lossy().into_owned(),
            args_json: r#"["--banner","hello-agent","--prompt"]"#.to_owned(),
            env_json: "{}".to_owned(),
            enabled: true,
            created_at: TIMESTAMP.to_owned(),
            updated_at: TIMESTAMP.to_owned(),
        })
        .expect("agent");

    let mut registry = AgentRegistry::empty();
    let definition = CustomAgentDefinition::try_from_parts(
        "fake",
        "Fake Agent",
        fake_agent.to_str().expect("utf-8 path"),
        ["--banner", "hello-agent", "--prompt"],
        BTreeMap::new(),
    )
    .expect("custom definition");
    registry
        .register(CustomAgentAdapter::new(definition))
        .expect("register fake adapter");

    let managed = temp.path().join("managed");
    let worktree = git
        .create_worktree(&WorktreeCreate {
            repository: &repo,
            managed_root: &managed,
            project_id: "project-1",
            task_label: "demo flow",
            branch: Some("agent/demo-flow".to_owned()),
            path: Some(managed.join("worktrees/project-1/demo-flow")),
        })
        .expect("worktree");

    storage
        .insert_session(&SessionRecord {
            id: "session-1".to_owned(),
            project_id: "project-1".to_owned(),
            agent_id: "fake".to_owned(),
            name: "Demo flow".to_owned(),
            cwd: worktree.path.to_string_lossy().into_owned(),
            status: "starting".to_owned(),
            runtime_pid: None,
            daemon_instance_id: Some("test-daemon".to_owned()),
            exit_code: None,
            error_code: None,
            created_at: TIMESTAMP.to_owned(),
            updated_at: TIMESTAMP.to_owned(),
            last_activity_at: None,
        })
        .expect("session row");
    storage
        .insert_worktree(&WorktreeRecord {
            id: "worktree-1".to_owned(),
            project_id: "project-1".to_owned(),
            session_id: Some("session-1".to_owned()),
            path: worktree.path.to_string_lossy().into_owned(),
            branch: worktree.branch.clone(),
            state: "active".to_owned(),
            created_at: TIMESTAMP.to_owned(),
            updated_at: TIMESTAMP.to_owned(),
        })
        .expect("worktree row");

    let adapter = registry.get("fake").expect("adapter");
    let context = LaunchContext::new(
        &worktree.path,
        LaunchEnvironment::from_search_paths([fake_agent.parent().expect("parent")]),
    );
    let command: CommandSpec = adapter.build_command(&context).expect("command");
    assert_eq!(command.cwd(), &worktree.path);
    assert!(command.args().iter().any(|arg| arg == "hello-agent"));

    let sessions = SessionManager::new();
    let session_id = SessionId::new();
    sessions
        .start(session_id, command, TerminalSize::default())
        .expect("PTY start");
    sessions
        .wait_for_output(session_id, "hello-agent", Duration::from_secs(10))
        .expect("banner");
    sessions
        .write(session_id, b"status please\n")
        .expect("input");
    sessions
        .wait_for_output(session_id, "ack:status please", Duration::from_secs(10))
        .expect("output");

    let git_status = git.status(&worktree.path).expect("git status");
    assert!(!git_status.is_dirty);
    assert_eq!(git_status.branch.as_deref(), Some("agent/demo-flow"));

    sessions.stop(session_id).expect("stop session");
    storage
        .update_session_status("session-1", "exited", None, Some(0), TIMESTAMP)
        .expect("persist stop");

    drop(storage);
    let reloaded = Storage::open(&database_path).expect("reload storage");
    let session = reloaded
        .get_session("session-1")
        .expect("session query")
        .expect("session exists");
    assert_eq!(session.status, "exited");
    let project = reloaded
        .get_project("project-1")
        .expect("project query")
        .expect("project exists");
    assert_eq!(project.name, "Demo");

    fs::write(worktree.path.join("dirty.txt"), "nope").expect("dirty file");
    let dirty_plan = git
        .prepare_remove(&repo, &worktree.path)
        .expect("prepare dirty");
    assert!(dirty_plan.is_dirty);
    git.remove_worktree(&dirty_plan, false)
        .expect_err("dirty worktree must be protected");
    assert!(worktree.path.exists());

    fs::remove_file(worktree.path.join("dirty.txt")).expect("clean worktree");
    let clean_plan = git
        .prepare_remove(&repo, &worktree.path)
        .expect("prepare clean");
    assert!(!clean_plan.is_dirty);
    git.remove_worktree(&clean_plan, false)
        .expect("clean worktree can be removed after protection was proven");
    assert!(!worktree.path.exists());
}
