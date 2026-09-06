mod support;

use std::fs;
use std::os::unix::fs::symlink;
use std::sync::{Arc, Mutex};

use cli_master_core::{
    SessionStatus,
    wire::{RelativeDirectory, SessionIsolation},
};
use cli_master_git::Git;
use cli_master_session::{
    CreateFaults, CreateStep, FakeSpawner, SagaErrorKind, SessionWorktreeSaga,
};
use cli_master_storage::{Storage, WorktreeState};
use support::{Fixture, git};

#[test]
fn prepared_worktree_has_no_runtime_and_survives_recovery() {
    let fixture = Fixture::new();
    let shared = Arc::new(Mutex::new(
        Storage::open_migrated(&fixture.database).unwrap(),
    ));
    let saga = SessionWorktreeSaga::new_with_shared_storage(
        Git::discover().unwrap(),
        Arc::clone(&shared),
        FakeSpawner::failing(),
        support::DAEMON_ID,
    )
    .unwrap();
    let prepared = saga
        .prepare_session(&fixture.request("Prepare", None), None)
        .unwrap();
    let worktree = prepared.worktree.unwrap();
    assert_eq!(prepared.session.status, SessionStatus::Unknown);
    assert!(prepared.session.pid.is_none());
    assert!(prepared.session.pty_id.is_none());
    assert!(worktree.path.is_dir());
    assert_eq!(worktree.session_id, Some(prepared.session.id));
    assert_eq!(worktree.state, cli_master_core::WorktreeState::Active);
    let storage = shared.lock().unwrap();
    let stored = storage.get_session(prepared.session.id).unwrap().unwrap();
    assert!(stored.daemon_instance_id.is_none());
    assert!(stored.last_activity_at_ms.is_none());
    assert_eq!(
        storage
            .recover_stale_sessions_for_daemon("next-daemon", stored.updated_at_ms + 1)
            .unwrap(),
        0
    );
    drop(storage);
    assert_eq!(saga.recover().unwrap(), Default::default());
    saga.delete_session(prepared.session.id).unwrap();
    assert!(
        worktree.path.is_dir(),
        "deleting metadata must preserve the worktree"
    );
    assert!(
        shared
            .lock()
            .unwrap()
            .get_worktree(worktree.id)
            .unwrap()
            .unwrap()
            .session_id
            .is_none()
    );
}

#[test]
fn relative_directory_is_resolved_in_the_new_checkout() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.repository.join("apps/api")).unwrap();
    fs::write(fixture.repository.join("apps/api/README.md"), "api\n").unwrap();
    git(&fixture.repository, ["add", "."]);
    git(&fixture.repository, ["commit", "-m", "api directory"]);
    let saga = fixture.saga(FakeSpawner::failing());
    let relative = RelativeDirectory::try_new("apps/api").unwrap();
    let prepared = saga
        .prepare_session(&fixture.request("API", None), Some(&relative))
        .unwrap();
    let root = prepared.worktree.unwrap().path;
    assert_eq!(
        prepared.session.cwd,
        root.join("apps/api").canonicalize().unwrap()
    );
    assert!(
        !prepared
            .session
            .cwd
            .starts_with(fixture.repository.canonicalize().unwrap())
    );
}

#[test]
fn project_subdirectory_is_preserved_for_current_and_worktree_sessions() {
    for isolation in [SessionIsolation::Current, SessionIsolation::NewWorktree] {
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.repository.join("apps/api")).unwrap();
        fs::write(fixture.repository.join("apps/api/README.md"), "api\n").unwrap();
        git(&fixture.repository, ["add", "."]);
        git(&fixture.repository, ["commit", "-m", "api directory"]);
        let storage = Storage::open(&fixture.database).unwrap();
        let mut project = storage.get_project(fixture.project_id).unwrap().unwrap();
        storage.remove_project_metadata(project.id).unwrap();
        project.path = fixture.repository.join("apps");
        project.repository_root = Some(fixture.repository.canonicalize().unwrap());
        storage.insert_project(&project).unwrap();
        let saga = fixture.saga(FakeSpawner::failing());
        let mut request = fixture.request("API", None);
        request.isolation = isolation;
        let prepared = saga
            .prepare_session(&request, Some(&RelativeDirectory::try_new("api").unwrap()))
            .unwrap();
        let root = prepared
            .worktree
            .map_or(fixture.repository, |worktree| worktree.path);
        assert_eq!(
            prepared.session.cwd,
            root.join("apps/api").canonicalize().unwrap()
        );
    }
}

#[test]
fn cancelled_removal_invalidates_the_old_token_and_restores_active_metadata() {
    let fixture = Fixture::new();
    let saga = fixture.saga(FakeSpawner::failing());
    let prepared = saga
        .prepare_session(&fixture.request("Cancel removal", None), None)
        .unwrap();
    let worktree = prepared.worktree.unwrap();
    let cli_master_core::wire::WorktreePrepareRemoveResponse::Ready {
        confirmation_token, ..
    } = saga.prepare_remove(worktree.id).unwrap()
    else {
        panic!("prepared clean worktree should be removable");
    };
    saga.cancel_pending_removal(worktree.id).unwrap();
    let stored = Storage::open(&fixture.database)
        .unwrap()
        .get_worktree(worktree.id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.state, WorktreeState::Active);
    assert_eq!(stored.session_id, Some(prepared.session.id));
    assert_eq!(
        saga.remove_worktree(worktree.id, &confirmation_token)
            .unwrap_err()
            .kind(),
        SagaErrorKind::InvalidToken
    );
    assert!(worktree.path.is_dir());
    let cli_master_core::wire::WorktreePrepareRemoveResponse::Ready {
        confirmation_token: new_token,
        ..
    } = saga.prepare_remove(worktree.id).unwrap()
    else {
        panic!("a fresh confirmation should still be available");
    };
    saga.remove_worktree(worktree.id, &new_token).unwrap();
    assert!(!worktree.path.exists());
}

#[test]
fn current_directory_can_be_prepared_without_a_git_repository() {
    let fixture = Fixture::new();
    fs::remove_dir_all(fixture.repository.join(".git")).unwrap();
    fs::create_dir(fixture.repository.join("child")).unwrap();
    let saga = fixture.saga(FakeSpawner::failing());
    let mut request = fixture.request("Current", None);
    request.isolation = SessionIsolation::Current;
    let prepared = saga
        .prepare_session(
            &request,
            Some(&RelativeDirectory::try_new("child").unwrap()),
        )
        .unwrap();
    assert_eq!(
        prepared.session.cwd,
        fixture.repository.join("child").canonicalize().unwrap()
    );
    assert!(prepared.worktree.is_none());
    assert!(!fixture.managed.exists());
}

#[test]
fn current_directory_rejects_symlink_escape_without_metadata() {
    let fixture = Fixture::new();
    symlink(fixture.temp.path(), fixture.repository.join("outside")).unwrap();
    let saga = fixture.saga(FakeSpawner::failing());
    let mut request = fixture.request("Escape", None);
    request.isolation = SessionIsolation::Current;
    let error = saga
        .prepare_session(
            &request,
            Some(&RelativeDirectory::try_new("outside").unwrap()),
        )
        .unwrap_err();
    assert_eq!(error.kind(), SagaErrorKind::InvalidInput);
    assert!(
        Storage::open(&fixture.database)
            .unwrap()
            .list_sessions()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn worktree_symlink_escape_is_compensated_without_deleting_the_target() {
    let fixture = Fixture::new();
    let outside = fixture.temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep.txt"), "keep\n").unwrap();
    symlink(&outside, fixture.repository.join("outside")).unwrap();
    git(&fixture.repository, ["add", "."]);
    git(&fixture.repository, ["commit", "-m", "linked directory"]);
    let saga = fixture.saga(FakeSpawner::failing());
    let error = saga
        .prepare_session(
            &fixture.request("Escape", None),
            Some(&RelativeDirectory::try_new("outside").unwrap()),
        )
        .unwrap_err();
    assert_eq!(error.kind(), SagaErrorKind::InvalidInput);
    assert_eq!(
        fs::read_to_string(outside.join("keep.txt")).unwrap(),
        "keep\n"
    );
    let storage = Storage::open(&fixture.database).unwrap();
    assert!(storage.list_sessions().unwrap().is_empty());
    assert!(storage.list_worktrees().unwrap().is_empty());
    assert_eq!(
        Git::discover()
            .unwrap()
            .list_worktrees(&fixture.repository)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn missing_directory_in_checkout_rolls_back_git_and_metadata() {
    let fixture = Fixture::new();
    fs::create_dir(fixture.repository.join("untracked-directory")).unwrap();
    let saga = fixture.saga(FakeSpawner::failing());
    let error = saga
        .prepare_session(
            &fixture.request("Missing", None),
            Some(&RelativeDirectory::try_new("untracked-directory").unwrap()),
        )
        .unwrap_err();
    assert_eq!(error.kind(), SagaErrorKind::InvalidInput);
    let storage = Storage::open(&fixture.database).unwrap();
    assert!(storage.list_sessions().unwrap().is_empty());
    assert!(storage.list_worktrees().unwrap().is_empty());
    assert_eq!(
        Git::discover()
            .unwrap()
            .list_worktrees(&fixture.repository)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn metadata_only_faults_compensate_each_durable_effect() {
    for step in [
        CreateStep::Plan,
        CreateStep::PersistCreating,
        CreateStep::GitAdd,
        CreateStep::PersistActive,
    ] {
        let fixture = Fixture::new();
        let saga = fixture.saga(FakeSpawner::failing());
        let error = saga
            .prepare_session_injected(
                &fixture.request("Fault", None),
                None,
                &CreateFaults {
                    fail_after: Some(step),
                    ..CreateFaults::default()
                },
            )
            .unwrap_err();
        assert_eq!(error.kind(), SagaErrorKind::InjectedFailure);
        let storage = Storage::open(&fixture.database).unwrap();
        assert!(storage.list_sessions().unwrap().is_empty(), "{step:?}");
        assert!(storage.list_worktrees().unwrap().is_empty(), "{step:?}");
        assert_eq!(
            Git::discover()
                .unwrap()
                .list_worktrees(&fixture.repository)
                .unwrap()
                .len(),
            1
        );
    }
}

#[test]
fn compensation_preserves_dirty_user_data_after_metadata_preparation() {
    let fixture = Fixture::new();
    let saga = fixture.saga(FakeSpawner::failing());
    let error = saga
        .prepare_session_injected(
            &fixture.request("Dirty", None),
            None,
            &CreateFaults {
                fail_after: Some(CreateStep::PersistActive),
                after_git_add: Some(Arc::new(|plan| {
                    fs::write(plan.destination().join("keep.txt"), "keep\n").unwrap();
                })),
                ..CreateFaults::default()
            },
        )
        .unwrap_err();
    assert_eq!(error.kind(), SagaErrorKind::PartialWorktree);
    let storage = Storage::open(&fixture.database).unwrap();
    assert!(storage.list_sessions().unwrap().is_empty());
    let worktrees = storage.list_worktrees().unwrap();
    assert_eq!(worktrees.len(), 1);
    assert_eq!(worktrees[0].state, WorktreeState::Orphaned);
    assert!(worktrees[0].path.join("keep.txt").is_file());
}
