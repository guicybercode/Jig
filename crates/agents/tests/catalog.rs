use std::collections::BTreeMap;

use cli_master_agents::{AgentCatalog, CatalogError};
use cli_master_core::{
    AgentCustomCreateRequest, AgentCustomUpdateRequest, AgentSource, LaunchTestStatusDto,
    agent_methods, builtin_agent_ids,
};
use tempfile::TempDir;

use common::{executable, isolated_env};

mod common;

#[test]
fn list_uses_uuid_ids_not_adapter_keys() {
    let catalog = AgentCatalog::new(isolated_env(&TempDir::new().expect("temp")));
    let agents = catalog.list().agents;
    assert_eq!(agents.len(), 4);
    for agent in &agents {
        assert_eq!(agent.id.as_uuid().get_version_num(), 7);
        assert_ne!(agent.id.to_string(), agent.adapter_key);
        assert!(matches!(
            agent.adapter_key.as_str(),
            "codex" | "claude" | "gemini" | "opencode"
        ));
        assert_eq!(agent.source, AgentSource::BuiltIn);
        assert!(agent.enabled);
        assert!(!agent.installed);
        assert!(agent.env_keys.is_empty());
    }
    assert_eq!(
        agents
            .iter()
            .find(|agent| agent.adapter_key == "codex")
            .map(|agent| agent.id),
        Some(builtin_agent_ids::codex())
    );
}

#[test]
fn detect_and_set_enabled_do_not_require_real_clis() {
    let temp = TempDir::new().expect("temp");
    let path = executable(temp.path(), "claude");
    let mut catalog = AgentCatalog::new(isolated_env(&temp));
    let detected = catalog
        .detect(Some(builtin_agent_ids::claude()))
        .expect("claude exists");
    assert_eq!(detected.diagnostics.len(), 1);
    assert!(detected.diagnostics[0].installed);
    assert_eq!(
        detected.diagnostics[0].path.as_deref(),
        Some(path.as_path())
    );
    assert_eq!(
        detected.diagnostics[0].launch_test,
        LaunchTestStatusDto::Success
    );
    let missing = catalog
        .detect(Some(builtin_agent_ids::codex()))
        .expect("codex exists");
    assert!(!missing.diagnostics[0].installed);

    let disabled = catalog
        .set_enabled(builtin_agent_ids::gemini(), false)
        .expect("gemini exists");
    assert!(!disabled.enabled);
    assert_eq!(disabled.id, builtin_agent_ids::gemini());
}

#[test]
fn custom_crud_keeps_env_values_out_of_records_and_debug() {
    let temp = TempDir::new().expect("temp");
    executable(temp.path(), "internal-agent");
    let mut catalog = AgentCatalog::new(isolated_env(&temp));
    let created = catalog
        .create_custom(AgentCustomCreateRequest {
            display_name: "Internal Agent".to_owned(),
            executable: "internal-agent".to_owned(),
            args: vec!["--workspace".to_owned(), "${PROJECT_PATH}".to_owned()],
            env: BTreeMap::from([("ACCESS_TOKEN".to_owned(), "super-secret".to_owned())]),
            default_cwd: None,
            requires_pty: true,
        })
        .expect("custom agent should create");
    assert_eq!(created.source, AgentSource::Custom);
    assert_eq!(created.id.as_uuid().get_version_num(), 7);
    assert_ne!(created.id.to_string(), created.adapter_key);
    assert_eq!(created.env_keys, ["ACCESS_TOKEN"]);
    let serialized = serde_json::to_string(&created).expect("record serializes");
    assert!(!serialized.contains("super-secret"));
    assert!(created.installed);

    let updated = catalog
        .update_custom(AgentCustomUpdateRequest {
            agent_id: created.id,
            display_name: "Internal Agent".to_owned(),
            executable: "internal-agent".to_owned(),
            args: vec!["--quiet".to_owned()],
            env: BTreeMap::from([("ACCESS_TOKEN".to_owned(), "replacement-secret".to_owned())]),
            default_cwd: None,
            requires_pty: true,
        })
        .expect("custom agent should update");
    assert_eq!(updated.default_args, ["--quiet"]);
    assert!(
        !serde_json::to_string(&updated)
            .expect("serialize")
            .contains("replacement-secret")
    );

    catalog
        .remove_custom(created.id)
        .expect("custom agent should remove");
    assert!(
        catalog
            .list()
            .agents
            .iter()
            .all(|agent| agent.id != created.id)
    );
}

#[test]
fn builtins_cannot_be_removed_or_updated() {
    let mut catalog = AgentCatalog::new(isolated_env(&TempDir::new().expect("temp")));
    assert!(matches!(
        catalog.remove_custom(builtin_agent_ids::codex()),
        Err(CatalogError::BuiltInProtected(id)) if id == builtin_agent_ids::codex()
    ));
    let error = catalog
        .update_custom(AgentCustomUpdateRequest {
            agent_id: builtin_agent_ids::codex(),
            display_name: "Codex".to_owned(),
            executable: "codex".to_owned(),
            args: Vec::new(),
            env: BTreeMap::new(),
            default_cwd: None,
            requires_pty: true,
        })
        .expect_err("built-in update should fail");
    assert!(matches!(error, CatalogError::BuiltInProtected(_)));
}

#[test]
fn ipc_method_names_are_stable() {
    assert_eq!(agent_methods::LIST, "agent.list");
    assert_eq!(agent_methods::DETECT, "agent.detect");
    assert_eq!(agent_methods::SET_ENABLED, "agent.set_enabled");
    assert_eq!(agent_methods::CUSTOM_CREATE, "agent.custom.create");
    assert_eq!(agent_methods::CUSTOM_UPDATE, "agent.custom.update");
    assert_eq!(agent_methods::CUSTOM_REMOVE, "agent.custom.remove");
}
