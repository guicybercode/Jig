use cli_master_core::wire::{event_name, method};
use serde_json::{Value, json};

#[test]
fn knowledge_operations_share_the_authoritative_catalog() {
    let catalog: Value =
        serde_json::from_str(include_str!("../../../protocol/catalog.json")).unwrap();
    assert_eq!(catalog["methods"], json!(method::ALL));
    assert_eq!(catalog["events"], json!(event_name::ALL));
    for name in [
        method::KNOWLEDGE_LIST,
        method::KNOWLEDGE_SAVE,
        method::KNOWLEDGE_DELETE,
    ] {
        assert!(method::is_supported(name));
    }
    // The metadata event is deliberately not advertised until its transport exists.
    assert!(!event_name::is_supported("knowledge.updated"));
}
