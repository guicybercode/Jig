use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use cli_master_core::{APPLICATION_VERSION, PROTOCOL_V1};
use cli_master_daemon::PreflightReport;
use tempfile::TempDir;

fn daemon_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_cli-masterd"))
}

#[test]
fn version_flag_prints_application_and_protocol() {
    let output = daemon_bin()
        .arg("--version")
        .output()
        .expect("cli-masterd --version should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(APPLICATION_VERSION));
    assert!(stdout.contains(&format!("protocol {PROTOCOL_V1}")));
}

#[test]
fn preflight_creates_private_directories_and_requires_git() {
    let temporary = TempDir::new().expect("temporary directory should exist");
    let home = temporary.path();
    let output = daemon_bin()
        .arg("--preflight")
        .env("HOME", home)
        .env("XDG_DATA_HOME", home.join("share"))
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_RUNTIME_DIR", home.join("run"))
        .output()
        .expect("cli-masterd --preflight should run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: PreflightReport =
        serde_json::from_slice(&output.stdout).expect("preflight should emit JSON");
    assert!(report.ok);
    assert!(report.git.available);
    assert_eq!(report.git.name, "git");
    assert!(report.git.required);
    assert_eq!(report.application_version, APPLICATION_VERSION);
    assert_eq!(report.protocol_version, PROTOCOL_V1);
    assert_eq!(report.agents.len(), 4);

    for directory in &report.directories {
        assert!(directory.ok, "{}", directory.kind);
        assert_eq!(directory.mode, Some(0o700));
        let mode = std::fs::metadata(&directory.path)
            .unwrap_or_else(|error| panic!("{}: {error}", directory.path.display()))
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700, "{}", directory.path.display());
    }
}

#[test]
fn unknown_argument_exits_with_usage() {
    let output = daemon_bin()
        .arg("--not-a-flag")
        .output()
        .expect("cli-masterd should run");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown argument"));
}
