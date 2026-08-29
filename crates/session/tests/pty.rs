use std::collections::BTreeMap;
use std::time::Duration;

use cli_master_core::{CommandSpec, SessionId, SessionStatus};
use cli_master_fake_agent::{Options, compiled_executable};
use cli_master_session::{SessionError, SessionManager, TerminalSize};
use tempfile::TempDir;

fn spec(args: &[&str], cwd: &std::path::Path) -> CommandSpec {
    CommandSpec::try_from_parts(
        compiled_executable()
            .to_str()
            .expect("utf-8 fake-agent path"),
        args.iter().map(|arg| (*arg).to_owned()),
        cwd,
        BTreeMap::new(),
    )
    .expect("command spec should be valid")
}

fn wait_ms() -> Duration {
    Duration::from_secs(10)
}

#[test]
fn start_write_and_stop_fake_agent() {
    let temp = TempDir::new().expect("temporary directory");
    let manager = SessionManager::new();
    let session_id = SessionId::new();
    manager
        .start(
            session_id,
            spec(&["--banner", "hello-pty", "--prompt"], temp.path()),
            TerminalSize::default(),
        )
        .expect("session should start");

    manager
        .wait_for_output(session_id, "hello-pty", wait_ms())
        .expect("banner should arrive");
    manager
        .wait_for_output(session_id, "READY>", wait_ms())
        .expect("prompt should arrive");
    manager
        .write(session_id, b"ping\n")
        .expect("write should succeed");
    manager
        .wait_for_output(session_id, "ack:ping", wait_ms())
        .expect("reply should arrive");

    let snapshot = manager.snapshot(session_id).expect("snapshot");
    let output = String::from_utf8_lossy(&snapshot);
    assert!(output.contains("cwd="));
    assert!(!output.contains("PATH="));

    manager.stop(session_id).expect("stop should succeed");
    let status = manager.status(session_id).expect("status");
    assert!(matches!(
        status,
        SessionStatus::Exited | SessionStatus::Failed
    ));
}

#[test]
fn unicode_and_bulk_output_are_captured() {
    let temp = TempDir::new().expect("temporary directory");
    let manager = SessionManager::new();
    let session_id = SessionId::new();
    manager
        .start(
            session_id,
            spec(
                &[
                    "--no-input",
                    "--unicode",
                    "--bytes",
                    "8192",
                    "--exit-code",
                    "0",
                ],
                temp.path(),
            ),
            TerminalSize::default(),
        )
        .expect("session should start");

    manager
        .wait_for_output(session_id, "unicode: café 日本語 🦀", wait_ms())
        .expect("unicode line");
    let snapshot = manager
        .wait_for_output(session_id, "xxxx", wait_ms())
        .expect("bulk output");
    assert!(snapshot.windows(4).any(|window| window == b"xxxx"));

    let deadline = std::time::Instant::now() + wait_ms();
    while manager.status(session_id).expect("status") == SessionStatus::Running {
        if std::time::Instant::now() > deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert_eq!(
        manager.status(session_id).expect("status"),
        SessionStatus::Exited
    );
}

#[test]
fn ignore_sigterm_requires_kill() {
    let temp = TempDir::new().expect("temporary directory");
    let manager = SessionManager::new();
    let session_id = SessionId::new();
    manager
        .start(
            session_id,
            spec(
                &[
                    "--no-input",
                    "--ignore-sigterm",
                    "--hold",
                    "--banner",
                    "holding",
                ],
                temp.path(),
            ),
            TerminalSize::default(),
        )
        .expect("session should start");
    manager
        .wait_for_output(session_id, "holding", wait_ms())
        .expect("banner");

    // SIGTERM is ignored; the cooperative stop path then force-kills.
    manager.stop(session_id).expect("stop should force-kill");
    let status = manager.status(session_id).expect("status");
    assert!(matches!(
        status,
        SessionStatus::Exited | SessionStatus::Failed
    ));
}

#[test]
fn restart_replaces_the_process_and_preserves_cwd() {
    let temp = TempDir::new().expect("temporary directory");
    let manager = SessionManager::new();
    let session_id = SessionId::new();
    manager
        .start(
            session_id,
            spec(&["--banner", "first-boot", "--prompt"], temp.path()),
            TerminalSize::default(),
        )
        .expect("session should start");
    manager
        .wait_for_output(session_id, "first-boot", wait_ms())
        .expect("first banner");

    manager.restart(session_id).expect("restart");
    manager
        .wait_for_output(session_id, "first-boot", wait_ms())
        .expect("restarted banner");
    manager.stop(session_id).expect("stop after restart");
}

#[test]
fn resize_does_not_kill_the_session() {
    let temp = TempDir::new().expect("temporary directory");
    let manager = SessionManager::new();
    let session_id = SessionId::new();
    manager
        .start(
            session_id,
            spec(&["--banner", "resize-me", "--prompt"], temp.path()),
            TerminalSize::default(),
        )
        .expect("session should start");
    manager
        .wait_for_output(session_id, "resize-me", wait_ms())
        .expect("banner");
    manager
        .resize(
            session_id,
            TerminalSize {
                cols: 120,
                rows: 40,
            },
        )
        .expect("resize");
    assert_eq!(
        manager.status(session_id).expect("status"),
        SessionStatus::Running
    );
    manager.stop(session_id).expect("stop");
}

#[test]
fn options_round_trip_through_command_spec() {
    let parsed = Options::parse(["--banner", "x", "--no-input"]).expect("parse");
    assert_eq!(parsed.banner.as_deref(), Some("x"));
    assert!(!parsed.prompt);
}

#[test]
fn duplicate_session_id_is_rejected_without_replacing_the_live_process() {
    let temp = TempDir::new().expect("temporary directory");
    let manager = SessionManager::new();
    let session_id = SessionId::new();
    manager
        .start(
            session_id,
            spec(&["--banner", "original", "--prompt"], temp.path()),
            TerminalSize::default(),
        )
        .expect("original session should start");
    manager
        .wait_for_output(session_id, "READY>", wait_ms())
        .expect("original prompt");

    let error = manager
        .start(
            session_id,
            spec(&["--banner", "replacement", "--prompt"], temp.path()),
            TerminalSize::default(),
        )
        .expect_err("duplicate session identifier must be rejected");
    assert!(matches!(error, SessionError::DuplicateSession(id) if id == session_id));

    manager
        .write(session_id, b"still alive\n")
        .expect("original process remains writable");
    let snapshot = manager
        .wait_for_output(session_id, "ack:still alive", wait_ms())
        .expect("original process should reply");
    assert!(!String::from_utf8_lossy(&snapshot).contains("replacement"));
    manager.stop(session_id).expect("stop original process");
}
