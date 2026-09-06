use cli_master_core::wire::{
    OrganizationEntry, OrganizationGetRequest, OrganizationGetResponse, method,
};
use serde_json::{Value, json};

#[test]
fn fixture_keeps_request_order_and_explicit_defaults() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../../protocol/fixtures/organization.json")).unwrap();
    let request: OrganizationGetRequest =
        serde_json::from_value(fixture["request"].clone()).unwrap();
    let response: OrganizationGetResponse =
        serde_json::from_value(fixture["response"].clone()).unwrap();
    assert_eq!(
        serde_json::to_value(&response).unwrap(),
        fixture["response"]
    );
    for (target, entry) in request.targets.as_slice().iter().zip(&response.entries) {
        assert_eq!(*target, entry.target);
        assert_eq!(*entry, OrganizationEntry::defaults(*target));
    }
    let saved: OrganizationEntry = serde_json::from_value(fixture["saved"].clone()).unwrap();
    assert_eq!(serde_json::to_value(saved).unwrap(), fixture["saved"]);
}

#[test]
fn catalog_registers_functional_metadata_operations_without_process_aliases() {
    let catalog: Value =
        serde_json::from_str(include_str!("../../../protocol/catalog.json")).unwrap();
    assert_eq!(catalog["methods"], json!(method::ALL));
    assert!(method::is_supported(method::ORGANIZATION_GET));
    assert!(method::is_supported(method::ORGANIZATION_SAVE));
    assert!(!method::is_supported("organization.stop"));
}
