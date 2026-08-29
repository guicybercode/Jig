mod support;

use std::fs;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use cli_master_session::{CreateFaults, CreateStep, FakeSpawner, SagaErrorKind};
use cli_master_storage::{Storage, WorktreeState};
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

fn git_worktree_count(repository: &std::path::Path) -> usize {
    let git = cli_master_git::Git::discover().unwrap();
    git.list_worktrees(repository).unwrap().len()
}
