mod common;

use std::collections::BTreeMap;

use cli_master_agents::{
    AgentRegistry, AgentSource, CustomAgentDefinition, PlaceholderContext, RegistryError,
};
use tempfile::TempDir;

use common::{context, executable};

#[test]
fn custom_agent_can_be_registered_and_started_as_pty_command() {
    let temp = TempDir::new().expect("temporary directory should be created");
    let path = executable(temp.path(), "internal-agent");
    let definition = CustomAgentDefinition::new(
        "internal-agent",
        "Internal Agent",
        path.to_str().expect("UTF-8"),
    )
    .expect("valid")
    .with_args(["--workspace", "${PROJECT_PATH}"])
    .expect("args")
    .with_env(BTreeMap::from([("MODE".to_owned(), "read".to_owned())]))
    .expect("env")
    .with_default_cwd("${PROJECT_PATH}")
    .expect("cwd")
    .with_icon("spark")
    .expect("icon")
    .with_color("#0af")
    .expect("color")
    .with_requires_pty(true);
    let mut registry = AgentRegistry::new();
    registry
        .register_custom(definition.clone())
        .expect("custom agent should register");
    let placeholders = PlaceholderContext::new()
        .with_project_path(temp.path().to_str().expect("UTF-8"))
        .expect("project");
    let cwd = definition
        .resolve_cwd(None, &placeholders)
        .expect("default cwd should expand");
    assert_eq!(cwd, temp.path());
    let context = context(&temp).with_placeholders(placeholders);
    let adapter = registry.get("internal-agent").expect("registered");
    assert_eq!(adapter.source(), AgentSource::Custom);
    assert!(adapter.capabilities().requires_pty);
    let command = adapter.build_command(&context).expect("should build");
    assert_eq!(command.executable(), path.to_str().expect("UTF-8"));
    assert_eq!(
        command.args(),
        ["--workspace", temp.path().to_str().expect("UTF-8")]
    );
    assert_eq!(command.env().get("MODE").map(String::as_str), Some("read"));
}

#[test]
fn custom_definition_rejects_empty_name_and_relative_executable() {
    let empty = CustomAgentDefinition::new("ok", "   ", "agent").expect_err("empty display");
    assert_eq!(empty.field(), "display_name");
    let executable = CustomAgentDefinition::new("ok", "Name", "tools/agent")
        .expect_err("relative path should fail");
    assert_eq!(executable.field(), "executable");
}

#[test]
fn custom_definition_rejects_invalid_env_keys_and_tilde_user() {
    let env_error = CustomAgentDefinition::try_from_parts(
        "ok",
        "Name",
        "agent",
        Vec::<String>::new(),
        BTreeMap::from([("BAD=KEY".to_owned(), "1".to_owned())]),
    )
    .expect_err("invalid env key");
    assert_eq!(env_error.field(), "env");
    let tilde = CustomAgentDefinition::new("ok", "Name", "~other/bin/agent")
        .expect_err("~user should fail");
    assert_eq!(tilde.field(), "executable");
}

#[test]
fn registry_rejects_duplicate_keys_and_display_names() {
    let mut registry = AgentRegistry::new();
    let first = CustomAgentDefinition::new("custom-one", "Custom One", "agent").expect("valid");
    registry.register_custom(first).expect("first custom");
    let duplicate_key =
        CustomAgentDefinition::new("codex", "Other Codex", "agent").expect("valid key clash");
    assert!(matches!(
        registry.register_custom(duplicate_key),
        Err(RegistryError::DuplicateKey(key)) if key == "codex"
    ));
    let duplicate_name =
        CustomAgentDefinition::new("custom-two", "Codex", "agent").expect("valid name clash");
    assert!(matches!(
        registry.register_custom(duplicate_name),
        Err(RegistryError::DuplicateDisplayName(name)) if name == "Codex"
    ));
}

#[test]
fn builtin_agents_cannot_be_unregistered() {
    let mut registry = AgentRegistry::new();
    assert!(matches!(
        registry.unregister("gemini"),
        Err(RegistryError::BuiltInProtected(key)) if key == "gemini"
    ));
}

#[test]
fn args_must_be_an_array_not_a_shell_string() {
    let json = r#"{
        "key": "x",
        "displayName": "X",
        "executable": "x",
        "args": "codex --foo"
    }"#;
    let error = serde_json::from_str::<CustomAgentDefinition>(json)
        .expect_err("shell string args should not deserialize");
    assert!(error.to_string().contains("invalid type"));
}
