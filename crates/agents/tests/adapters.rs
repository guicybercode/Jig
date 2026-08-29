use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use cli_master_agents::{
    AgentAdapter, AgentError, AgentRegistry, AgentSource, ClaudeCodeAdapter, CodexAdapter,
    CustomAgentAdapter, CustomAgentDefinition, DetectionResult, GeminiCliAdapter, LaunchContext,
    LaunchEnvironment, OpenCodeAdapter,
};
use tempfile::TempDir;

fn executable(directory: &Path, name: &str) -> PathBuf {
    let path = directory.join(name);
    fs::write(&path, b"#!/bin/sh\n").expect("fixture executable should be written");
    let mut permissions = fs::metadata(&path)
        .expect("fixture metadata should exist")
        .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&path, permissions).expect("fixture should become executable");
    path
}

fn context(temp: &TempDir) -> LaunchContext {
    LaunchContext::new(
        temp.path(),
        LaunchEnvironment::from_search_paths([temp.path()]),
    )
}

#[test]
fn detects_executable_in_explicit_path_and_builds_in_context_cwd() {
    let temp = TempDir::new().expect("temporary directory should be created");
    let expected = executable(temp.path(), "codex");
    let adapter = CodexAdapter;
    let context = context(&temp);

    assert_eq!(
        adapter.detect(context.environment()),
        DetectionResult::Found {
            executable: expected.clone()
        }
    );
    let command = adapter
        .build_command(&context)
        .expect("detected adapter should build");
    assert_eq!(
        command.executable(),
        expected.to_str().expect("UTF-8 fixture")
    );
    assert!(command.args().is_empty());
    assert!(command.env().is_empty());
    assert_eq!(command.cwd(), &temp.path());
}

#[test]
fn reports_missing_executable() {
    let temp = TempDir::new().expect("temporary directory should be created");
    let adapter = CodexAdapter;
    let context = context(&temp);

    assert_eq!(
        adapter.detect(context.environment()),
        DetectionResult::NotFound
    );
    assert_eq!(
        adapter.build_command(&context),
        Err(AgentError::ExecutableNotFound)
    );
}

#[test]
fn rejects_non_executable_file() {
    let temp = TempDir::new().expect("temporary directory should be created");
    let path = temp.path().join("codex");
    fs::write(&path, b"not executable").expect("fixture should be written");
    let mut permissions = fs::metadata(&path)
        .expect("fixture metadata should exist")
        .permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(&path, permissions).expect("fixture mode should be set");
    let adapter = CodexAdapter;
    let context = context(&temp);

    assert_eq!(
        adapter.detect(context.environment()),
        DetectionResult::NotExecutable {
            candidate: path.clone()
        }
    );
    assert_eq!(
        adapter.build_command(&context),
        Err(AgentError::ExecutableNotExecutable(path))
    );
}

#[test]
fn resolves_custom_absolute_path_without_path_entry() {
    let temp = TempDir::new().expect("temporary directory should be created");
    let path = executable(temp.path(), "internal-agent");
    let definition = CustomAgentDefinition::new(
        "company-agent",
        "Company Agent",
        path.to_str().expect("UTF-8 fixture"),
    )
    .expect("definition should be valid");
    let adapter = CustomAgentAdapter::new(definition);
    let context = LaunchContext::new(temp.path(), LaunchEnvironment::default());

    let command = adapter
        .build_command(&context)
        .expect("absolute executable should build");
    assert_eq!(command.executable(), path.to_str().expect("UTF-8 fixture"));
}

#[test]
fn custom_arguments_with_spaces_and_metacharacters_stay_separate() {
    let temp = TempDir::new().expect("temporary directory should be created");
    executable(temp.path(), "company-agent");
    let args = [
        "two words",
        "$(touch should-not-exist)",
        "; rm -rf impossible",
        "*.rs",
    ];
    let env = BTreeMap::from([
        ("ACCESS_TOKEN".to_owned(), "override-secret".to_owned()),
        ("MODE".to_owned(), "interactive mode".to_owned()),
    ]);
    let definition = CustomAgentDefinition::try_from_parts(
        "company-agent",
        "Company Agent",
        "company-agent",
        args,
        env.clone(),
    )
    .expect("definition should be valid");
    let adapter = CustomAgentAdapter::new(definition);

    let command = adapter
        .build_command(&context(&temp))
        .expect("custom command should build");
    assert_eq!(command.args(), args);
    assert_eq!(command.env(), &env);
    assert!(!temp.path().join("should-not-exist").exists());

    let debug = format!("{adapter:?} {command:?}");
    assert!(debug.contains("ACCESS_TOKEN"));
    assert!(!debug.contains("override-secret"));
    assert!(!debug.contains("touch should-not-exist"));
    assert!(!debug.contains("interactive mode"));
}

#[test]
fn built_in_registry_keys_and_commands_are_stable() {
    let temp = TempDir::new().expect("temporary directory should be created");
    for name in ["codex", "claude", "gemini", "opencode"] {
        executable(temp.path(), name);
    }
    let registry = AgentRegistry::new();
    assert_eq!(
        registry.keys().collect::<Vec<_>>(),
        ["claude", "codex", "gemini", "opencode"]
    );

    let expected = [
        ("codex", "Codex", CodexAdapter.key()),
        ("claude", "Claude Code", ClaudeCodeAdapter.key()),
        ("gemini", "Gemini CLI", GeminiCliAdapter.key()),
        ("opencode", "OpenCode", OpenCodeAdapter.key()),
    ];
    let launch_context = context(&temp);
    for (key, display_name, direct_key) in expected {
        let adapter = registry.get(key).expect("built-in should be registered");
        assert_eq!(adapter.key(), direct_key);
        assert_eq!(adapter.display_name(), display_name);
        assert_eq!(adapter.source(), AgentSource::BuiltIn);
        let command = adapter
            .build_command(&launch_context)
            .expect("built-in command should build");
        assert!(command.args().is_empty());
        assert!(command.env().is_empty());
    }
}

#[test]
fn later_executable_path_entry_wins_over_non_executable_candidate() {
    let first = TempDir::new().expect("first temporary directory should be created");
    let second = TempDir::new().expect("second temporary directory should be created");
    fs::write(first.path().join("codex"), b"not executable").expect("fixture should be written");
    let executable_path = executable(second.path(), "codex");
    let environment = LaunchEnvironment::from_search_paths([first.path(), second.path()]);

    assert_eq!(
        CodexAdapter.detect(&environment),
        DetectionResult::Found {
            executable: executable_path
        }
    );
}

#[test]
fn build_command_rejects_missing_working_directory() {
    let temp = TempDir::new().expect("temporary directory should be created");
    executable(temp.path(), "codex");
    let missing = temp.path().join("gone");
    let context = LaunchContext::new(
        &missing,
        LaunchEnvironment::from_search_paths([temp.path()]),
    );
    assert_eq!(
        CodexAdapter.build_command(&context),
        Err(AgentError::InvalidWorkingDirectory(missing))
    );
}

#[test]
fn registry_rejects_duplicate_and_empty_keys() {
    use cli_master_agents::{CustomAgentAdapter, CustomAgentDefinition, RegistryError};

    let mut registry = AgentRegistry::new();
    let error = registry
        .register(CodexAdapter)
        .expect_err("duplicate built-in key");
    assert!(matches!(error, RegistryError::DuplicateKey(key) if key == "codex"));

    let definition = CustomAgentDefinition::new("ok", "Ok", "ok").expect("definition");
    let mut empty = AgentRegistry::empty();
    empty
        .register(CustomAgentAdapter::new(definition))
        .expect("custom adapter should register");
}

#[test]
fn custom_definition_rejects_relative_path_with_separator() {
    use cli_master_agents::CustomAgentDefinition;

    let error = CustomAgentDefinition::new("agent", "Agent", "bin/agent")
        .expect_err("relative path with separator");
    assert_eq!(error.field(), "executable");
}
