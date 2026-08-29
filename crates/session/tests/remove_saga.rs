mod support;

use std::fs;

use cli_master_core::SessionStatus;
use cli_master_core::wire::{ConfirmationToken, WorktreePrepareRemoveResponse};
use cli_master_session::{FakeSpawner, SagaErrorKind};
use cli_master_storage::Storage;
use support::Fixture;

#[test]
fn two_step_remove_requires_a_confirmation_token() {
    let fixture = Fixture::new();
    let saga = fixture.saga(FakeSpawner::succeeding(21));
    let created = saga
        .create_session(&fixture.request("Remove Me", Some("r3m0ve01")))
        .unwrap();
    saga.record_session_exit(created.session.id, Some(0))
        .unwrap();
    let worktree = created.worktree.unwrap();
    let path = worktree.path.clone();

    let prepared = saga.prepare_remove(worktree.id).unwrap();
    let WorktreePrepareRemoveResponse::Ready {
        confirmation_token, ..
    } = prepared
    else {
        panic!("clean unused worktree should be removable: {prepared:?}");
    };

    let forged = ConfirmationToken::try_new("forgedtokenvalue1").unwrap();
    let error = saga
        .remove_worktree(worktree.id, &forged)
        .expect_err("forged token");
    assert_eq!(error.kind(), SagaErrorKind::InvalidToken);
    assert!(path.is_dir());

    saga.remove_worktree(worktree.id, &confirmation_token)
        .expect("confirmed remove");
    assert!(!path.exists());
    assert!(
        Storage::open(&fixture.database)
            .unwrap()
            .get_worktree(worktree.id)
            .unwrap()
            .is_none()
    );
    assert!(
        Storage::open(&fixture.database)
            .unwrap()
            .get_session(created.session.id)
            .unwrap()
            .is_some(),
        "worktree removal must not delete session metadata"
    );
}

#[test]
fn dirty_worktree_cannot_be_removed_and_has_no_bypass() {
    let fixture = Fixture::new();
    let saga = fixture.saga(FakeSpawner::succeeding(22));
    let created = saga
        .create_session(&fixture.request("Dirty Tree", Some("d1r7eeee")))
        .unwrap();
    saga.record_session_exit(created.session.id, Some(0))
        .unwrap();
    let worktree = created.worktree.unwrap();
    fs::write(worktree.path.join("scratch.txt"), "dirty\n").unwrap();

    let prepared = saga.prepare_remove(worktree.id).unwrap();
    let WorktreePrepareRemoveResponse::Blocked {
        is_dirty, blockers, ..
    } = prepared
    else {
        panic!("dirty worktree must not issue a token: {prepared:?}");
    };
    assert!(is_dirty);
    assert!(!blockers.is_empty());
    assert!(worktree.path.is_dir());
}

#[test]
fn active_session_blocks_removal() {
    let fixture = Fixture::new();
    let saga = fixture.saga(FakeSpawner::succeeding(23));
    let created = saga
        .create_session(&fixture.request("Still Running", Some("r0nning1")))
        .unwrap();
    let worktree = created.worktree.unwrap();
    assert_eq!(created.session.status, SessionStatus::Running);

    let prepared = saga.prepare_remove(worktree.id).unwrap();
    let WorktreePrepareRemoveResponse::Blocked { blockers, .. } = prepared else {
        panic!("running session must block removal: {prepared:?}");
    };
    assert!(blockers.iter().any(|blocker| {
        matches!(
            blocker,
            cli_master_core::wire::WorktreeRemovalBlocker::Running
                | cli_master_core::wire::WorktreeRemovalBlocker::InUse
        )
    }));
}

#[test]
fn deleting_session_metadata_keeps_the_worktree() {
    let fixture = Fixture::new();
    let saga = fixture.saga(FakeSpawner::succeeding(24));
    let created = saga
        .create_session(&fixture.request("Keep Tree", Some("keep0001")))
        .unwrap();
    let live = saga
        .delete_session(created.session.id)
        .expect_err("live session");
    assert_eq!(live.kind(), SagaErrorKind::SessionInUse);

    saga.record_session_exit(created.session.id, Some(0))
        .unwrap();
    saga.delete_session(created.session.id).unwrap();
    let worktree = created.worktree.unwrap();
    assert!(worktree.path.is_dir());
    let stored = Storage::open(&fixture.database)
        .unwrap()
        .get_worktree(worktree.id)
        .unwrap()
        .expect("worktree metadata remains");
    assert!(stored.session_id.is_none());
    assert_eq!(stored.state, cli_master_storage::WorktreeState::Active);
}

#[test]
fn removing_a_project_never_deletes_the_repository() {
    let fixture = Fixture::new();
    let saga = fixture.saga(FakeSpawner::succeeding(25));
    let created = saga
        .create_session(&fixture.request("Hold Project", Some("proj0001")))
        .unwrap();
    let error = saga
        .remove_project(fixture.project_id)
        .expect_err("referenced project");
    assert_eq!(error.kind(), SagaErrorKind::Storage);
    assert!(fixture.repository.is_dir());
    assert!(created.worktree.unwrap().path.is_dir());
}

#[test]
fn empty_project_metadata_remove_keeps_the_repo_directory() {
    let fixture = Fixture::new();
    let saga = fixture.saga(FakeSpawner::succeeding(26));
    let path = saga.remove_project(fixture.project_id).unwrap();
    assert_eq!(path, fixture.repository);
    assert!(fixture.repository.join("tracked.txt").is_file());
    assert!(
        Storage::open(&fixture.database)
            .unwrap()
            .get_project(fixture.project_id)
            .unwrap()
            .is_none()
    );
}

#[test]
fn token_is_bound_to_the_inspected_clean_state() {
    let fixture = Fixture::new();
    let saga = fixture.saga(FakeSpawner::succeeding(27));
    let created = saga
        .create_session(&fixture.request("State Bound", Some("57a7eb0d")))
        .unwrap();
    saga.record_session_exit(created.session.id, Some(0))
        .unwrap();
    let worktree = created.worktree.unwrap();
    let prepared = saga.prepare_remove(worktree.id).unwrap();
    let WorktreePrepareRemoveResponse::Ready {
        confirmation_token, ..
    } = prepared
    else {
        panic!("expected ready token: {prepared:?}");
    };
    fs::write(worktree.path.join("after-token.txt"), "changed\n").unwrap();
    let error = saga
        .remove_worktree(worktree.id, &confirmation_token)
        .expect_err("state changed");
    assert_eq!(error.kind(), SagaErrorKind::InvalidToken);
    assert!(worktree.path.join("after-token.txt").is_file());
}

#[test]
fn token_is_invalidated_when_the_session_association_changes() {
    let fixture = Fixture::new();
    let saga = fixture.saga(FakeSpawner::succeeding(28));
    let created = saga
        .create_session(&fixture.request("Association Bound", Some("a550c1a7")))
        .unwrap();
    saga.record_session_exit(created.session.id, Some(0))
        .unwrap();
    let worktree = created.worktree.unwrap();
    let prepared = saga.prepare_remove(worktree.id).unwrap();
    let WorktreePrepareRemoveResponse::Ready {
        confirmation_token, ..
    } = prepared
    else {
        panic!("expected ready token: {prepared:?}");
    };
    saga.delete_session(created.session.id).unwrap();

    let error = saga
        .remove_worktree(worktree.id, &confirmation_token)
        .expect_err("session association changed after inspection");

    assert_eq!(error.kind(), SagaErrorKind::InvalidToken);
    assert!(worktree.path.is_dir());
    let stored = Storage::open(&fixture.database)
        .unwrap()
        .get_worktree(worktree.id)
        .unwrap()
        .expect("worktree metadata must remain");
    assert_eq!(stored.state, cli_master_storage::WorktreeState::Active);
    assert!(stored.session_id.is_none());
}
