mod support;

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use cli_master_session::{
    CreateFaults, CreateStep, FakeSpawner, SagaErrorKind, SessionError, SessionEvent,
    SessionManager, SessionManagerConfig, SessionSpawner, SessionWorktreeSaga, SpawnRequest,
    SpawnedSession,
};
use cli_master_storage::{SessionRuntimeUpdate, Storage, WorktreeState};
use support::{Fixture, branch_exists, git};

#[test]
fn create_uses_worktree_plan_oid_and_safe_names() {
    let fixture = Fixture::new();
    let saga = fixture.saga(FakeSpawner::succeeding(4242));
    let created = saga
        .create_session(&fixture.request("Safe Names", Some("c0ffee12")))
        .expect("create should succeed");
    let plan = created.plan.expect("worktree plan");
    let worktree = created.worktree.expect("worktree");

    assert!(matches!(plan.initial_oid().len(), 40 | 64));
    assert_eq!(
        created.session.worktree_path.as_deref(),
        Some(plan.destination())
    );
    assert_eq!(worktree.branch, plan.branch());
    assert!(plan.branch().starts_with("agent/safe-names-c0ffee12"));
    assert_eq!(plan.destination().parent(), Some(plan.managed_root()));
    assert_eq!(worktree.state, cli_master_core::WorktreeState::Active);
    assert_eq!(
        created.session.status,
        cli_master_core::SessionStatus::Running
    );
    assert_eq!(created.session.pid, Some(4242));
    assert!(branch_exists(&fixture.repository, plan.branch()));
}

#[test]
fn injected_failure_after_each_effect() {
    for step in [
        CreateStep::Plan,
        CreateStep::PersistCreating,
        CreateStep::GitAdd,
        CreateStep::PersistActive,
        CreateStep::Spawn,
        CreateStep::PersistRunning,
    ] {
        let fixture = Fixture::new();
        let saga = fixture.saga(FakeSpawner::succeeding(7));
        let faults = CreateFaults {
            fail_after: Some(step),
            ..CreateFaults::default()
        };
        let error = saga
            .create_session_injected(&fixture.request("Inject", Some("ab12cd34")), &faults)
            .expect_err("injected failure");
        assert_eq!(error.kind(), SagaErrorKind::InjectedFailure, "{step:?}");
        let rows = Storage::open(&fixture.database)
            .unwrap()
            .list_worktrees()
            .unwrap();
        assert!(
            rows.is_empty(),
            "{step:?} should compensate metadata, found {rows:?}"
        );
        assert_eq!(
            Storage::open(&fixture.database)
                .unwrap()
                .list_sessions()
                .unwrap()
                .len(),
            0,
            "{step:?} should not leave a session row"
        );
        let git_created = matches!(
            step,
            CreateStep::GitAdd
                | CreateStep::PersistActive
                | CreateStep::Spawn
                | CreateStep::PersistRunning
        );
        if git_created {
            assert_eq!(
                git_worktree_count(&fixture.repository),
                1,
                "{step:?} should roll back the linked worktree"
            );
        }
    }
}

#[test]
fn compensation_preserves_data_when_worktree_is_dirty() {
    let fixture = Fixture::new();
    let saga = fixture.saga(FakeSpawner::succeeding(9));
    let faults = CreateFaults {
        fail_after: Some(CreateStep::GitAdd),
        after_git_add: Some(Arc::new(|plan| {
            fs::write(plan.destination().join("user-notes.txt"), "keep me\n")
                .expect("user data should be written");
        })),
        ..CreateFaults::default()
    };
    let error = saga
        .create_session_injected(
            &fixture.request("Dirty Compensate", Some("d1r7c0de")),
            &faults,
        )
        .expect_err("partial worktree");
    assert_eq!(error.kind(), SagaErrorKind::PartialWorktree);
    let path = error.path().expect("path");
    assert!(path.join("user-notes.txt").is_file());
    let rows = Storage::open(&fixture.database)
        .unwrap()
        .list_worktrees()
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].state, WorktreeState::Orphaned);
}

#[derive(Debug)]
struct RollbackFailingSpawner;

impl SessionSpawner for RollbackFailingSpawner {
    fn spawn(&self, _request: SpawnRequest<'_>) -> Result<SpawnedSession, SessionError> {
        Ok(SpawnedSession {
            pid: 99,
            pty_id: Some("uncertain-pty".to_owned()),
        })
    }

    fn rollback(&self, _session_id: cli_master_core::SessionId) -> Result<(), SessionError> {
        Err(SessionError::Signal(
            "test could not prove process-group termination".to_owned(),
        ))
    }
}

#[derive(Clone)]
struct ReadyManagerSpawner {
    manager: SessionManager,
    ready: PathBuf,
}

impl SessionSpawner for ReadyManagerSpawner {
    fn spawn(&self, request: SpawnRequest<'_>) -> Result<SpawnedSession, SessionError> {
        let session = SessionSpawner::spawn(&self.manager, request)?;
        let deadline = Instant::now() + Duration::from_secs(2);
        while !self.ready.is_file() {
            if Instant::now() >= deadline {
                return Err(SessionError::Io(
                    "descendant process did not report readiness".to_owned(),
                ));
            }
            thread::sleep(Duration::from_millis(5));
        }
        Ok(session)
    }

    fn rollback(&self, session_id: cli_master_core::SessionId) -> Result<(), SessionError> {
        SessionSpawner::rollback(&self.manager, session_id)
    }

    fn is_live(&self, session_id: cli_master_core::SessionId) -> bool {
        SessionSpawner::is_live(&self.manager, session_id)
    }
}

#[test]
fn failed_runtime_rollback_preserves_the_worktree_and_ownership_metadata() {
    let fixture = Fixture::new();
    let saga = fixture.saga(RollbackFailingSpawner);
    let faults = CreateFaults {
        fail_after: Some(CreateStep::Spawn),
        ..CreateFaults::default()
    };

    let error = saga
        .create_session_injected(
            &fixture.request("Uncertain Rollback", Some("uncertain1")),
            &faults,
        )
        .expect_err("unproven process termination must abort compensation");

    assert_eq!(error.kind(), SagaErrorKind::PartialWorktree);
    let storage = Storage::open(&fixture.database).unwrap();
    let sessions = storage.list_sessions().unwrap();
    let worktrees = storage.list_worktrees().unwrap();
    assert_eq!(sessions.len(), 1, "live ownership metadata must remain");
    assert_eq!(worktrees.len(), 1);
    assert_eq!(worktrees[0].state, WorktreeState::Orphaned);
    assert_eq!(worktrees[0].session_id, Some(sessions[0].id));
    assert!(worktrees[0].path.is_dir());
}

#[test]
fn existing_branch_gets_a_new_name() {
    let fixture = Fixture::new();
    git(
        &fixture.repository,
        ["branch", "agent/existing-branch-abcd1234"],
    );
    let saga = fixture.saga(FakeSpawner::succeeding(11));
    let created = saga
        .create_session(&fixture.request("Existing Branch", Some("abcd1234")))
        .expect("create should succeed");
    let branch = created.worktree.expect("worktree").branch;
    assert_eq!(branch, "agent/existing-branch-abcd1234-2");
    assert!(branch_exists(&fixture.repository, &branch));
}

#[test]
fn concurrent_creates_for_the_same_destination_are_rejected() {
    let fixture = Fixture::new();
    let saga = Arc::new(fixture.saga(FakeSpawner::succeeding(13)));
    let locked = Arc::new((Mutex::new(false), Condvar::new()));
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let request = fixture.request("Same Dest", Some("same0001"));
    let locked_hook = Arc::clone(&locked);
    let release_hook = Arc::clone(&release);
    let faults = CreateFaults {
        after_lock: Some(Arc::new(move || {
            let (flag, signal) = &*locked_hook;
            *flag.lock().expect("lock flag") = true;
            signal.notify_all();
            let (flag, signal) = &*release_hook;
            let mut released = flag.lock().expect("release flag");
            while !*released {
                released = signal.wait(released).expect("release wait");
            }
        })),
        ..CreateFaults::default()
    };

    let saga_a = Arc::clone(&saga);
    let request_a = request.clone();
    let handle = thread::spawn(move || saga_a.create_session_injected(&request_a, &faults));

    {
        let (flag, signal) = &*locked;
        let mut is_locked = flag.lock().expect("lock flag");
        while !*is_locked {
            is_locked = signal.wait(is_locked).expect("lock wait");
        }
    }
    let error = saga
        .create_session(&request)
        .expect_err("second create should fail");
    assert_eq!(error.kind(), SagaErrorKind::ConcurrentCreate);
    {
        let (flag, signal) = &*release;
        *flag.lock().expect("release flag") = true;
        signal.notify_all();
    }
    handle
        .join()
        .expect("first create")
        .expect("first create should finish");
}

#[test]
fn spawn_failure_compensates_a_clean_worktree() {
    let fixture = Fixture::new();
    let saga = fixture.saga(FakeSpawner::failing());
    let error = saga
        .create_session(&fixture.request("Spawn Fail", Some("5pawnf41")))
        .expect_err("spawn should fail");
    assert_eq!(error.kind(), SagaErrorKind::InjectedFailure);
    assert!(
        Storage::open(&fixture.database)
            .unwrap()
            .list_worktrees()
            .unwrap()
            .is_empty()
    );
    assert_eq!(git_worktree_count(&fixture.repository), 1);
}

#[test]
fn stale_plan_is_rejected_when_the_branch_appears_after_planning() {
    let fixture = Fixture::new();
    let saga = fixture.saga(FakeSpawner::succeeding(17));
    let repo = fixture.repository.clone();
    let faults = CreateFaults {
        after_plan: Some(Arc::new(move |plan| {
            git(&repo, ["branch", plan.branch()]);
        })),
        ..CreateFaults::default()
    };
    let error = saga
        .create_session_injected(&fixture.request("Stale Branch", Some("57a1eb00")), &faults)
        .expect_err("stale plan");
    assert_eq!(error.kind(), SagaErrorKind::InvalidInput);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn production_spawner_registers_the_saga_session_in_the_pty_manager() {
    let fixture = Fixture::new();
    fixture.replace_agent_command("/bin/cat", Vec::new());
    let manager = SessionManager::new(SessionManagerConfig::for_tests());
    let mut events = manager.subscribe_events();
    let storage = Storage::open_migrated(&fixture.database).unwrap();
    let saga = SessionWorktreeSaga::new(
        cli_master_git::Git::discover().unwrap(),
        storage,
        manager.clone(),
        support::DAEMON_ID,
    )
    .unwrap();

    let created = saga
        .create_session(&fixture.request("Managed PTY", Some("manager1")))
        .expect("manager-backed create should succeed");
    let runtime = manager
        .get(created.session.id)
        .expect("runtime should use the saga session id");

    assert_eq!(runtime.id, created.session.id);
    assert_eq!(runtime.pid, created.session.pid);
    assert_eq!(runtime.pty_id, created.session.pty_id);
    assert_eq!(runtime.worktree_id, created.session.worktree_id);
    assert_eq!(runtime.worktree_path, created.session.worktree_path);
    assert_eq!(runtime.branch, created.session.branch);
    assert!(matches!(
        events.recv().await.unwrap(),
        SessionEvent::Created(_)
    ));
    let premature_exit = saga
        .record_session_exit(created.session.id, None)
        .expect_err("durable status must not outrun the PTY runtime");
    assert_eq!(premature_exit.kind(), SagaErrorKind::SessionInUse);

    Storage::open(&fixture.database)
        .unwrap()
        .update_session_runtime(
            created.session.id,
            &SessionRuntimeUpdate {
                status: cli_master_core::SessionStatus::Exited,
                runtime_pid: None,
                daemon_instance_id: None,
                exit_code: Some(0),
                error_code: None,
                last_activity_at_ms: Some(support::CREATED_AT_MS + 1),
                updated_at_ms: support::CREATED_AT_MS + 1,
            },
        )
        .unwrap();
    let prepared = saga
        .prepare_remove(created.worktree.as_ref().unwrap().id)
        .expect("live runtime inspection should succeed");
    let cli_master_core::wire::WorktreePrepareRemoveResponse::Blocked { blockers, .. } = prepared
    else {
        panic!("runtime ownership must block removal despite exited durable state: {prepared:?}");
    };
    assert!(blockers.iter().any(|blocker| matches!(
        blocker,
        cli_master_core::wire::WorktreeRemovalBlocker::Running
            | cli_master_core::wire::WorktreeRemovalBlocker::InUse
    )));

    manager
        .kill(created.session.id)
        .await
        .expect("manager should stop the process group");
    saga.record_session_exit(created.session.id, None)
        .expect("durable runtime should record exit");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn saga_failure_after_spawn_rolls_back_the_pty_runtime() {
    let fixture = Fixture::new();
    fixture.replace_agent_command("/bin/cat", Vec::new());
    let manager = SessionManager::new(SessionManagerConfig::for_tests());
    let storage = Storage::open_migrated(&fixture.database).unwrap();
    let saga = SessionWorktreeSaga::new(
        cli_master_git::Git::discover().unwrap(),
        storage,
        manager.clone(),
        support::DAEMON_ID,
    )
    .unwrap();
    let faults = CreateFaults {
        fail_after: Some(CreateStep::Spawn),
        ..CreateFaults::default()
    };

    let error = saga
        .create_session_injected(&fixture.request("Rollback PTY", Some("manager2")), &faults)
        .expect_err("injected post-spawn failure");

    assert_eq!(error.kind(), SagaErrorKind::InjectedFailure);
    assert!(
        manager.list().is_empty(),
        "rollback must forget the PTY runtime"
    );
    assert!(
        Storage::open(&fixture.database)
            .unwrap()
            .list_sessions()
            .unwrap()
            .is_empty()
    );
    assert_eq!(git_worktree_count(&fixture.repository), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn saga_rollback_kills_the_entire_process_group_before_removing_the_worktree() {
    let fixture = Fixture::new();
    let ready = fixture.temp.path().join("descendant-ready");
    let leaked = fixture.temp.path().join("descendant-survived");
    let script = fixture.temp.path().join("process-group-fixture.sh");
    fs::write(
        &script,
        "(trap '' HUP INT TERM; printf ready > \"$1\"; sleep 1; printf leaked > \"$2\") & wait\n",
    )
    .expect("process-group fixture should be written");
    fixture.replace_agent_command(
        "/bin/sh",
        vec![
            script.to_string_lossy().into_owned(),
            ready.to_string_lossy().into_owned(),
            leaked.to_string_lossy().into_owned(),
        ],
    );
    let manager = SessionManager::new(SessionManagerConfig::for_tests());
    let spawner = ReadyManagerSpawner {
        manager: manager.clone(),
        ready,
    };
    let storage = Storage::open_migrated(&fixture.database).unwrap();
    let saga = SessionWorktreeSaga::new(
        cli_master_git::Git::discover().unwrap(),
        storage,
        spawner,
        support::DAEMON_ID,
    )
    .unwrap();
    let faults = CreateFaults {
        fail_after: Some(CreateStep::Spawn),
        ..CreateFaults::default()
    };

    let error = saga
        .create_session_injected(
            &fixture.request("Rollback Process Group", Some("manager3")),
            &faults,
        )
        .expect_err("post-spawn failure should roll back the process group");

    assert_eq!(error.kind(), SagaErrorKind::InjectedFailure);
    assert!(manager.list().is_empty());
    assert_eq!(git_worktree_count(&fixture.repository), 1);
    thread::sleep(Duration::from_millis(1_200));
    assert!(
        !leaked.exists(),
        "a descendant survived after the worktree was removed"
    );
}

fn git_worktree_count(repository: &std::path::Path) -> usize {
    let git = cli_master_git::Git::discover().unwrap();
    git.list_worktrees(repository).unwrap().len()
}
