use std::{
    collections::BTreeMap,
    fs,
    io::Write as _,
    os::unix::process::CommandExt as _,
    process::Command,
    thread,
    time::{Duration, Instant},
};

use cli_master_core::{CommandSpec, SessionStatus};
use cli_master_session::{SessionManager, SessionManagerConfig, TerminalSize};
use nix::{
    errno::Errno,
    sys::signal::killpg,
    unistd::{Pid, getpgid},
};

use super::support::*;

#[test]
fn stopping_one_session_does_not_affect_another() {
    let runtime = TestRuntime::new();
    let first = runtime
        .spawn_shell("trap 'exit 0' INT; printf 'first-ready\\n'; while :; do sleep 1; done");
    let second = runtime.spawn_shell(
        "trap 'exit 0' INT; printf 'second-ready\\n'; \
         while IFS= read -r line; do printf 'second:%s\\n' \"$line\"; done",
    );
    wait_for_output(&runtime.manager, first, b"first-ready", TEST_TIMEOUT);
    wait_for_output(&runtime.manager, second, b"second-ready", TEST_TIMEOUT);

    let stopped = runtime.manager.stop(first).unwrap();
    runtime.manager.write(second, b"still-running\n").unwrap();
    let second_output = wait_for_output(
        &runtime.manager,
        second,
        b"second:still-running",
        TEST_TIMEOUT,
    );

    assert_eq!(stopped.status, SessionStatus::Exited);
    assert!(
        second_output
            .windows(b"second:still-running".len())
            .any(|part| part == b"second:still-running")
    );
    assert!(matches!(
        runtime.manager.snapshot(second).unwrap().status,
        SessionStatus::Running | SessionStatus::Idle
    ));
}

#[test]
fn stop_force_kills_a_process_that_ignores_graceful_signals() {
    let runtime = TestRuntime::new();
    let session_id = runtime
        .spawn_shell("trap '' INT HUP; printf 'stubborn-ready\\n'; while :; do sleep 1; done");
    wait_for_output(
        &runtime.manager,
        session_id,
        b"stubborn-ready",
        TEST_TIMEOUT,
    );

    let snapshot = runtime.manager.stop(session_id).unwrap();

    assert_eq!(snapshot.status, SessionStatus::Exited);
    assert!(snapshot.exit_code.is_some());
}

#[test]
fn force_kill_reaps_without_waiting_for_the_supervisor_interval() {
    let config = SessionManagerConfig {
        supervisor_interval: Duration::from_secs(2),
        kill_wait: Duration::from_millis(50),
        ..test_config()
    };
    let runtime = TestRuntime::with_config(config);
    let session_id = runtime.spawn_shell("trap '' HUP INT; printf 'kill-ready\\n'; sleep 30");
    wait_for_output(&runtime.manager, session_id, b"kill-ready", TEST_TIMEOUT);
    let mut events = runtime.manager.reconnect(session_id, 0).unwrap().receiver;

    let started = Instant::now();
    let snapshot = runtime.manager.kill(session_id).unwrap();

    assert!(started.elapsed() < Duration::from_millis(500));
    assert_eq!(snapshot.status, SessionStatus::Exited);
    let (terminal_transitions, exit_events) = count_terminal_events(&mut events);
    assert_eq!(terminal_transitions, 1);
    assert_eq!(exit_events, 1);
}

#[test]
fn reaped_leader_does_not_leave_descendants_in_its_process_group() {
    let runtime = TestRuntime::new();
    let descendant_file = runtime.working_directory.path().join("descendant.pid");
    let ready_file = runtime.working_directory.path().join("descendant.ready");
    let command = CommandSpec::try_from_parts(
        "/bin/sh",
        [
            "-c",
            "(trap '' HUP INT TERM; printf ready > \"$DESCENDANT_READY\"; while :; do sleep 1; done) & \
             descendant=$!; while [ ! -s \"$DESCENDANT_READY\" ]; do sleep 0.01; done; \
             printf '%s' \"$descendant\" > \"$DESCENDANT_PID\"; \
             printf 'leader-ready\\n'; IFS= read -r ignored; exit 0",
        ],
        runtime.working_directory.path(),
        BTreeMap::from([
            (
                "DESCENDANT_PID".to_owned(),
                descendant_file.to_string_lossy().into_owned(),
            ),
            (
                "DESCENDANT_READY".to_owned(),
                ready_file.to_string_lossy().into_owned(),
            ),
        ]),
    )
    .unwrap();
    let handle = runtime
        .manager
        .spawn(&command, TerminalSize::default())
        .unwrap();
    let leader_pid = Pid::from_raw(i32::try_from(handle.pid.unwrap()).unwrap());
    wait_for_output(&runtime.manager, handle.id, b"leader-ready", TEST_TIMEOUT);
    // Let the supervisor record the descendant identity before the leader can
    // exit and reparent it outside the discoverable ancestry chain.
    thread::sleep(Duration::from_millis(100));
    runtime.manager.write(handle.id, b"exit\n").unwrap();

    let snapshot = wait_for_terminal(&runtime.manager, handle.id, TEST_TIMEOUT);
    let descendant_pid = wait_for_pid_file(&descendant_file, TEST_TIMEOUT);

    assert_eq!(snapshot.status, SessionStatus::Exited);
    wait_for_process_absent(descendant_pid, TEST_TIMEOUT);
    assert_eq!(killpg(leader_pid, None), Err(Errno::ESRCH));
}

#[test]
fn escaped_process_parent_helper() {
    let Ok(pid_file) = std::env::var("CLI_MASTER_ESCAPED_PID_FILE") else {
        return;
    };
    let ready_file = std::env::var("CLI_MASTER_ESCAPED_READY_FILE")
        .expect("helper ready path should accompany its pid path");
    let mut command = Command::new("/bin/sh");
    command
        .args([
            "-c",
            "trap '' HUP INT TERM; printf ready > \"$READY_FILE\"; while :; do sleep 1; done",
        ])
        .env("READY_FILE", &ready_file)
        .process_group(0);
    let mut descendant = command
        .spawn()
        .expect("escaped process-group helper should start");
    fs::write(&pid_file, descendant.id().to_string())
        .expect("escaped descendant pid should be published");
    let deadline = Instant::now() + TEST_TIMEOUT;
    while !std::path::Path::new(&ready_file).exists() {
        assert!(
            Instant::now() < deadline,
            "escaped descendant did not install its signal handlers"
        );
        thread::sleep(Duration::from_millis(5));
    }
    println!("escaped-descendant-ready");
    std::io::stdout()
        .flush()
        .expect("helper readiness should flush");
    let _ = descendant.wait();
}

#[test]
fn stop_kills_a_proven_descendant_that_changed_process_group() {
    let runtime = TestRuntime::new();
    let (session_id, leader_pid, descendant_pid) =
        spawn_escaped_descendant(&runtime.manager, runtime.working_directory.path());
    assert_ne!(leader_pid, descendant_pid);
    assert_eq!(getpgid(Some(descendant_pid)).unwrap(), descendant_pid);

    let snapshot = runtime.manager.stop(session_id).unwrap();

    assert_eq!(snapshot.status, SessionStatus::Exited);
    wait_for_process_absent(descendant_pid, TEST_TIMEOUT);
}

#[test]
fn kill_kills_a_proven_descendant_that_changed_process_group() {
    let runtime = TestRuntime::new();
    let (session_id, _, descendant_pid) =
        spawn_escaped_descendant(&runtime.manager, runtime.working_directory.path());

    let snapshot = runtime.manager.kill(session_id).unwrap();

    assert_eq!(snapshot.status, SessionStatus::Exited);
    wait_for_process_absent(descendant_pid, TEST_TIMEOUT);
}

#[test]
fn dropping_manager_kills_a_proven_descendant_that_changed_process_group() {
    let manager = SessionManager::new(test_config()).unwrap();
    let working_directory = tempfile::tempdir().unwrap();
    let (_, _, descendant_pid) = spawn_escaped_descendant(&manager, working_directory.path());

    drop(manager);

    wait_for_process_absent(descendant_pid, TEST_TIMEOUT);
}

#[test]
fn dropping_the_manager_cleans_a_live_process_group() {
    let config = SessionManagerConfig {
        interrupt_grace: Duration::from_millis(50),
        hangup_grace: Duration::from_millis(50),
        kill_wait: Duration::from_millis(500),
        ..test_config()
    };
    let manager = SessionManager::new(config).unwrap();
    let working_directory = tempfile::tempdir().unwrap();
    let command = CommandSpec::try_from_parts(
        "/bin/sh",
        [
            "-c",
            "trap '' INT HUP; printf 'drop-ready\\n'; while :; do sleep 1; done",
        ],
        working_directory.path(),
        BTreeMap::new(),
    )
    .unwrap();
    let handle = manager.spawn(&command, TerminalSize::default()).unwrap();
    wait_for_output(&manager, handle.id, b"drop-ready", TEST_TIMEOUT);
    let process_group_id = Pid::from_raw(i32::try_from(handle.pid.unwrap()).unwrap());

    drop(manager);

    wait_for_process_group_absent(process_group_id, TEST_TIMEOUT);
}
