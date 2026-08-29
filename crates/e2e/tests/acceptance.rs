//! Beta acceptance: local repo, two worktree sessions, grid I/O, recovery.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use cli_master_core::{
    AgentId, AgentSource, Project, ProjectId, SessionId, SessionStatus, WorktreeId,
};
use cli_master_daemon::{Daemon, DaemonConfig};
use cli_master_e2e::{
    RepositoryFixture, SessionFixture, contains, fake_agent_command, now_ms, process_is_alive,
    wait_for_bytes, wait_live, wait_status, which,
};
use cli_master_fake_agent::{ACK_PREFIX, CWD_PREFIX, INTERRUPT, READY, REDACTED, SIZE_PREFIX};
use cli_master_git::{Git, GitErrorKind, RemovalBlocker, WorktreeUse};
use cli_master_storage::{Storage, StoredAgent, StoredSession, StoredWorktree, WorktreeState};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn adds_a_local_repository_and_runs_two_grid_sessions() {
    let repo = RepositoryFixture::new();
    let git = Git::discover().expect("Git should be on PATH");
    let mut storage = Storage::open_in_memory().expect("database should open");
    storage.migrate().expect("database should migrate");
    let created_at = now_ms();
    let (project, agent) = register_local_repository(&storage, &git, &repo, created_at);
    let (first_tree, second_tree) = create_isolated_worktrees(&git, &repo);

    let fixture = SessionFixture::new();
    let first = fixture
        .start_fake_agent(project.id, agent.id, "Review", &first_tree.path, &[])
        .expect("first session should start");
    let second = fixture
        .start_fake_agent(project.id, agent.id, "Hotfix", &second_tree.path, &[])
        .expect("second session should start");
    wait_live(&fixture.manager, first.id).await;
    wait_live(&fixture.manager, second.id).await;
    assert_eq!(fixture.manager.live_count(), 2);

    persist_live_session(&storage, &first, &first_tree, created_at);
    persist_live_session(&storage, &second, &second_tree, created_at);
    assert_eq!(
        storage.list_sessions().expect("sessions should list").len(),
        2
    );

    drive_grid_and_reconnect(
        &fixture,
        &storage,
        &project,
        &first,
        &second,
        &first_tree,
        &second_tree,
    )
    .await;
    fixture.manager.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fake_agent_exit_codes_are_captured_by_the_session_manager() {
    let fixture = SessionFixture::new();
    let project_id = ProjectId::new();
    let agent_id = AgentId::new();

    let zero = fixture
        .start_fake_agent(project_id, agent_id, "zero", &fixture.cwd, &[])
        .expect("zero session");
    let mut sub = fixture.manager.subscribe(zero.id).expect("subscribe");
    let mut out = Vec::new();
    wait_for_bytes(&mut sub, &mut out, READY.as_bytes()).await;
    fixture.manager.write(zero.id, b"exit 0\n").expect("exit 0");
    let exited = wait_status(&fixture.manager, zero.id, SessionStatus::Exited).await;
    assert_eq!(exited.exit_code, Some(0));

    let failed = fixture
        .start_fake_agent(project_id, agent_id, "fail", &fixture.cwd, &[])
        .expect("fail session");
    let mut sub = fixture.manager.subscribe(failed.id).expect("subscribe");
    let mut out = Vec::new();
    wait_for_bytes(&mut sub, &mut out, READY.as_bytes()).await;
    fixture
        .manager
        .write(failed.id, b"fail\n")
        .expect("fail command");
    let failed = wait_status(&fixture.manager, failed.id, SessionStatus::Failed).await;
    assert_eq!(failed.exit_code, Some(17));
}

#[test]
fn dirty_worktree_cannot_be_removed() {
    let repo = RepositoryFixture::new();
    let git = Git::discover().expect("Git should be on PATH");
    let worktree = git
        .create_worktree(&repo.repository, &repo.managed, "Dirty tree", "d1e2f3a4")
        .expect("worktree should create");
    fs::write(worktree.path.join("scratch.txt"), "uncommitted\n")
        .expect("dirty file should be written");

    let preparation = git
        .prepare_remove(
            &repo.repository,
            &repo.managed,
            &worktree.path,
            WorktreeUse::default(),
        )
        .expect("prepare_remove should inspect");
    assert!(!preparation.can_remove);
    assert!(
        preparation
            .blockers
            .contains(&RemovalBlocker::UntrackedFiles)
    );

    let error = git
        .remove_worktree(
            &repo.repository,
            &repo.managed,
            &worktree.path,
            WorktreeUse::default,
        )
        .expect_err("dirty worktree must not be removed");
    assert_eq!(error.kind(), GitErrorKind::DirtyWorktree);
    assert!(worktree.path.exists());
}

#[test]
fn daemon_restart_converts_stale_live_sessions_to_unknown_without_killing_the_pid() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    let config = DaemonConfig::from_paths(temp.path().join("data"), temp.path().join("run"));
    fs::create_dir_all(config.data_directory()).expect("data dir");
    fs::create_dir_all(config.runtime_directory()).expect("runtime dir");

    let mut leftover = Command::new(which("sleep"))
        .args(["30"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("leftover process should start");
    let leftover_pid = leftover.id();

    let mut storage = Storage::open(config.database_path()).expect("storage open");
    storage.migrate().expect("migrate");
    let created_at = now_ms();
    let project = Project {
        id: ProjectId::new(),
        name: "Stale".to_owned(),
        path: temp.path().join("project"),
        repository_root: None,
        current_branch: None,
        created_at_ms: created_at,
        last_opened_at_ms: created_at,
    };
    fs::create_dir_all(&project.path).expect("project dir");
    storage.insert_project(&project).expect("project insert");
    let agent = StoredAgent {
        id: AgentId::new(),
        source: AgentSource::Custom,
        display_name: "Sleep".to_owned(),
        executable: which("sleep").to_string_lossy().into_owned(),
        args: vec!["30".to_owned()],
        env: BTreeMap::new(),
        enabled: true,
        created_at_ms: created_at,
        updated_at_ms: created_at,
    };
    storage.insert_agent(&agent).expect("agent insert");
    let session_id = SessionId::new();
    storage
        .insert_session(&StoredSession {
            id: session_id,
            project_id: project.id,
            agent_id: agent.id,
            name: "Orphaned".to_owned(),
            cwd: project.path.clone(),
            status: SessionStatus::Running,
            runtime_pid: Some(leftover_pid),
            daemon_instance_id: Some("old-daemon-instance".to_owned()),
            exit_code: None,
            error_code: None,
            created_at_ms: created_at,
            updated_at_ms: created_at,
            last_activity_at_ms: Some(created_at),
        })
        .expect("stale session insert");
    drop(storage);

    let daemon = Daemon::bind(config.clone()).expect("new daemon should bind");
    assert_ne!(daemon.instance_id().to_string(), "old-daemon-instance");

    let storage = Storage::open(config.database_path()).expect("reopen storage");
    let recovered = storage
        .get_session(session_id)
        .expect("load")
        .expect("session exists");
    assert_eq!(recovered.status, SessionStatus::Unknown);
    assert_eq!(recovered.runtime_pid, None);
    assert_eq!(recovered.daemon_instance_id, None);
    assert_eq!(recovered.error_code.as_deref(), Some("daemon_restarted"));
    assert!(
        process_is_alive(leftover_pid),
        "daemon restart must not signal a PID it did not spawn"
    );

    drop(daemon);
    let _ = leftover.kill();
    let _ = leftover.wait();
}

fn register_local_repository(
    storage: &Storage,
    git: &Git,
    repo: &RepositoryFixture,
    created_at: i64,
) -> (Project, StoredAgent) {
    let inspection = git
        .inspect_repository(&repo.repository)
        .expect("selected path should inspect");
    assert!(inspection.is_repository());
    assert_eq!(inspection.repository_root.as_ref(), Some(&inspection.path));
    assert_eq!(inspection.branch.as_deref(), Some("main"));

    let project = Project {
        id: ProjectId::new(),
        name: "Beta Acceptance".to_owned(),
        path: repo.repository.clone(),
        repository_root: inspection.repository_root.clone(),
        current_branch: inspection.branch.clone(),
        created_at_ms: created_at,
        last_opened_at_ms: created_at,
    };
    storage
        .insert_project(&project)
        .expect("project metadata should persist");
    let listed = storage.list_projects().expect("projects should list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].path, repo.repository);

    let agent = StoredAgent {
        id: AgentId::new(),
        source: AgentSource::Custom,
        display_name: "Fake Agent".to_owned(),
        executable: cli_master_fake_agent::compiled_executable()
            .to_string_lossy()
            .into_owned(),
        args: Vec::new(),
        env: BTreeMap::new(),
        enabled: true,
        created_at_ms: created_at,
        updated_at_ms: created_at,
    };
    storage
        .insert_agent(&agent)
        .expect("agent metadata should persist");
    (project, agent)
}

fn create_isolated_worktrees(
    git: &Git,
    repo: &RepositoryFixture,
) -> (cli_master_git::WorktreeInfo, cli_master_git::WorktreeInfo) {
    let first_tree = git
        .create_worktree(
            &repo.repository,
            &repo.managed,
            "Review comments",
            "aa11bb22",
        )
        .expect("first worktree should create");
    let second_tree = git
        .create_worktree(
            &repo.repository,
            &repo.managed,
            "Hotfix logging",
            "cc33dd44",
        )
        .expect("second worktree should create");
    assert_ne!(first_tree.path, second_tree.path);
    assert_ne!(first_tree.branch, second_tree.branch);
    assert!(
        first_tree
            .branch
            .as_deref()
            .is_some_and(|branch| branch.starts_with("agent/")),
        "first branch: {:?}",
        first_tree.branch
    );
    (first_tree, second_tree)
}

async fn drive_grid_and_reconnect(
    fixture: &SessionFixture,
    storage: &Storage,
    project: &Project,
    first: &cli_master_core::Session,
    second: &cli_master_core::Session,
    first_tree: &cli_master_git::WorktreeInfo,
    second_tree: &cli_master_git::WorktreeInfo,
) {
    let mut first_sub = fixture
        .manager
        .subscribe(first.id)
        .expect("first tile should subscribe");
    let mut second_sub = fixture
        .manager
        .subscribe(second.id)
        .expect("second tile should subscribe");
    let mut first_out = Vec::new();
    let mut second_out = Vec::new();
    wait_for_bytes(&mut first_sub, &mut first_out, READY.as_bytes()).await;
    wait_for_bytes(&mut second_sub, &mut second_out, READY.as_bytes()).await;
    wait_for_bytes(
        &mut first_sub,
        &mut first_out,
        format!("{}{}", CWD_PREFIX, first_tree.path.display()).as_bytes(),
    )
    .await;
    wait_for_bytes(
        &mut second_sub,
        &mut second_out,
        format!("{}{}", CWD_PREFIX, second_tree.path.display()).as_bytes(),
    )
    .await;

    interact_with_both_tiles(
        fixture,
        first.id,
        second.id,
        &mut first_sub,
        &mut second_sub,
        &mut first_out,
        &mut second_out,
    )
    .await;
    stop_one_tile_and_reopen(
        fixture,
        storage,
        project,
        first.id,
        second.id,
        &second_tree.path,
        first_sub,
        second_sub,
        second_out,
    )
    .await;
}

async fn interact_with_both_tiles(
    fixture: &SessionFixture,
    first_id: SessionId,
    second_id: SessionId,
    first_sub: &mut cli_master_session::SessionSubscription,
    second_sub: &mut cli_master_session::SessionSubscription,
    first_out: &mut Vec<u8>,
    second_out: &mut Vec<u8>,
) {
    fixture
        .manager
        .write(first_id, b"review-ping\n")
        .expect("first tile input");
    fixture
        .manager
        .write(second_id, b"hotfix-ping\n")
        .expect("second tile input");
    wait_for_bytes(
        first_sub,
        first_out,
        format!("{ACK_PREFIX}review-ping").as_bytes(),
    )
    .await;
    wait_for_bytes(
        second_sub,
        second_out,
        format!("{ACK_PREFIX}hotfix-ping").as_bytes(),
    )
    .await;
    assert!(
        !contains(first_out, b"hotfix-ping"),
        "first tile received the second tile's input"
    );
    assert!(
        !contains(second_out, b"review-ping"),
        "second tile received the first tile's input"
    );
    assert!(
        !contains(first_out, b"must-not-appear-in-output"),
        "fake agent leaked launch env"
    );

    fixture
        .manager
        .write(first_id, b"dump-env\n")
        .expect("env probe");
    wait_for_bytes(first_sub, first_out, REDACTED.as_bytes()).await;

    fixture
        .manager
        .resize(first_id, 40, 12)
        .expect("first tile resize");
    fixture
        .manager
        .write(first_id, b"size\n")
        .expect("size probe");
    wait_for_bytes(
        first_sub,
        first_out,
        format!("{SIZE_PREFIX} cols=40 rows=12").as_bytes(),
    )
    .await;

    fixture
        .manager
        .write(first_id, b"\x03")
        .expect("Ctrl+C on first tile");
    wait_for_bytes(first_sub, first_out, INTERRUPT.as_bytes()).await;
    wait_live(&fixture.manager, first_id).await;
}

#[allow(clippy::too_many_arguments)]
async fn stop_one_tile_and_reopen(
    fixture: &SessionFixture,
    storage: &Storage,
    project: &Project,
    first_id: SessionId,
    second_id: SessionId,
    second_cwd: &Path,
    first_sub: cli_master_session::SessionSubscription,
    mut second_sub: cli_master_session::SessionSubscription,
    mut second_out: Vec<u8>,
) {
    fixture
        .manager
        .stop(first_id)
        .await
        .expect("stop first tile");
    wait_status(&fixture.manager, first_id, SessionStatus::Exited).await;
    let second_after_stop = fixture
        .manager
        .get(second_id)
        .expect("second session should remain");
    assert!(
        second_after_stop.status.is_live(),
        "stopping one grid tile must not stop the other: {second_after_stop:?}"
    );
    fixture
        .manager
        .write(second_id, b"still-alive\n")
        .expect("second tile still accepts input");
    wait_for_bytes(
        &mut second_sub,
        &mut second_out,
        format!("{ACK_PREFIX}still-alive").as_bytes(),
    )
    .await;

    drop(first_sub);
    drop(second_sub);
    let still_live = fixture
        .manager
        .get(second_id)
        .expect("session survives UI close");
    assert!(still_live.status.is_live());

    let reopened = fixture
        .manager
        .subscribe(second_id)
        .expect("reopened window should subscribe");
    let replay = reopened.snapshot.concatenated();
    assert!(
        contains(&replay, format!("{ACK_PREFIX}hotfix-ping").as_bytes()),
        "reconnect snapshot missing earlier output"
    );
    assert!(
        contains(&replay, format!("{ACK_PREFIX}still-alive").as_bytes()),
        "reconnect snapshot missing output written while the UI was closed"
    );
    let mut reopened = reopened;
    let mut replayed = replay;
    fixture
        .manager
        .write(second_id, b"after-reopen\n")
        .expect("write after reopen");
    wait_for_bytes(
        &mut reopened,
        &mut replayed,
        format!("{ACK_PREFIX}after-reopen").as_bytes(),
    )
    .await;

    let stored = storage
        .get_session(second_id)
        .expect("metadata load")
        .expect("second session metadata");
    assert_eq!(stored.cwd, second_cwd);
    assert_eq!(stored.project_id, project.id);
}

fn persist_live_session(
    storage: &Storage,
    session: &cli_master_core::Session,
    worktree: &cli_master_git::WorktreeInfo,
    created_at: i64,
) {
    let live = session
        .pid
        .expect("live session should have a pid after spawn");
    storage
        .insert_session(&StoredSession {
            id: session.id,
            project_id: session.project_id,
            agent_id: session.agent_id,
            name: session.name.clone(),
            cwd: session.cwd.clone(),
            status: SessionStatus::Running,
            runtime_pid: Some(live),
            daemon_instance_id: Some("e2e-daemon".to_owned()),
            exit_code: None,
            error_code: None,
            created_at_ms: created_at,
            updated_at_ms: created_at,
            last_activity_at_ms: Some(created_at),
        })
        .expect("session metadata should persist");
    storage
        .insert_worktree(&StoredWorktree {
            id: WorktreeId::new(),
            project_id: session.project_id,
            session_id: Some(session.id),
            path: worktree.path.clone(),
            branch: worktree
                .branch
                .clone()
                .expect("created worktree should have a branch"),
            state: WorktreeState::Active,
            is_dirty: false,
            created_at_ms: created_at,
            updated_at_ms: created_at,
        })
        .expect("worktree metadata should persist");
}

#[test]
fn fake_agent_command_does_not_flatten_to_a_shell() {
    let command = fake_agent_command(Path::new("/tmp"), &["--hold"]);
    assert!(command.executable().ends_with("cli-master-fake-agent"));
    assert_eq!(command.args(), ["--hold"]);
}
