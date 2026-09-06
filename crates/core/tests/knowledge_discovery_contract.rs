use cli_master_core::wire::{
    KnowledgeDiscoverRequest, KnowledgeDiscoverResponse, KnowledgeReadRequest,
    KnowledgeReadResponse,
};
use serde_json::{Value, json};

#[test]
fn discovery_fixture_preserves_provenance_and_explicit_content() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../protocol/fixtures/knowledge-discovery.json"
    ))
    .unwrap();
    let scan: KnowledgeDiscoverResponse = serde_json::from_value(fixture["scan"].clone()).unwrap();
    let read: KnowledgeReadResponse = serde_json::from_value(fixture["read"].clone()).unwrap();
    assert_eq!(read.entry, scan.entries[0]);
    assert_eq!(serde_json::to_value(&scan).unwrap(), fixture["scan"]);
    assert_eq!(serde_json::to_value(&read).unwrap(), fixture["read"]);
    assert!(!format!("{read:?}").contains("Review the current change."));
}

#[test]
fn requests_accept_only_registered_scopes_and_opaque_v7_selectors() {
    assert!(serde_json::from_value::<KnowledgeDiscoverRequest>(json!({})).is_ok());
    assert!(serde_json::from_value::<KnowledgeDiscoverRequest>(json!({"projectId":null})).is_ok());
    assert!(
        serde_json::from_value::<KnowledgeDiscoverRequest>(json!({"path":"/private"})).is_err()
    );
    let scan = "0198b6e0-0000-7000-8000-000000000001";
    let entry = "0198b6e0-0001-7000-8000-000000000001";
    assert!(
        serde_json::from_value::<KnowledgeReadRequest>(json!({"scanId":scan,"entryId":entry}))
            .is_ok()
    );
    for value in [
        json!({"scanId":scan,"entryId":entry,"path":"/private"}),
        json!({"scanId":scan,"entryId":"../SKILL.md"}),
        json!({"scanId":"0198b6e0-0000-4000-8000-000000000001","entryId":entry}),
        json!({"entryId":entry}),
    ] {
        assert!(serde_json::from_value::<KnowledgeReadRequest>(value).is_err());
    }
}
