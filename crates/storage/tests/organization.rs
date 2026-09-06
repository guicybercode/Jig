mod common;

use std::{
    fs,
    sync::{Arc, Barrier},
    thread,
};

use cli_master_core::{
    AgentSource, Project, ProjectId, SessionStatus,
    organization::{
        MAX_ORGANIZATION_REVISION, OrganizationEntry, OrganizationGetRequest,
        OrganizationSaveRequest, OrganizationTarget, OrganizationTargets, OrganizationWorkflow,
    },
};
use cli_master_storage::{Storage, StoredSession, organization::OrganizationStorageError};
use rusqlite::{Connection, TransactionBehavior, params};
use tempfile::TempDir;

const NOW: i64 = 1_788_600_000_000;

struct Fixture {
    root: TempDir,
    storage: Storage,
    project: Project,
    session: StoredSession,
}

impl Fixture {
    fn new() -> Self {
        let root = TempDir::new().unwrap();
        let storage = Storage::open_migrated(root.path().join("organization.db")).unwrap();
        let project = common::project("Organization", root.path().join("project"));
        fs::create_dir_all(&project.path).unwrap();
        fs::write(project.path.join("keep.txt"), b"Repository content").unwrap();
        storage.insert_project(&project).unwrap();
        let agent = common::agent(AgentSource::BuiltIn, "Agent");
        storage.insert_agent(&agent).unwrap();
        let session = common::session(
            project.id,
            agent.id,
            SessionStatus::Running,
            Some("organization-daemon"),
            Some(4_343),
        );
        storage.insert_session(&session).unwrap();
        Self {
            root,
            storage,
            project,
            session,
        }
    }

    fn project_target(&self) -> OrganizationTarget {
        OrganizationTarget::Project {
            id: self.project.id,
        }
    }
    fn session_target(&self) -> OrganizationTarget {
        OrganizationTarget::Session {
            id: self.session.id,
        }
    }
    fn connection(&self) -> Connection {
        Connection::open(self.root.path().join("organization.db")).unwrap()
    }
}

fn get(targets: Vec<OrganizationTarget>) -> OrganizationGetRequest {
    OrganizationGetRequest {
        targets: OrganizationTargets::try_new(targets).unwrap(),
    }
}

fn save(entry: &OrganizationEntry, pinned: bool, archived: bool) -> OrganizationSaveRequest {
    OrganizationSaveRequest {
        target: entry.target,
        expected_revision: entry.revision,
        pinned,
        archived,
        workflow: entry.workflow,
    }
}

fn count_rows(connection: &Connection) -> i64 {
    connection.query_row("SELECT (SELECT COUNT(*) FROM project_organization) + (SELECT COUNT(*) FROM session_organization)", [], |row| row.get(0)).unwrap()
}

#[test]
fn defaults_preserve_request_order_without_persisting_rows() {
    let fixture = Fixture::new();
    let targets = vec![fixture.session_target(), fixture.project_target()];
    let expected = targets
        .iter()
        .copied()
        .map(OrganizationEntry::defaults)
        .collect::<Vec<_>>();
    let raw = fixture.connection();
    assert_eq!(count_rows(&raw), 0);
    for _ in 0..3 {
        assert_eq!(
            fixture
                .storage
                .get_organization(&get(targets.clone()))
                .unwrap()
                .entries,
            expected
        );
    }
    assert_eq!(expected[0].workflow, Some(OrganizationWorkflow::Backlog));
    assert_eq!(expected[1].workflow, None);
    assert_eq!(count_rows(&raw), 0);
}

#[test]
fn saves_survive_reopen_and_returning_to_defaults_retains_revision() {
    let fixture = Fixture::new();
    let project_default = OrganizationEntry::defaults(fixture.project_target());
    let project = fixture
        .storage
        .save_organization(&save(&project_default, true, true), NOW)
        .unwrap();
    assert_eq!(project.revision, 1);
    assert_eq!(project.updated_at_ms, Some(NOW));
    let mut session_request = save(
        &OrganizationEntry::defaults(fixture.session_target()),
        false,
        true,
    );
    session_request.workflow = Some(OrganizationWorkflow::InProgress);
    let mut session = fixture
        .storage
        .save_organization(&session_request, NOW)
        .unwrap();
    for workflow in [
        OrganizationWorkflow::InReview,
        OrganizationWorkflow::Blocked,
        OrganizationWorkflow::Done,
    ] {
        let mut request = save(&session, true, false);
        request.workflow = Some(workflow);
        session = fixture
            .storage
            .save_organization(&request, NOW + 1)
            .unwrap();
    }
    let restored = fixture
        .storage
        .save_organization(&save(&project, false, false), NOW - 100)
        .unwrap();
    assert_eq!(restored.revision, 2);
    assert_eq!(restored.updated_at_ms, Some(NOW));
    assert_eq!(count_rows(&fixture.connection()), 2);
    fixture.storage.close().unwrap();
    let path = fixture.root.path().join("organization.db");
    drop(fixture.storage);
    let reopened = Storage::open_migrated(path).unwrap();
    assert_eq!(
        reopened
            .get_organization(&get(vec![session.target, restored.target]))
            .unwrap()
            .entries,
        vec![session, restored]
    );
}

#[test]
fn archiving_running_session_and_workflow_changes_preserve_native_metadata_and_files() {
    let fixture = Fixture::new();
    let original_project = fixture.storage.get_project(fixture.project.id).unwrap();
    let original_session = fixture.storage.get_session(fixture.session.id).unwrap();
    let mut request = save(
        &OrganizationEntry::defaults(fixture.session_target()),
        true,
        true,
    );
    request.workflow = Some(OrganizationWorkflow::Done);
    let entry = fixture.storage.save_organization(&request, NOW).unwrap();
    assert!(entry.archived);
    assert_eq!(
        fixture.storage.get_project(fixture.project.id).unwrap(),
        original_project
    );
    assert_eq!(
        fixture.storage.get_session(fixture.session.id).unwrap(),
        original_session
    );
    assert_eq!(
        fs::read(fixture.project.path.join("keep.txt")).unwrap(),
        b"Repository content"
    );
}

fn compete(
    path: &std::path::Path,
    first: OrganizationSaveRequest,
    second: OrganizationSaveRequest,
) -> Vec<Result<OrganizationEntry, OrganizationStorageError>> {
    let first_storage = Storage::open_migrated(path).unwrap();
    let second_storage = Storage::open_migrated(path).unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let handles = [(first_storage, first), (second_storage, second)]
        .into_iter()
        .map(|(storage, request)| {
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                storage.save_organization(&request, NOW)
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect()
}

#[test]
fn independent_connections_resolve_first_insert_and_update_races_with_one_winner() {
    let fixture = Fixture::new();
    let path = fixture.root.path().join("organization.db");
    let defaults = OrganizationEntry::defaults(fixture.project_target());
    for baseline in [
        defaults.clone(),
        OrganizationEntry {
            pinned: true,
            revision: 1,
            updated_at_ms: Some(NOW),
            ..defaults
        },
    ] {
        let outcomes = compete(
            &path,
            save(&baseline, true, false),
            save(&baseline, false, true),
        );
        assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| matches!(outcome, Err(OrganizationStorageError::Conflict)))
                .count(),
            1
        );
        let winner = fixture
            .storage
            .get_organization(&get(vec![baseline.target]))
            .unwrap()
            .entries
            .remove(0);
        assert_eq!(winner.revision, baseline.revision + 1);
        assert_ne!(winner.pinned, winner.archived);
        let stale = fixture
            .storage
            .save_organization(&save(&baseline, false, false), NOW)
            .unwrap_err();
        assert_eq!(stale.to_api_error().code, "organization_conflict");
        assert_eq!(
            fixture
                .storage
                .get_organization(&get(vec![baseline.target]))
                .unwrap()
                .entries,
            vec![winner]
        );
    }
}

#[test]
fn mixed_batches_observe_one_snapshot_during_concurrent_atomic_updates() {
    let fixture = Fixture::new();
    for target in [fixture.project_target(), fixture.session_target()] {
        fixture
            .storage
            .save_organization(
                &save(&OrganizationEntry::defaults(target), false, false),
                NOW,
            )
            .unwrap();
    }
    let path = fixture.root.path().join("organization.db");
    let project_id = fixture.project.id.to_string();
    let session_id = fixture.session.id.to_string();
    let barrier = Arc::new(Barrier::new(2));
    let writer_barrier = Arc::clone(&barrier);
    let writer = thread::spawn(move || {
        let mut connection = Connection::open(path).unwrap();
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        writer_barrier.wait();
        for index in 0..80 {
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            transaction.execute("UPDATE project_organization SET pinned = ?1, revision = revision + 1 WHERE project_id = ?2", params![index % 2, project_id]).unwrap();
            thread::yield_now();
            transaction.execute("UPDATE session_organization SET pinned = ?1, revision = revision + 1 WHERE session_id = ?2", params![index % 2, session_id]).unwrap();
            transaction.commit().unwrap();
        }
    });
    barrier.wait();
    let request = get(vec![fixture.session_target(), fixture.project_target()]);
    for _ in 0..100 {
        let entries = fixture.storage.get_organization(&request).unwrap().entries;
        assert_eq!(entries[0].revision, entries[1].revision);
        assert_eq!(entries[0].pinned, entries[1].pinned);
    }
    writer.join().unwrap();
}

#[test]
fn unknown_entities_abort_whole_batches_and_failed_saves_create_nothing() {
    let fixture = Fixture::new();
    for target in [
        OrganizationTarget::Project {
            id: ProjectId::new(),
        },
        OrganizationTarget::Session {
            id: cli_master_core::SessionId::new(),
        },
    ] {
        let expected = match target {
            OrganizationTarget::Project { .. } => "project_not_found",
            OrganizationTarget::Session { .. } => "session_not_found",
        };
        let error = fixture
            .storage
            .get_organization(&get(vec![
                fixture.project_target(),
                target,
                fixture.session_target(),
            ]))
            .unwrap_err();
        assert_eq!(error.to_api_error().code, expected);
        let error = fixture
            .storage
            .save_organization(&save(&OrganizationEntry::defaults(target), true, true), NOW)
            .unwrap_err();
        assert_eq!(error.to_api_error().code, expected);
    }
    assert_eq!(count_rows(&fixture.connection()), 0);
}

#[test]
fn foreign_keys_remove_only_organization_rows_when_native_metadata_is_removed() {
    let fixture = Fixture::new();
    for target in [fixture.project_target(), fixture.session_target()] {
        fixture
            .storage
            .save_organization(&save(&OrganizationEntry::defaults(target), true, true), NOW)
            .unwrap();
    }
    fixture
        .storage
        .remove_session_metadata(fixture.session.id)
        .unwrap();
    assert_eq!(count_rows(&fixture.connection()), 1);
    fixture
        .storage
        .remove_project_metadata(fixture.project.id)
        .unwrap();
    assert_eq!(count_rows(&fixture.connection()), 0);
    assert_eq!(
        fs::read(fixture.project.path.join("keep.txt")).unwrap(),
        b"Repository content"
    );
}

#[test]
fn validation_constraints_and_revision_exhaustion_preserve_previous_values() {
    let fixture = Fixture::new();
    let defaults = OrganizationEntry::defaults(fixture.project_target());
    let mut invalid = save(&defaults, true, false);
    invalid.workflow = Some(OrganizationWorkflow::Done);
    assert!(fixture.storage.save_organization(&invalid, NOW).is_err());
    invalid.workflow = None;
    for now in [-1, 9_007_199_254_740_992] {
        assert!(fixture.storage.save_organization(&invalid, now).is_err());
    }
    let saved = fixture.storage.save_organization(&invalid, NOW).unwrap();
    let raw = fixture.connection();
    for sql in [
        "UPDATE project_organization SET pinned = 2",
        "UPDATE project_organization SET archived = 'false'",
        "UPDATE project_organization SET revision = 0",
        "UPDATE project_organization SET revision = 9007199254740992",
        "UPDATE project_organization SET updated_at_ms = -1",
    ] {
        assert!(raw.execute(sql, []).is_err());
    }
    assert_eq!(
        fixture
            .storage
            .get_organization(&get(vec![saved.target]))
            .unwrap()
            .entries,
        vec![saved.clone()]
    );
    raw.execute(
        "UPDATE project_organization SET revision = ?1",
        [i64::try_from(MAX_ORGANIZATION_REVISION).unwrap()],
    )
    .unwrap();
    let exhausted = OrganizationEntry {
        revision: MAX_ORGANIZATION_REVISION,
        ..saved
    };
    assert!(matches!(
        fixture
            .storage
            .save_organization(&save(&exhausted, false, false), NOW + 1),
        Err(OrganizationStorageError::RevisionExhausted)
    ));
    assert_eq!(
        fixture
            .storage
            .get_organization(&get(vec![exhausted.target]))
            .unwrap()
            .entries,
        vec![exhausted]
    );
}

#[test]
fn corrupt_workflow_is_rejected_without_exposing_persisted_values() {
    let fixture = Fixture::new();
    fixture
        .storage
        .save_organization(
            &save(
                &OrganizationEntry::defaults(fixture.session_target()),
                false,
                false,
            ),
            NOW,
        )
        .unwrap();
    let raw = fixture.connection();
    raw.execute_batch("PRAGMA ignore_check_constraints = ON")
        .unwrap();
    raw.execute(
        "UPDATE session_organization SET workflow = ?1",
        ["PRIVATE_CORRUPT_SENTINEL"],
    )
    .unwrap();
    let error = fixture
        .storage
        .get_organization(&get(vec![
            fixture.project_target(),
            fixture.session_target(),
        ]))
        .unwrap_err();
    assert!(matches!(error, OrganizationStorageError::CorruptData));
    assert!(
        !format!("{error:?} {}", error.to_api_error().message).contains("PRIVATE_CORRUPT_SENTINEL")
    );
}

#[test]
fn database_constraints_protect_typed_parents_and_session_workflow() {
    let fixture = Fixture::new();
    let raw = fixture.connection();
    raw.pragma_update(None, "foreign_keys", true).unwrap();
    let invalid_workflow = raw.execute(
        "INSERT INTO session_organization (session_id,pinned,archived,workflow,revision,updated_at_ms) VALUES (?1,0,0,'running',1,0)",
        [fixture.session.id.to_string()],
    ).unwrap_err();
    assert_eq!(
        invalid_workflow.sqlite_error().unwrap().extended_code,
        rusqlite::ffi::SQLITE_CONSTRAINT_CHECK
    );
    let wrong_parent = raw.execute(
        "INSERT INTO project_organization (project_id,pinned,archived,revision,updated_at_ms) VALUES (?1,0,0,1,0)",
        [fixture.session.id.to_string()],
    ).unwrap_err();
    assert_eq!(
        wrong_parent.sqlite_error().unwrap().extended_code,
        rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY
    );
    let wrong_parent = raw.execute(
        "INSERT INTO session_organization (session_id,pinned,archived,workflow,revision,updated_at_ms) VALUES (?1,0,0,'backlog',1,0)",
        [fixture.project.id.to_string()],
    ).unwrap_err();
    assert_eq!(
        wrong_parent.sqlite_error().unwrap().extended_code,
        rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY
    );
    assert_eq!(count_rows(&raw), 0);
}
