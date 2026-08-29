//! Beta acceptance: local repo, two worktree sessions, grid I/O, and recovery.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use cli_master_core::wire::{
    SessionIsolation, WorktreePrepareRemoveResponse, WorktreeRemovalBlocker,
};
use cli_master_core::{AgentId, AgentSource, Project, ProjectId, SessionId, SessionStatus};
use cli_master_daemon::{Daemon, DaemonConfig};
use cli_master_e2e::{
    ChildGuard, RepositoryFixture, contains, fake_agent_command, now_ms, process_is_alive,
    system_executable, wait_for_bytes, wait_live, wait_status,
};
use cli_master_fake_agent::{ACK_PREFIX, CWD_PREFIX, INTERRUPT, READY, REDACTED};
use cli_master_git::Git;
use cli_master_session::{
    CreateSession, CreatedSession, SessionManager, SessionManagerConfig, SessionWorktreeSaga,
    TerminalSize,
};
use cli_master_storage::{Storage, StoredAgent, StoredSession};

const DAEMON_ID: &str = "beta-e2e-daemon";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn adds_a_local_repository_and_runs_two_isolated_sessions() {
    let fixture = AcceptanceFixture::new();
    let first = fixture.create_isolated("Review", "aa11bb22");
    let second = fixture.create_isolated("Hotfix", "cc33dd44");
    wait_live(&fixture.manager, first.session.id).await;
    wait_live(&fixture.manager, second.session.id).await;
    assert_eq!(
        fixture
            .manager
            .list()
            .into_iter()
            .filter(|session| session.status.is_live())
            .count(),
        2
    );

    let first_tree = first.worktree.as_ref().expect("first worktree");
    let second_tree = second.worktree.as_ref().expect("second worktree");
    assert_ne!(first_tree.path, second_tree.path);
    assert_ne!(first_tree.branch, second_tree.branch);
    assert!(first_tree.branch.starts_with("agent/"));
    assert_eq!(first.session.worktree_id, Some(first_tree.id));
    assert_eq!(first.session.worktree_path.as_ref(), Some(&first_tree.path));

    let storage = fixture.storage();
    assert_eq!(
        storage.list_projects().expect("projects should list").len(),
        1
    );
    assert_eq!(
        storage.list_sessions().expect("sessions should list").len(),
        2
    );
    assert_eq!(
        storage
            .list_worktrees()
            .expect("worktrees should list")
            .len(),
        2
    );
    drop(storage);

    drive_grid_and_reconnect(
        &fixture.manager,
        &fixture.repo.database,
        fixture.project.id,
        &first,
        &second,
    )
    .await;
    fixture.manager.shutdown().expect("manager shutdown");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fake_agent_exit_codes_are_captured_by_the_session_manager() {
    let cwd = tempfile::TempDir::new().expect("runtime cwd");
    let manager = SessionManager::new(SessionManagerConfig::default()).expect("test manager");
    let start = || {
        manager.spawn(
            &fake_agent_command(cwd.path(), &[]),
            TerminalSize::new(24, 80).expect("terminal size"),
        )
    };

    let zero = start().expect("zero session");
    let mut subscription = manager.reconnect(zero.id, 0).expect("subscribe");
    let mut output = Vec::new();
    wait_for_bytes(&mut subscription, &mut output, READY.as_bytes()).await;
    manager.write(zero.id, b"exit 0\n").expect("exit 0");
    let exited = wait_status(&manager, zero.id, SessionStatus::Exited).await;
    assert_eq!(exited.exit_code, Some(0));

    let failed = start().expect("fail session");
    let mut subscription = manager.reconnect(failed.id, 0).expect("subscribe");
    let mut output = Vec::new();
    wait_for_bytes(&mut subscription, &mut output, READY.as_bytes()).await;
    manager.write(failed.id, b"fail\n").expect("fail command");
    let failed = wait_status(&manager, failed.id, SessionStatus::Failed).await;
    assert_eq!(failed.exit_code, Some(17));
    manager.shutdown().expect("manager shutdown");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dirty_worktree_never_receives_a_removal_token() {
    let fixture = AcceptanceFixture::new();
    let created = fixture.create_isolated("Dirty tree", "d1e2f3a4");
    let session_id = created.session.id;
    let worktree = created.worktree.expect("managed worktree");
    wait_live(&fixture.manager, session_id).await;
    fixture.manager.stop(session_id).expect("stop session");
    let exited = wait_status(&fixture.manager, session_id, SessionStatus::Exited).await;
    fixture
        .saga
        .record_session_exit(session_id, exited.exit_code)
        .expect("persist exit");
    fixture
        .saga
        .delete_session(session_id)
        .expect("delete stopped session metadata");
    fs::write(worktree.path.join("scratch.txt"), "uncommitted\n")
        .expect("dirty file should be written");

    match fixture
        .saga
        .prepare_remove(worktree.id)
        .expect("prepare_remove should inspect")
    {
        WorktreePrepareRemoveResponse::Blocked {
            worktree_id,
            is_dirty,
            blockers,
        } => {
            assert_eq!(worktree_id, worktree.id);
            assert!(is_dirty);
            assert!(blockers.contains(&WorktreeRemovalBlocker::UntrackedFiles));
        }
        WorktreePrepareRemoveResponse::Ready { .. } => {
            panic!("dirty worktree must never receive a confirmation token")
        }
    }
    assert!(worktree.path.exists());
    fixture.manager.shutdown().expect("manager shutdown");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn daemon_restart_marks_stale_sessions_unknown_without_signaling_the_pid() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    let config = DaemonConfig::from_paths(temp.path().join("data"), temp.path().join("run"));
    fs::create_dir_all(config.data_directory()).expect("data dir");
    fs::create_dir_all(config.runtime_directory()).expect("runtime dir");

    let sleep = system_executable("sleep");
    let mut leftover = ChildGuard::spawn(&sleep, &["30"]);
    let leftover_pid = leftover.id();

    let storage = Storage::open_migrated(config.database_path()).expect("storage open");
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
        executable: sleep.to_string_lossy().into_owned(),
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

    drop(storage);
    drop(daemon);
    leftover.terminate();
    assert!(!process_is_alive(leftover_pid));
}

struct AcceptanceFixture {
    manager: SessionManager,
    saga: SessionWorktreeSaga<SessionManager>,
    repo: RepositoryFixture,
    project: Project,
    agent: StoredAgent,
}

impl AcceptanceFixture {
    fn new() -> Self {
        let repo = RepositoryFixture::new();
        let git = Git::discover().expect("Git should be on PATH");
        let storage = Storage::open_migrated(&repo.database).expect("database should migrate");
        let (project, agent) = register_local_repository(&storage, &git, &repo, now_ms());
        let manager = SessionManager::new(SessionManagerConfig::default()).expect("test manager");
        let saga = SessionWorktreeSaga::new(git, storage, manager.clone(), DAEMON_ID)
            .expect("session saga should construct");
        Self {
            manager,
            saga,
            repo,
            project,
            agent,
        }
    }

    fn create_isolated(&self, name: &str, short_id: &str) -> CreatedSession {
        self.saga
            .create_session(&CreateSession {
                project_id: self.project.id,
                agent_id: self.agent.id,
                name: name.to_owned(),
                isolation: SessionIsolation::NewWorktree,
                managed_root: self.repo.managed.clone(),
                short_id: Some(short_id.to_owned()),
            })
            .expect("isolated session should start through the production saga")
    }

    fn storage(&self) -> Storage {
        Storage::open(&self.repo.database).expect("database should reopen")
    }
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

    let mut env = BTreeMap::new();
    env.insert(
        "FAKE_AGENT_MARKER".to_owned(),
        "must-not-appear-in-output".to_owned(),
    );
    let agent = StoredAgent {
        id: AgentId::new(),
        source: AgentSource::Custom,
        display_name: "Fake Agent".to_owned(),
        executable: cli_master_fake_agent::compiled_executable()
            .expect("fake agent should be built")
            .to_string_lossy()
            .into_owned(),
        args: Vec::new(),
        env,
        enabled: true,
        created_at_ms: created_at,
        updated_at_ms: created_at,
    };
    storage
        .insert_agent(&agent)
        .expect("agent metadata should persist");
    (project, agent)
}

async fn drive_grid_and_reconnect(
    manager: &SessionManager,
    database: &Path,
    project_id: ProjectId,
    first: &CreatedSession,
    second: &CreatedSession,
) {
    let first_tree = first.worktree.as_ref().expect("first worktree");
    let second_tree = second.worktree.as_ref().expect("second worktree");
    let mut first_subscription = manager
        .reconnect(first.session.id, 0)
        .expect("first tile should subscribe");
    let mut second_subscription = manager
        .reconnect(second.session.id, 0)
        .expect("second tile should subscribe");
    let mut first_output = Vec::new();
    let mut second_output = Vec::new();
    wait_for_bytes(&mut first_subscription, &mut first_output, READY.as_bytes()).await;
    wait_for_bytes(
        &mut second_subscription,
        &mut second_output,
        READY.as_bytes(),
    )
    .await;
    wait_for_bytes(
        &mut first_subscription,
        &mut first_output,
        format!("{}{}", CWD_PREFIX, first_tree.path.display()).as_bytes(),
    )
    .await;
    wait_for_bytes(
        &mut second_subscription,
        &mut second_output,
        format!("{}{}", CWD_PREFIX, second_tree.path.display()).as_bytes(),
    )
    .await;

    interact_with_both_tiles(
        manager,
        first.session.id,
        second.session.id,
        &mut first_subscription,
        &mut second_subscription,
        &mut first_output,
        &mut second_output,
    )
    .await;
    stop_one_tile_and_reopen(
        manager,
        database,
        project_id,
        first.session.id,
        second.session.id,
        &second_tree.path,
        first_subscription,
        second_subscription,
        second_output,
    )
    .await;
}

#[allow(clippy::too_many_arguments)]
async fn interact_with_both_tiles(
    manager: &SessionManager,
    first_id: SessionId,
    second_id: SessionId,
    first_subscription: &mut cli_master_session::SessionSubscription,
    second_subscription: &mut cli_master_session::SessionSubscription,
    first_output: &mut Vec<u8>,
    second_output: &mut Vec<u8>,
) {
    manager
        .write(first_id, b"review-ping\n")
        .expect("first tile input");
    manager
        .write(second_id, b"hotfix-ping\n")
        .expect("second tile input");
    wait_for_bytes(
        first_subscription,
        first_output,
        format!("{ACK_PREFIX}review-ping").as_bytes(),
    )
    .await;
    wait_for_bytes(
        second_subscription,
        second_output,
        format!("{ACK_PREFIX}hotfix-ping").as_bytes(),
    )
    .await;
    assert!(!contains(first_output, b"hotfix-ping"));
    assert!(!contains(second_output, b"review-ping"));
    assert!(!contains(first_output, b"must-not-appear-in-output"));

    manager.write(first_id, b"dump-env\n").expect("env probe");
    wait_for_bytes(first_subscription, first_output, REDACTED.as_bytes()).await;
    manager
        .resize(first_id, TerminalSize::new(12, 40).expect("terminal size"))
        .expect("first tile resize");
    manager.write(first_id, b"size\n").expect("size probe");
    wait_for_bytes(first_subscription, first_output, b"cols=40 rows=12").await;
    manager
        .write(first_id, b"\x03")
        .expect("Ctrl+C on first tile");
    wait_for_bytes(first_subscription, first_output, INTERRUPT.as_bytes()).await;
    wait_live(manager, first_id).await;
}

#[allow(clippy::too_many_arguments)]
async fn stop_one_tile_and_reopen(
    manager: &SessionManager,
    database: &Path,
    project_id: ProjectId,
    first_id: SessionId,
    second_id: SessionId,
    second_cwd: &Path,
    first_subscription: cli_master_session::SessionSubscription,
    mut second_subscription: cli_master_session::SessionSubscription,
    mut second_output: Vec<u8>,
) {
    manager.stop(first_id).expect("stop first tile");
    wait_status(manager, first_id, SessionStatus::Exited).await;
    let second_after_stop = manager
        .snapshot(second_id)
        .expect("second session should remain");
    assert!(second_after_stop.status.is_live());
    manager
        .write(second_id, b"still-alive\n")
        .expect("second tile still accepts input");
    wait_for_bytes(
        &mut second_subscription,
        &mut second_output,
        format!("{ACK_PREFIX}still-alive").as_bytes(),
    )
    .await;

    drop(first_subscription);
    drop(second_subscription);
    assert!(
        manager
            .snapshot(second_id)
            .expect("surviving session")
            .status
            .is_live()
    );

    let reopened = manager
        .reconnect(second_id, 0)
        .expect("reopen subscription");
    let replay = reopened
        .snapshot
        .output
        .iter()
        .flat_map(|chunk| chunk.bytes.iter().copied())
        .collect::<Vec<_>>();
    assert!(contains(
        &replay,
        format!("{ACK_PREFIX}hotfix-ping").as_bytes()
    ));
    assert!(contains(
        &replay,
        format!("{ACK_PREFIX}still-alive").as_bytes()
    ));
    let mut reopened = reopened;
    let mut replayed = replay;
    manager
        .write(second_id, b"after-reopen\n")
        .expect("write after reopen");
    wait_for_bytes(
        &mut reopened,
        &mut replayed,
        format!("{ACK_PREFIX}after-reopen").as_bytes(),
    )
    .await;

    let storage = Storage::open(database).expect("metadata database");
    let stored = storage
        .get_session(second_id)
        .expect("metadata load")
        .expect("second session metadata");
    assert_eq!(stored.cwd, second_cwd);
    assert_eq!(stored.project_id, project_id);
}

#[test]
fn fake_agent_command_does_not_flatten_to_a_shell() {
    let command = fake_agent_command(Path::new("/tmp"), &["--hold"]);
    assert!(command.executable().ends_with("cli-master-fake-agent"));
    assert_eq!(command.args(), ["--hold"]);
}
