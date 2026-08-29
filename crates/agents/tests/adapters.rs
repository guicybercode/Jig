mod common;

use std::{
    collections::BTreeMap,
    env, fs,
    os::unix::{fs::PermissionsExt, fs::symlink},
    path::{Path, PathBuf},
    sync::Mutex,
};

use cli_master_agents::{
    AgentAdapter, AgentError, AgentRegistry, AgentSource, ClaudeCodeAdapter, CodexAdapter,
    CustomAgentAdapter, CustomAgentDefinition, DetectionResult, GeminiCliAdapter, LaunchContext,
    LaunchEnvironment, OpenCodeAdapter, RegistryError,
};
use tempfile::TempDir;

use common::{context, executable};

static CURRENT_DIR_LOCK: Mutex<()> = Mutex::new(());

struct CurrentDirGuard(PathBuf);

impl CurrentDirGuard {
    fn change_to(path: &Path) -> Self {
        let original = env::current_dir().expect("current directory should be readable");
        env::set_current_dir(path).expect("test current directory should be set");
        Self(original)
    }
}

impl Drop for CurrentDirGuard {
    fn drop(&mut self) {
        env::set_current_dir(&self.0).expect("original current directory should be restored");
    }
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
    assert_eq!(command.cwd(), temp.path());
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
fn rejects_file_executable_only_by_other_users() {
    let temp = TempDir::new().expect("temporary directory should be created");
    let path = temp.path().join("codex");
    fs::write(&path, b"not executable by owner").expect("fixture should be written");
    let mut permissions = fs::metadata(&path)
        .expect("fixture metadata should exist")
        .permissions();
    permissions.set_mode(0o001);
    fs::set_permissions(&path, permissions).expect("fixture mode should be set");
    let environment = LaunchEnvironment::from_search_paths([temp.path()]);

    assert_eq!(
        CodexAdapter.detect(&environment),
        DetectionResult::NotExecutable { candidate: path }
    );
}

#[test]
fn relative_path_entry_resolves_to_stable_absolute_executable() {
    let _lock = CURRENT_DIR_LOCK
        .lock()
        .expect("current directory lock should work");
    let temp = TempDir::new().expect("temporary directory should be created");
    let relative_bin = temp.path().join("relative-bin");
    let session_cwd = temp.path().join("session-cwd");
    fs::create_dir_all(&relative_bin).expect("relative bin directory should be created");
    fs::create_dir_all(&session_cwd).expect("session directory should be created");
    let expected = executable(&relative_bin, "codex")
        .canonicalize()
        .expect("fixture path should canonicalize");
    let _current_dir = CurrentDirGuard::change_to(temp.path());
    let context = LaunchContext::new(&session_cwd, LaunchEnvironment::from_path("relative-bin"));

    let command = CodexAdapter
        .build_command(&context)
        .expect("relative PATH entry should resolve");
    let resolved = Path::new(command.executable());
    assert!(resolved.is_absolute());
    assert_eq!(resolved, expected);

    env::set_current_dir(&session_cwd).expect("session current directory should be set");
    assert!(resolved.is_file());
}

#[test]
fn executable_symlink_resolves_to_regular_target() {
    let temp = TempDir::new().expect("temporary directory should be created");
    let target = executable(temp.path(), "agent-target")
        .canonicalize()
        .expect("fixture target should canonicalize");
    let link = temp.path().join("codex");
    symlink(&target, &link).expect("fixture symlink should be created");
    let environment = LaunchEnvironment::from_search_paths([temp.path()]);

    assert_eq!(
        CodexAdapter.detect(&environment),
        DetectionResult::Found { executable: target }
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
        assert!(adapter.capabilities().requires_pty);
        let command = adapter
            .build_command(&launch_context)
            .expect("built-in command should build");
        assert!(command.args().is_empty());
        assert!(command.env().is_empty());
        assert!(command.env_removals().is_empty());
        assert!(command.startup_input().is_none());
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
fn missing_built_ins_do_not_panic_and_report_not_installed() {
    let temp = TempDir::new().expect("temporary directory should be created");
    let environment = LaunchEnvironment::from_search_paths([temp.path()]);
    let registry = AgentRegistry::new();
    for key in ["codex", "claude", "gemini", "opencode"] {
        let snapshot = registry
            .get(key)
            .expect("built-in")
            .resolve_definition(&environment);
        assert!(!snapshot.installed);
        assert!(snapshot.resolved_path.is_none());
        assert_eq!(snapshot.version, None);
    }
}

#[test]
fn executable_with_spaces_in_path_is_preserved() {
    let temp = TempDir::new().expect("temporary directory should be created");
    let bin_dir = temp.path().join("my tools");
    let expected = executable(&bin_dir, "my agent");
    let environment = LaunchEnvironment::from_search_paths([&bin_dir]);
    let context = LaunchContext::new(temp.path(), environment);

    let definition =
        CustomAgentDefinition::new("spaced", "Spaced Agent", expected.to_str().expect("UTF-8"))
            .expect("definition should be valid");
    let command = CustomAgentAdapter::new(definition)
        .build_command(&context)
        .expect("spaced path should build");
    assert_eq!(command.executable(), expected.to_str().expect("UTF-8"));
}

#[test]
fn extra_args_are_appended_without_shell_joining() {
    let temp = TempDir::new().expect("temporary directory should be created");
    executable(temp.path(), "codex");
    let context = context(&temp)
        .with_extra_args(["--search", "foo bar", "$(uname)"])
        .expect("extra args should be valid");
    let command = CodexAdapter
        .build_command(&context)
        .expect("command should build");
    assert_eq!(command.args(), ["--search", "foo bar", "$(uname)"]);
}

#[test]
fn cwd_error_is_returned_when_directory_is_missing() {
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
fn binary_removed_after_registration_fails_detection() {
    let temp = TempDir::new().expect("temporary directory should be created");
    let path = executable(temp.path(), "company-agent");
    let definition = CustomAgentDefinition::new("company-agent", "Company Agent", "company-agent")
        .expect("definition should be valid");
    let mut registry = AgentRegistry::new();
    registry
        .register_custom(definition)
        .expect("custom agent should register");
    let environment = LaunchEnvironment::from_search_paths([temp.path()]);
    assert!(
        registry
            .get("company-agent")
            .expect("registered")
            .detect(&environment)
            .is_found()
    );
    fs::remove_file(&path).expect("fixture should be removable");
    assert_eq!(
        registry
            .get("company-agent")
            .expect("registered")
            .detect(&environment),
        DetectionResult::NotFound
    );
    let context = LaunchContext::new(temp.path(), environment);
    assert_eq!(
        registry
            .get("company-agent")
            .expect("registered")
            .build_command(&context),
        Err(AgentError::ExecutableNotFound)
    );
}

#[test]
fn deserialization_rejects_nul_and_invalid_environment_name() {
    let nul_argument = r#"{
        "key":"custom",
        "displayName":"Custom",
        "executable":"agent",
        "args":["bad\u0000argument"],
        "env":{}
    }"#;
    let invalid_environment = r#"{
        "key":"custom",
        "displayName":"Custom",
        "executable":"agent",
        "args":[],
        "env":{"BAD=KEY":"value"}
    }"#;

    let nul_error = serde_json::from_str::<CustomAgentDefinition>(nul_argument)
        .expect_err("NUL argument must be rejected");
    let env_error = serde_json::from_str::<CustomAgentDefinition>(invalid_environment)
        .expect_err("invalid environment name must be rejected");
    assert!(
        nul_error
            .to_string()
            .contains("must not contain a NUL byte")
    );
    assert!(
        env_error
            .to_string()
            .contains("variable names must be non-empty")
    );
}

#[test]
fn executable_override_skips_path_lookup() {
    let temp = TempDir::new().expect("temporary directory should be created");
    let override_path = executable(temp.path(), "codex-override");
    let context = LaunchContext::new(temp.path(), LaunchEnvironment::default())
        .with_executable_override(&override_path);
    let command = CodexAdapter
        .build_command(&context)
        .expect("override should build");
    assert_eq!(command.executable(), override_path.to_str().expect("UTF-8"));
}

#[test]
fn registry_rejects_duplicate_key_and_detects_all_adapters() {
    let temp = TempDir::new().expect("temporary directory should be created");
    for name in ["codex", "claude", "gemini", "opencode"] {
        executable(temp.path(), name);
    }
    let duplicate = CustomAgentAdapter::new(
        CustomAgentDefinition::new("codex", "Duplicate Codex", "codex")
            .expect("custom definition should be valid"),
    );
    let mut registry = AgentRegistry::new();

    assert_eq!(
        registry.register(duplicate),
        Err(RegistryError::DuplicateKey("codex".to_owned()))
    );
    let detections = registry.detect_all(&LaunchEnvironment::from_search_paths([temp.path()]));
    assert_eq!(detections.len(), 4);
    assert!(detections.values().all(DetectionResult::is_found));
}
