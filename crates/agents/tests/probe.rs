mod common;

use std::{collections::BTreeMap, time::Duration};

use cli_master_agents::{
    AgentAdapter, AgentRegistry, CustomAgentDefinition, LaunchContext, LaunchTestStatus,
    PlaceholderContext, ProbeOptions, test_executable,
};
use tempfile::TempDir;

use common::{context, executable, isolated_env, script};

#[test]
fn version_probe_captures_first_line_with_timeout() {
    let temp = TempDir::new().expect("temporary directory should be created");
    script(temp.path(), "codex", "echo 'codex-cli 9.9.9'\necho extra");
    let report = test_executable(
        "codex",
        &isolated_env(&temp),
        ProbeOptions::default().with_timeout(Duration::from_secs(2)),
    );
    assert!(report.installed);
    assert_eq!(report.version.as_deref(), Some("codex-cli 9.9.9"));
    assert_eq!(report.launch_test, LaunchTestStatus::Success);
}

#[test]
fn version_probe_times_out_on_hanging_executable() {
    let temp = TempDir::new().expect("temporary directory should be created");
    // Sleep instead of a busy loop so parallel workspace tests cannot starve
    // process creation on Linux CI runners.
    script(temp.path(), "codex", "exec sleep 30");
    let report = test_executable(
        "codex",
        &isolated_env(&temp),
        ProbeOptions::default().with_timeout(Duration::from_millis(200)),
    );
    assert!(report.installed);
    assert_eq!(report.launch_test, LaunchTestStatus::Timeout);
    assert!(report.version.is_none());
}

#[test]
fn an_exhausted_probe_budget_reports_timeout_instead_of_io_failure() {
    let temp = TempDir::new().expect("temporary directory should be created");
    executable(temp.path(), "codex");
    let report = test_executable(
        "codex",
        &isolated_env(&temp),
        ProbeOptions::default().with_timeout(Duration::ZERO),
    );

    assert!(report.installed);
    assert_eq!(report.launch_test, LaunchTestStatus::Timeout);
    assert!(report.version.is_none());
}

#[test]
fn a_missing_script_interpreter_reports_safe_os_diagnostics() {
    let temp = TempDir::new().expect("temporary directory should be created");
    let path = script(temp.path(), "private-agent-name", "echo must-not-run");
    let interpreter = temp.path().join("private-missing-interpreter");
    std::fs::write(
        &path,
        format!("#!{}\necho must-not-run\n", interpreter.display()),
    )
    .expect("fixture should retain executable permissions with an absent interpreter");

    let report = test_executable(&path, &isolated_env(&temp), ProbeOptions::default());
    assert!(report.installed);
    assert!(report.version.is_none());
    let LaunchTestStatus::Failed { message } = &report.launch_test else {
        panic!("missing interpreter must fail the probe: {report:?}");
    };
    let errno = nix::errno::Errno::ENOENT as i32;
    assert_eq!(
        message,
        &format!("version probe could not complete (kind: NotFound, os error: {errno})")
    );
    let diagnostics = format!("{:?} {:?}", report.launch_test, report.warning);
    assert!(!diagnostics.contains("private-agent-name"));
    assert!(!diagnostics.contains("private-missing-interpreter"));
    assert!(!diagnostics.contains("must-not-run"));
}

#[test]
fn test_executable_does_not_depend_on_real_agents() {
    let temp = TempDir::new().expect("temporary directory should be created");
    let report = test_executable("codex", &isolated_env(&temp), ProbeOptions::default());
    assert!(!report.installed);
    assert_eq!(report.launch_test, LaunchTestStatus::NotFound);
}

#[test]
fn diagnostics_include_path_and_launch_test_without_environment() {
    let temp = TempDir::new().expect("temporary directory should be created");
    let path = script(temp.path(), "claude", "echo 'claude 1.2.3'")
        .canonicalize()
        .expect("fixture executable should canonicalize");
    let diagnostics = AgentRegistry::new()
        .diagnostics(
            "claude",
            &isolated_env(&temp),
            ProbeOptions::default().with_timeout(Duration::from_secs(2)),
        )
        .expect("claude is built-in");
    assert!(diagnostics.installed);
    assert_eq!(diagnostics.path.as_deref(), Some(path.as_path()));
    assert_eq!(diagnostics.version.as_deref(), Some("claude 1.2.3"));
    assert_eq!(diagnostics.launch_test, LaunchTestStatus::Success);
    let rendered = format!("{diagnostics:?}");
    assert!(!rendered.contains("TOKEN"));
    assert!(!rendered.contains("HOME="));
}

#[test]
fn placeholders_expand_in_args_env_and_title_not_as_shell() {
    let temp = TempDir::new().expect("temporary directory should be created");
    executable(temp.path(), "company-agent");
    let definition = CustomAgentDefinition::try_from_parts(
        "company-agent",
        "Company Agent",
        "company-agent",
        ["--project", "${PROJECT_PATH}", "$(rm -rf /)"],
        BTreeMap::from([("LABEL".to_owned(), "${SESSION_NAME}".to_owned())]),
    )
    .expect("definition should be valid");
    let placeholders = PlaceholderContext::new()
        .with_project_path("/tmp/demo project")
        .expect("project")
        .with_session_name("Fix login")
        .expect("name");
    let context = context(&temp)
        .with_placeholders(placeholders)
        .with_terminal_title("${SESSION_NAME}")
        .expect("title");
    let command = cli_master_agents::CustomAgentAdapter::new(definition)
        .build_command(&context)
        .expect("command should build");
    assert_eq!(
        command.args(),
        ["--project", "/tmp/demo project", "$(rm -rf /)"]
    );
    assert_eq!(
        command.env().get("LABEL").map(String::as_str),
        Some("Fix login")
    );
    assert_eq!(command.terminal_title(), Some("Fix login"));
}

#[test]
fn unknown_placeholder_is_rejected() {
    let temp = TempDir::new().expect("temporary directory should be created");
    executable(temp.path(), "codex");
    let context = context(&temp)
        .with_extra_args(["${HOME}"])
        .expect("args stored")
        .with_placeholders(PlaceholderContext::new());
    let error = cli_master_agents::CodexAdapter
        .build_command(&context)
        .expect_err("unknown placeholder should fail");
    assert!(error.to_string().contains("HOME"));
}

#[test]
fn env_removals_and_startup_input_are_structured() {
    let temp = TempDir::new().expect("temporary directory should be created");
    executable(temp.path(), "codex");
    let context = LaunchContext::new(temp.path(), isolated_env(&temp))
        .with_env_removals(["STALE_TOKEN"])
        .expect("removals")
        .with_startup_input("ready\n")
        .expect("startup");
    let command = cli_master_agents::CodexAdapter
        .build_command(&context)
        .expect("command should build");
    assert_eq!(command.env_removals(), ["STALE_TOKEN"]);
    assert_eq!(command.startup_input(), Some("ready\n"));
    assert!(!format!("{command:?}").contains("ready"));
}
