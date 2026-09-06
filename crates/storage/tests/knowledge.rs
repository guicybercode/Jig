mod common;

use std::{
    collections::BTreeSet,
    sync::{Arc, Barrier},
    thread,
};

use cli_master_core::{
    ProjectId,
    knowledge::{
        KnowledgeBody, KnowledgeDeleteRequest, KnowledgeEntry, KnowledgeKind, KnowledgeListRequest,
        KnowledgeSaveRequest, KnowledgeTitle, MAX_KNOWLEDGE_BODY_BYTES, MAX_KNOWLEDGE_REVISION,
    },
};
use cli_master_storage::{
    KnowledgeStorageError, LATEST_SCHEMA_VERSION, Storage,
    knowledge::{MAX_KNOWLEDGE_PAGE_BYTES, MAX_KNOWLEDGE_PAGE_ENTRIES},
};
use rusqlite::{Connection, params};
use tempfile::TempDir;

const NOW: i64 = 1_788_600_000_000;

fn create(
    project_id: Option<ProjectId>,
    kind: KnowledgeKind,
    title: &str,
    body: &str,
) -> KnowledgeSaveRequest {
    KnowledgeSaveRequest {
        id: None,
        expected_revision: None,
        project_id,
        kind,
        title: KnowledgeTitle::try_new(title).unwrap(),
        body: KnowledgeBody::try_new(body).unwrap(),
    }
}

fn update(entry: &KnowledgeEntry, body: &str) -> KnowledgeSaveRequest {
    KnowledgeSaveRequest {
        id: Some(entry.id),
        expected_revision: Some(entry.revision),
        project_id: entry.project_id,
        kind: entry.kind,
        title: entry.title.clone(),
        body: KnowledgeBody::try_new(body).unwrap(),
    }
}

fn list(project_id: Option<ProjectId>) -> KnowledgeListRequest {
    KnowledgeListRequest {
        project_id,
        ..KnowledgeListRequest::default()
    }
}

#[test]
fn entries_survive_reopen_with_scopes_kind_search_and_monotonic_timestamps() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("knowledge.db");
    let storage = Storage::open_migrated(&path).unwrap();
    let project = common::project("First", directory.path().join("first"));
    let other = common::project("Other", directory.path().join("other"));
    storage.insert_project(&project).unwrap();
    storage.insert_project(&other).unwrap();
    let global = storage
        .save_knowledge(
            &create(None, KnowledgeKind::Prompt, "Review", "Check failures"),
            NOW,
        )
        .unwrap();
    let scoped = storage
        .save_knowledge(
            &create(
                Some(project.id),
                KnowledgeKind::Context,
                "Architecture",
                "Daemon owns PTYs",
            ),
            NOW,
        )
        .unwrap();
    storage
        .save_knowledge(
            &create(Some(other.id), KnowledgeKind::Prompt, "Other", "Unrelated"),
            NOW,
        )
        .unwrap();
    assert_eq!(global.revision, 1);
    let changed = storage
        .save_knowledge(&update(&scoped, "Daemon owns processes"), NOW - 1)
        .unwrap();
    assert_eq!(changed.created_at_ms, NOW);
    assert_eq!(changed.updated_at_ms, NOW);
    assert_eq!(changed.revision, 2);
    storage.close().unwrap();
    drop(storage);

    let reopened = Storage::open_migrated(&path).unwrap();
    assert_eq!(
        reopened.list_knowledge(&list(None)).unwrap().entries,
        vec![global]
    );
    let scoped_page = reopened.list_knowledge(&list(Some(project.id))).unwrap();
    assert_eq!(scoped_page.entries.len(), 2);
    assert!(scoped_page.entries.contains(&changed));
    let mut filtered = list(Some(project.id));
    filtered.kind = Some(KnowledgeKind::Context);
    assert_eq!(
        reopened.list_knowledge(&filtered).unwrap().entries,
        vec![changed.clone()]
    );
    filtered.query = Some("  PROCESSES  ".to_owned());
    assert_eq!(
        reopened.list_knowledge(&filtered).unwrap().entries,
        vec![changed]
    );
}

#[test]
fn literal_search_treats_percent_underscore_and_sql_syntax_as_text() {
    let directory = TempDir::new().unwrap();
    let storage = Storage::open_migrated(directory.path().join("literal.db")).unwrap();
    let exact = storage
        .save_knowledge(
            &create(
                None,
                KnowledgeKind::Prompt,
                "Literal %_",
                "SELECT ' OR 1=1 --",
            ),
            NOW,
        )
        .unwrap();
    storage
        .save_knowledge(
            &create(None, KnowledgeKind::Prompt, "Other", "Different"),
            NOW,
        )
        .unwrap();
    for text in ["%_", "literal", "' OR 1=1 --"] {
        let request = KnowledgeListRequest {
            query: Some(text.to_owned()),
            ..list(None)
        };
        assert_eq!(
            storage.list_knowledge(&request).unwrap().entries,
            vec![exact.clone()]
        );
    }
    assert_eq!(
        storage
            .list_knowledge(&KnowledgeListRequest {
                query: Some(String::new()),
                ..list(None)
            })
            .unwrap()
            .entries
            .len(),
        2
    );
}

#[test]
fn missing_projects_fail_and_explicit_scope_move_obeys_cascade() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("scope.db");
    let storage = Storage::open_migrated(&path).unwrap();
    let project = common::project("Scope", directory.path().join("project"));
    std::fs::create_dir(&project.path).unwrap();
    std::fs::write(project.path.join("keep.txt"), "repository content").unwrap();
    storage.insert_project(&project).unwrap();
    let global = storage
        .save_knowledge(
            &create(None, KnowledgeKind::Prompt, "Global", "Preserved"),
            NOW,
        )
        .unwrap();
    let scoped = storage
        .save_knowledge(
            &create(
                Some(project.id),
                KnowledgeKind::Prompt,
                "Scoped",
                "Project data",
            ),
            NOW,
        )
        .unwrap();
    let missing_project = ProjectId::new();
    assert!(matches!(
        storage.list_knowledge(&list(Some(missing_project))),
        Err(KnowledgeStorageError::ProjectNotFound)
    ));
    assert!(matches!(
        storage.save_knowledge(
            &create(
                Some(missing_project),
                KnowledgeKind::Context,
                "Missing",
                "Should not save"
            ),
            NOW
        ),
        Err(KnowledgeStorageError::ProjectNotFound)
    ));
    let mut move_request = update(&global, "Attempted edit");
    move_request.project_id = Some(missing_project);
    assert!(matches!(
        storage.save_knowledge(&move_request, NOW + 1),
        Err(KnowledgeStorageError::ProjectNotFound)
    ));
    assert_eq!(
        storage.list_knowledge(&list(None)).unwrap().entries,
        vec![global]
    );
    let mut export = update(&scoped, "Exported to global");
    export.project_id = None;
    let exported = storage.save_knowledge(&export, NOW + 1).unwrap();
    storage
        .save_knowledge(
            &create(
                Some(project.id),
                KnowledgeKind::Context,
                "Disposable",
                "Removed with metadata",
            ),
            NOW,
        )
        .unwrap();
    storage.remove_project_metadata(project.id).unwrap();
    let globals = storage.list_knowledge(&list(None)).unwrap().entries;
    assert_eq!(globals.len(), 2);
    assert!(globals.contains(&exported));
    assert_eq!(
        std::fs::read_to_string(project.path.join("keep.txt")).unwrap(),
        "repository content"
    );
    let raw = Connection::open(path).unwrap();
    let remaining: i64 = raw
        .query_row(
            "SELECT COUNT(*) FROM knowledge_documents WHERE project_id IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(remaining, 0);
}

#[test]
fn independent_connections_cannot_overwrite_the_same_revision() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("concurrent.db");
    let original = Storage::open_migrated(&path).unwrap();
    let entry = original
        .save_knowledge(&create(None, KnowledgeKind::Prompt, "Race", "Initial"), NOW)
        .unwrap();
    let first = Storage::open_migrated(&path).unwrap();
    let second = Storage::open_migrated(&path).unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let mut handles = Vec::new();
    for (storage, text) in [(first, "First writer"), (second, "Second writer")] {
        let barrier = Arc::clone(&barrier);
        let request = update(&entry, text);
        handles.push(thread::spawn(move || {
            barrier.wait();
            storage.save_knowledge(&request, NOW + 1)
        }));
    }
    barrier.wait();
    let outcomes: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, Err(KnowledgeStorageError::Conflict)))
            .count(),
        1
    );
    let winner = original
        .list_knowledge(&list(None))
        .unwrap()
        .entries
        .remove(0);
    assert_eq!(winner.revision, 2);
    let stale_delete = KnowledgeDeleteRequest {
        id: entry.id,
        expected_revision: entry.revision,
    };
    let conflict = original.delete_knowledge(&stale_delete).unwrap_err();
    assert_eq!(conflict.to_api_error().code, "knowledge_conflict");
    original
        .delete_knowledge(&KnowledgeDeleteRequest {
            id: winner.id,
            expected_revision: winner.revision,
        })
        .unwrap();
    let absent = original.delete_knowledge(&stale_delete).unwrap_err();
    assert_eq!(absent.to_api_error().code, "knowledge_not_found");
    assert!(matches!(
        original.save_knowledge(&update(&winner, "Gone"), NOW),
        Err(KnowledgeStorageError::NotFound)
    ));
}

#[test]
fn pagination_is_bounded_ordered_and_exclusive_without_duplicate_or_missing_rows() {
    let directory = TempDir::new().unwrap();
    let storage = Storage::open_migrated(directory.path().join("pages.db")).unwrap();
    let mut expected = BTreeSet::new();
    for index in 0..57 {
        let entry = storage
            .save_knowledge(
                &create(
                    None,
                    KnowledgeKind::Prompt,
                    &format!("Entry {index}"),
                    "A small body",
                ),
                NOW,
            )
            .unwrap();
        expected.insert(entry.id);
    }
    let first = storage.list_knowledge(&list(None)).unwrap();
    assert_eq!(first.entries.len(), MAX_KNOWLEDGE_PAGE_ENTRIES);
    assert_eq!(
        first.next_cursor,
        first.entries.last().map(|entry| entry.id)
    );
    let second = storage
        .list_knowledge(&KnowledgeListRequest {
            cursor: first.next_cursor,
            ..list(None)
        })
        .unwrap();
    assert_eq!(second.entries.len(), 7);
    assert_eq!(second.next_cursor, None);
    let seen: Vec<_> = first
        .entries
        .into_iter()
        .chain(second.entries)
        .map(|entry| entry.id)
        .collect();
    assert!(seen.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(seen.into_iter().collect::<BTreeSet<_>>(), expected);
}

#[test]
fn worst_case_json_escaping_fits_one_entry_and_pages_without_skipping() {
    let directory = TempDir::new().unwrap();
    let storage = Storage::open_migrated(directory.path().join("escaped.db")).unwrap();
    let body = "\u{0001}".repeat(MAX_KNOWLEDGE_BODY_BYTES);
    let mut expected = BTreeSet::new();
    for index in 0..3 {
        expected.insert(
            storage
                .save_knowledge(
                    &create(
                        None,
                        KnowledgeKind::Context,
                        &format!("Large {index}"),
                        &body,
                    ),
                    NOW,
                )
                .unwrap()
                .id,
        );
    }
    let mut request = list(None);
    let mut seen = BTreeSet::new();
    loop {
        let page = storage.list_knowledge(&request).unwrap();
        assert_eq!(page.entries.len(), 1);
        assert!(serde_json::to_vec(&page).unwrap().len() <= MAX_KNOWLEDGE_PAGE_BYTES);
        assert_eq!(page.entries[0].body.as_str(), body);
        assert!(seen.insert(page.entries[0].id));
        request.cursor = page.next_cursor;
        if request.cursor.is_none() {
            break;
        }
    }
    assert_eq!(seen, expected);
}

#[test]
fn exhausted_revisions_reject_updates_but_allow_checked_deletion() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("exhausted.db");
    let storage = Storage::open_migrated(&path).unwrap();
    let entry = storage
        .save_knowledge(
            &create(None, KnowledgeKind::Prompt, "Max revision", "Retained"),
            NOW,
        )
        .unwrap();
    let raw = Connection::open(path).unwrap();
    raw.execute(
        "UPDATE knowledge_documents SET revision = ?1 WHERE id = ?2",
        params![
            i64::try_from(MAX_KNOWLEDGE_REVISION).unwrap(),
            entry.id.to_string()
        ],
    )
    .unwrap();
    let mut exhausted = entry;
    exhausted.revision = MAX_KNOWLEDGE_REVISION;
    assert!(matches!(
        storage.save_knowledge(&update(&exhausted, "Never stored"), NOW + 1),
        Err(KnowledgeStorageError::RevisionExhausted)
    ));
    assert_eq!(
        storage.list_knowledge(&list(None)).unwrap().entries,
        vec![exhausted.clone()]
    );
    storage
        .delete_knowledge(&KnowledgeDeleteRequest {
            id: exhausted.id,
            expected_revision: exhausted.revision,
        })
        .unwrap();
}

#[test]
fn programmatic_input_constraints_and_corrupt_rows_never_echo_content() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("validation.db");
    let storage = Storage::open_migrated(&path).unwrap();
    let mut invalid = create(None, KnowledgeKind::Prompt, "Private title", "Private body");
    invalid.expected_revision = Some(1);
    assert!(storage.save_knowledge(&invalid, NOW).is_err());
    invalid.expected_revision = None;
    for timestamp in [-1, 9_007_199_254_740_992] {
        assert!(matches!(
            storage.save_knowledge(&invalid, timestamp),
            Err(KnowledgeStorageError::InvalidTimestamp)
        ));
    }
    let invalid_query = KnowledgeListRequest {
        query: Some("private-query\0".into()),
        ..list(None)
    };
    let error = storage.list_knowledge(&invalid_query).unwrap_err();
    assert!(!format!("{error:?}").contains("private-query"));
    let entry = storage.save_knowledge(&invalid, NOW).unwrap();
    let raw = Connection::open(path).unwrap();
    raw.execute_batch("PRAGMA ignore_check_constraints = ON")
        .unwrap();
    raw.execute(
        "UPDATE knowledge_documents SET body = ?1 WHERE id = ?2",
        params!["PRIVATE_CORRUPT_CONTENT\0", entry.id.to_string()],
    )
    .unwrap();
    let error = storage.list_knowledge(&list(None)).unwrap_err();
    assert!(matches!(error, KnowledgeStorageError::CorruptData));
    assert!(
        !format!("{error:?} {}", error.to_api_error().message).contains("PRIVATE_CORRUPT_CONTENT")
    );
}

#[test]
fn version_three_upgrade_preserves_metadata_and_checks_new_schema_objects() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("v3.db");
    let project = common::project("Existing", directory.path().join("existing"));
    {
        let raw = Connection::open(&path).unwrap();
        raw.execute_batch(include_str!("../migrations/0001_initial.sql"))
            .unwrap();
        raw.execute_batch(include_str!("../migrations/0002_worktree_dirty_state.sql"))
            .unwrap();
        raw.execute_batch(include_str!("../migrations/0003_recovery_metadata.sql"))
            .unwrap();
        raw.execute_batch("CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, name TEXT NOT NULL, applied_at TEXT NOT NULL DEFAULT '2026-09-05');
            INSERT INTO schema_migrations (version, name) VALUES (1, 'initial'), (2, 'worktree_dirty_state'), (3, 'recovery_metadata');").unwrap();
        raw.execute(
            "INSERT INTO projects (id,name,path,created_at,last_opened_at) VALUES (?1,?2,?3,?4,?4)",
            params![
                project.id.to_string(),
                project.name,
                project.path.to_str().unwrap(),
                common::CREATED_AT_MS
            ],
        )
        .unwrap();
    }
    let storage = Storage::open_migrated(&path).unwrap();
    assert_eq!(storage.schema_version().unwrap(), LATEST_SCHEMA_VERSION);
    assert_eq!(storage.get_project(project.id).unwrap(), Some(project));
    storage.migrate().unwrap();
    storage
        .save_knowledge(
            &create(None, KnowledgeKind::Prompt, "After upgrade", "New data"),
            NOW,
        )
        .unwrap();
    let raw = Connection::open(&path).unwrap();
    let indexes: i64 = raw.query_row("SELECT COUNT(*) FROM sqlite_schema WHERE type='index' AND name IN ('knowledge_by_scope_id','knowledge_by_scope_kind_id')", [], |row| row.get(0)).unwrap();
    assert_eq!(indexes, 2);
    raw.execute_batch("DROP INDEX knowledge_by_scope_id")
        .unwrap();
    assert!(storage.migrate().is_err());
}
