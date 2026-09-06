use cli_master_core::{
    ProjectId,
    knowledge::{
        KnowledgeBody, KnowledgeDeleteRequest, KnowledgeEntry, KnowledgeId, KnowledgeKind,
        KnowledgeListRequest, KnowledgeListResponse, KnowledgeSaveRequest, KnowledgeTitle,
        MAX_KNOWLEDGE_BODY_BYTES, MAX_KNOWLEDGE_REVISION, MAX_KNOWLEDGE_TITLE_BYTES,
    },
};
use serde_json::{Value, json};
use uuid::Uuid;

fn create_payload() -> Value {
    json!({"kind": "prompt", "title": "Review", "body": "  Review this change.\n"})
}

fn entry() -> KnowledgeEntry {
    KnowledgeEntry {
        id: KnowledgeId::new(),
        kind: KnowledgeKind::Context,
        project_id: Some(ProjectId::new()),
        title: KnowledgeTitle::try_new("Architecture").unwrap(),
        body: KnowledgeBody::try_new("Keep the process owner in the daemon.\n").unwrap(),
        revision: 2,
        created_at_ms: 1_778_000_000_000,
        updated_at_ms: 1_778_000_000_001,
    }
}

#[test]
fn shared_page_fixture_matches_the_typed_contract_exactly() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../protocol/fixtures/knowledge-page.json"
    ))
    .unwrap();
    let page: KnowledgeListResponse = serde_json::from_value(fixture.clone()).unwrap();
    assert!(page.entries[0].id < page.entries[1].id);
    assert_eq!(page.entries[0].project_id, None);
    assert!(page.entries[1].project_id.is_some());
    assert_eq!(page.next_cursor, None);
    assert_eq!(serde_json::to_value(page).unwrap(), fixture);
}

#[test]
fn entry_timestamps_reject_negative_reversed_and_unsafe_values() {
    for (created, updated) in [(-1, 0), (2, 1), (0, 9_007_199_254_740_992)] {
        let mut invalid = entry();
        invalid.created_at_ms = created;
        invalid.updated_at_ms = updated;
        assert!(invalid.validate().is_err());
        assert!(serde_json::from_value::<KnowledgeEntry>(json!(invalid)).is_err());
    }
}

#[test]
fn identifiers_validate_version_and_variant_without_echoing_input() {
    let id = KnowledgeId::new();
    assert_eq!(id.as_uuid().get_version_num(), 7);
    assert_eq!(id.to_string().parse::<KnowledgeId>().unwrap(), id);
    assert_eq!(KnowledgeId::try_from_uuid(id.into_uuid()).unwrap(), id);
    assert_eq!(
        serde_json::from_value::<KnowledgeId>(json!(id)).unwrap(),
        id
    );
    assert_eq!(KnowledgeId::default().as_uuid().get_version_num(), 7);

    for invalid in [
        "secret prompt text",
        "00000000-0000-0000-0000-000000000000",
        "550e8400-e29b-41d4-a716-446655440000",
        "01900000-0000-7000-0000-000000000000",
    ] {
        let error = invalid.parse::<KnowledgeId>().unwrap_err();
        assert_eq!(error.field(), "id");
        assert!(!error.message().contains(invalid));
        assert!(!error.to_string().contains(invalid));
        assert!(serde_json::from_value::<KnowledgeId>(json!(invalid)).is_err());
    }
    assert!(KnowledgeId::try_from_uuid(Uuid::nil()).is_err());
    assert!(serde_json::from_value::<KnowledgeId>(json!(5)).is_err());
}

#[test]
fn text_validation_counts_utf8_bytes_preserves_whitespace_and_redacts_debug() {
    let title = "á".repeat(MAX_KNOWLEDGE_TITLE_BYTES / 2);
    assert_eq!(
        KnowledgeTitle::try_new(title.clone()).unwrap().into_inner(),
        title
    );
    assert!(KnowledgeTitle::try_new(format!("{title}a")).is_err());
    let body = "á".repeat(MAX_KNOWLEDGE_BODY_BYTES / 2);
    assert_eq!(
        KnowledgeBody::try_new(body.clone()).unwrap().into_inner(),
        body
    );
    assert!(KnowledgeBody::try_new(format!("{body}a")).is_err());

    for invalid in ["", " \t\n", "private\0body"] {
        assert!(KnowledgeTitle::try_new(invalid).is_err());
        assert!(KnowledgeBody::try_new(invalid).is_err());
    }
    let text = "  private-content\n";
    let title = KnowledgeTitle::try_new(text).unwrap();
    let body = KnowledgeBody::try_new(text).unwrap();
    assert_eq!(title.as_str(), text);
    assert_eq!(body.as_str(), text);
    assert!(!format!("{title:?} {body:?}").contains("private-content"));
}

#[test]
fn create_and_update_require_matching_id_revision_presence() {
    let created: KnowledgeSaveRequest = serde_json::from_value(create_payload()).unwrap();
    assert_eq!(created.id, None);
    assert_eq!(created.expected_revision, None);
    assert_eq!(created.body.as_str(), "  Review this change.\n");
    assert_eq!(serde_json::to_value(&created).unwrap(), create_payload());

    let mut updated = create_payload();
    updated["id"] = json!(KnowledgeId::new());
    assert!(serde_json::from_value::<KnowledgeSaveRequest>(updated.clone()).is_err());
    updated["expectedRevision"] = json!(1);
    let request: KnowledgeSaveRequest = serde_json::from_value(updated.clone()).unwrap();
    assert_eq!(request.expected_revision, Some(1));
    assert_eq!(serde_json::to_value(&request).unwrap(), updated);
    updated.as_object_mut().unwrap().remove("id");
    assert!(serde_json::from_value::<KnowledgeSaveRequest>(updated).is_err());

    let mut invalid_programmatic = created;
    invalid_programmatic.expected_revision = Some(1);
    assert!(invalid_programmatic.validate().is_err());
}

#[test]
fn every_mutation_rejects_zero_fractional_and_unsafe_revisions() {
    for revision in [
        json!(0),
        json!(-1),
        json!(1.5),
        json!(MAX_KNOWLEDGE_REVISION + 1),
        json!("1"),
    ] {
        let mut update = create_payload();
        update["id"] = json!(KnowledgeId::new());
        update["expectedRevision"] = revision.clone();
        assert!(serde_json::from_value::<KnowledgeSaveRequest>(update).is_err());
        assert!(
            serde_json::from_value::<KnowledgeDeleteRequest>(json!({
                "id": KnowledgeId::new(), "expectedRevision": revision,
            }))
            .is_err()
        );
        let mut invalid_entry = serde_json::to_value(entry()).unwrap();
        invalid_entry["revision"] = revision;
        assert!(serde_json::from_value::<KnowledgeEntry>(invalid_entry).is_err());
    }

    for revision in [1, MAX_KNOWLEDGE_REVISION] {
        let deletion = KnowledgeDeleteRequest {
            id: KnowledgeId::new(),
            expected_revision: revision,
        };
        let round_trip: KnowledgeDeleteRequest = serde_json::from_value(json!(deletion)).unwrap();
        assert_eq!(round_trip, deletion);
        let mut valid_entry = entry();
        valid_entry.revision = revision;
        assert!(serde_json::from_value::<KnowledgeEntry>(json!(valid_entry)).is_ok());
    }
}

#[test]
fn wire_contract_uses_camel_case_epoch_milliseconds_and_optional_scope() {
    let entry = entry();
    let encoded = serde_json::to_value(&entry).unwrap();
    assert_eq!(encoded["createdAtMs"], 1_778_000_000_000_i64);
    assert_eq!(encoded["updatedAtMs"], 1_778_000_000_001_i64);
    assert_eq!(encoded["projectId"], json!(entry.project_id));
    assert_eq!(encoded["kind"], "context");
    assert_eq!(
        serde_json::from_value::<KnowledgeEntry>(encoded).unwrap(),
        entry
    );

    let mut global = entry;
    global.project_id = None;
    assert_eq!(
        serde_json::to_value(global).unwrap()["projectId"],
        Value::Null
    );
    assert_eq!(
        serde_json::to_value(KnowledgeListRequest::default()).unwrap(),
        json!({})
    );
}

#[test]
fn list_contract_validates_pagination_search_and_response() {
    let id = KnowledgeId::new();
    let project_id = ProjectId::new();
    let request: KnowledgeListRequest = serde_json::from_value(json!({
        "projectId": project_id, "kind": "prompt", "cursor": id, "query": "  secret_%query  ",
    }))
    .unwrap();
    assert_eq!(request.project_id, Some(project_id));
    assert_eq!(request.kind, Some(KnowledgeKind::Prompt));
    assert_eq!(request.cursor, Some(id));
    assert_eq!(request.query.as_deref(), Some("secret_%query"));
    assert!(!format!("{request:?}").contains("secret_%query"));
    assert!(serde_json::from_value::<KnowledgeListRequest>(json!({"query": "   "})).is_ok());
    assert!(
        serde_json::from_value::<KnowledgeListRequest>(json!({"query": "a".repeat(256)})).is_ok()
    );
    for invalid in ["a".repeat(257), "secret\0query".into()] {
        assert!(serde_json::from_value::<KnowledgeListRequest>(json!({"query": invalid})).is_err());
    }

    let response = KnowledgeListResponse {
        entries: vec![entry()],
        next_cursor: Some(id),
    };
    let encoded = serde_json::to_value(&response).unwrap();
    assert_eq!(encoded["nextCursor"], json!(id));
    assert_eq!(
        serde_json::from_value::<KnowledgeListResponse>(encoded).unwrap(),
        response
    );
    assert_eq!(
        serde_json::to_value(KnowledgeListResponse {
            entries: vec![],
            next_cursor: None
        })
        .unwrap(),
        json!({"entries": [], "nextCursor": null}),
    );
}

#[test]
fn schemas_reject_unknown_duplicate_and_unsafe_fields_without_text_leaks() {
    let mut save = create_payload();
    save["private_unknown_content"] = json!(true);
    let error = serde_json::from_value::<KnowledgeSaveRequest>(save).unwrap_err();
    assert!(!error.to_string().contains("private_unknown_content"));
    assert!(
        serde_json::from_str::<KnowledgeSaveRequest>(
            r#"{"kind":"prompt","title":"One","title":"Two","body":"Text"}"#,
        )
        .is_err()
    );
    assert!(serde_json::from_value::<KnowledgeListRequest>(json!({"limit": 99})).is_err());
    assert!(
        serde_json::from_value::<KnowledgeDeleteRequest>(json!({
            "id": KnowledgeId::new(), "expectedRevision": 1, "force": true,
        }))
        .is_err()
    );
    let mut saved = serde_json::to_value(entry()).unwrap();
    saved["extra"] = json!(true);
    assert!(serde_json::from_value::<KnowledgeEntry>(saved).is_err());

    for field in ["title", "body"] {
        let mut invalid = create_payload();
        invalid[field] = json!("private-content\0");
        let error = serde_json::from_value::<KnowledgeSaveRequest>(invalid).unwrap_err();
        assert!(!error.to_string().contains("private-content"));
    }
    let request: KnowledgeSaveRequest = serde_json::from_value(json!({
        "kind": "prompt", "title": "private-title", "body": "private-body",
    }))
    .unwrap();
    let debug = format!("{request:?}");
    assert!(!debug.contains("private-title"));
    assert!(!debug.contains("private-body"));
}
