mod support;

use std::fs;

use cli_master_core::WorktreeId;
use cli_master_core::wire::WorktreePrepareRemoveResponse;
use cli_master_git::Git;
use cli_master_session::{FakeSpawner, SagaErrorKind};
use cli_master_storage::{Storage, StoredWorktree, WorktreeState};
use support::{CREATED_AT_MS, Fixture};

#[test]
fn restart_drops_in_memory_confirmation_tokens() {
    let fixture = Fixture::new();
    let created = {
        let saga = fixture.saga(FakeSpawner::succeeding(31));
        let created = saga
            .create_session(&fixture.request("Restart Token", Some("r357ar7")))
            .unwrap();
        saga.record_session_exit(created.session.id, Some(0))
            .unwrap();
        let worktree = created.worktree.clone().unwrap();
        let prepared = saga.prepare_remove(worktree.id).unwrap();
        let WorktreePrepareRemoveResponse::Ready {
            confirmation_token, ..
        } = prepared
        else {
            panic!("expected token before restart: {prepared:?}");
        };
        (created, confirmation_token)
    };

    let saga = fixture.saga(FakeSpawner::succeeding(31));
    let worktree = created.0.worktree.unwrap();
    let error = saga
        .remove_worktree(worktree.id, &created.1)
        .expect_err("token must not survive restart");
    assert_eq!(error.kind(), SagaErrorKind::InvalidToken);
    assert!(worktree.path.is_dir());
}

#[test]
fn recover_drops_creating_rows_when_git_proves_absence() {
    let fixture = Fixture::new();
    let worktree_id = WorktreeId::new();
    {
        let storage = Storage::open(&fixture.database).unwrap();
        storage
            .insert_worktree(&StoredWorktree {
                id: worktree_id,
                project_id: fixture.project_id,
                session_id: None,
                path: fixture.managed.join("missing-tree"),
                branch: "agent/missing-tree".to_owned(),
                state: WorktreeState::Creating,
                is_dirty: false,
                created_at_ms: CREATED_AT_MS,
                updated_at_ms: CREATED_AT_MS,
            })
            .unwrap();
    }

    let saga = fixture.saga(FakeSpawner::succeeding(32));
    let report = saga.recover().unwrap();
    assert_eq!(report.dropped_creating, vec![worktree_id]);
    assert!(
        Storage::open(&fixture.database)
            .unwrap()
            .get_worktree(worktree_id)
            .unwrap()
            .is_none()
    );
}

#[test]
fn recover_orphans_a_creating_row_when_the_worktree_exists() {
    let fixture = Fixture::new();
    let git = Git::discover().unwrap();
    let created = git
        .create_worktree(
            &fixture.repository,
            &fixture.managed,
            "Orphan Creating",
            "0rphan01",
        )
        .unwrap();
    let worktree_id = WorktreeId::new();
    {
        let storage = Storage::open(&fixture.database).unwrap();
        storage
            .insert_worktree(&StoredWorktree {
                id: worktree_id,
                project_id: fixture.project_id,
                session_id: None,
                path: created.path.clone(),
                branch: created.branch.clone().unwrap(),
                state: WorktreeState::Creating,
                is_dirty: false,
                created_at_ms: CREATED_AT_MS,
                updated_at_ms: CREATED_AT_MS,
            })
            .unwrap();
    }

    let saga = fixture.saga(FakeSpawner::succeeding(33));
    let report = saga.recover().unwrap();
    assert_eq!(report.orphaned, vec![worktree_id]);
    assert!(created.path.is_dir());
    let stored = Storage::open(&fixture.database)
        .unwrap()
        .get_worktree(worktree_id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.state, WorktreeState::Orphaned);
}

#[test]
fn recover_restores_remove_pending_when_the_worktree_still_exists() {
    let fixture = Fixture::new();
    let created = {
        let saga = fixture.saga(FakeSpawner::succeeding(34));
        let created = saga
            .create_session(&fixture.request("Pending Remove", Some("pend0001")))
            .unwrap();
        saga.record_session_exit(created.session.id, Some(0))
            .unwrap();
        let worktree = created.worktree.clone().unwrap();
        let prepared = saga.prepare_remove(worktree.id).unwrap();
        assert!(matches!(
            prepared,
            WorktreePrepareRemoveResponse::Ready { .. }
        ));
        created
    };

    let saga = fixture.saga(FakeSpawner::succeeding(34));
    let report = saga.recover().unwrap();
    let worktree = created.worktree.unwrap();
    assert_eq!(report.restored_active, vec![worktree.id]);
    let stored = Storage::open(&fixture.database)
        .unwrap()
        .get_worktree(worktree.id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.state, WorktreeState::Active);
    assert!(worktree.path.is_dir());
}

#[test]
fn recover_does_not_delete_user_files_in_an_orphaned_tree() {
    let fixture = Fixture::new();
    let git = Git::discover().unwrap();
    let created = git
        .create_worktree(
            &fixture.repository,
            &fixture.managed,
            "Keep Files",
            "keepf11e",
        )
        .unwrap();
    fs::write(created.path.join("notes.md"), "user data\n").unwrap();
    let worktree_id = WorktreeId::new();
    {
        let storage = Storage::open(&fixture.database).unwrap();
        storage
            .insert_worktree(&StoredWorktree {
                id: worktree_id,
                project_id: fixture.project_id,
                session_id: None,
                path: created.path.clone(),
                branch: created.branch.clone().unwrap(),
                state: WorktreeState::Creating,
                is_dirty: false,
                created_at_ms: CREATED_AT_MS,
                updated_at_ms: CREATED_AT_MS,
            })
            .unwrap();
    }

    let saga = fixture.saga(FakeSpawner::succeeding(35));
    saga.recover().unwrap();
    assert_eq!(
        fs::read_to_string(created.path.join("notes.md")).unwrap(),
        "user data\n"
    );
}
