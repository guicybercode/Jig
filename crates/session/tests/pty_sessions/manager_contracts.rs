use std::{
    collections::BTreeMap,
    sync::{Arc, Barrier},
    thread,
    time::{Duration, Instant},
};

use cli_master_core::{CommandSpec, SessionId, SessionStatus};
use cli_master_session::{SessionError, SessionManager, TerminalSize};

use super::support::*;

#[test]
fn shutdown_closes_the_spawn_gate_before_cleanup() {
    let runtime = TestRuntime::new();
    runtime.manager.shutdown().unwrap();
    let marker = runtime
        .working_directory
        .path()
        .join("spawned-after-shutdown");
    let command = CommandSpec::try_from_parts(
        "/usr/bin/touch",
        [marker.to_string_lossy().as_ref()],
        runtime.working_directory.path(),
        BTreeMap::new(),
    )
    .unwrap();

    let result = runtime.manager.spawn(&command, TerminalSize::default());

    assert!(matches!(result, Err(SessionError::ManagerShuttingDown)));
    assert!(!marker.exists());
}

#[test]
fn concurrent_spawn_is_either_rejected_or_included_in_shutdown() {
    let manager = SessionManager::new(test_config()).unwrap();
    let working_directory = tempfile::tempdir().unwrap();
    let command = shell_script_command(
        working_directory.path(),
        BTreeMap::new(),
        "trap '' HUP INT; sleep 30",
    );
    let barrier = Arc::new(Barrier::new(3));

    let spawn_manager = manager.clone();
    let spawn_barrier = Arc::clone(&barrier);
    let spawn = thread::spawn(move || {
        spawn_barrier.wait();
        spawn_manager.spawn(&command, TerminalSize::default())
    });
    let shutdown_manager = manager.clone();
    let shutdown_barrier = Arc::clone(&barrier);
    let shutdown = thread::spawn(move || {
        shutdown_barrier.wait();
        shutdown_manager.shutdown()
    });
    barrier.wait();

    let spawned = spawn.join().expect("spawn contender should not panic");
    shutdown
        .join()
        .expect("shutdown contender should not panic")
        .unwrap();
    match spawned {
        Ok(handle) => assert!(matches!(
            manager.snapshot(handle.id).unwrap().status,
            SessionStatus::Exited | SessionStatus::Failed | SessionStatus::Unknown
        )),
        Err(SessionError::ManagerShuttingDown) => {}
        Err(error) => panic!("unexpected concurrent spawn result: {error}"),
    }
}

#[test]
fn caller_allocated_session_ids_are_unique_and_checked_before_spawn() {
    let runtime = TestRuntime::new();
    let session_id = SessionId::new();
    let first_command = shell_script_command(
        runtime.working_directory.path(),
        BTreeMap::new(),
        "printf 'first-ready\\n'; sleep 30",
    );
    let marker = runtime.working_directory.path().join("duplicate-ran");
    let second_command = CommandSpec::try_from_parts(
        "/usr/bin/touch",
        [marker.to_string_lossy().as_ref()],
        runtime.working_directory.path(),
        BTreeMap::new(),
    )
    .unwrap();
    runtime
        .manager
        .spawn_with_id(session_id, &first_command, TerminalSize::default())
        .unwrap();
    wait_for_output(&runtime.manager, session_id, b"first-ready", TEST_TIMEOUT);

    let duplicate =
        runtime
            .manager
            .spawn_with_id(session_id, &second_command, TerminalSize::default());

    assert!(matches!(
        duplicate,
        Err(SessionError::DuplicateSessionId { session_id: actual }) if actual == session_id
    ));
    assert!(!marker.exists());
    runtime.manager.kill(session_id).unwrap();
}

#[test]
fn repeated_spawn_failures_join_the_preattached_reader() {
    let runtime = TestRuntime::new();
    let command = CommandSpec::new(
        "/cli-master-test/executable-does-not-exist",
        runtime.working_directory.path(),
    )
    .unwrap();
    let started = Instant::now();

    for _ in 0..32 {
        assert!(matches!(
            runtime.manager.spawn(&command, TerminalSize::default()),
            Err(SessionError::Spawn { .. })
        ));
    }

    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(runtime.manager.list().is_empty());
}

#[test]
fn validates_working_directory_and_input_bounds() {
    let runtime = TestRuntime::new();
    let missing = runtime.working_directory.path().join("missing");
    let command = CommandSpec::new("/bin/sh", &missing).unwrap();
    assert!(matches!(
        runtime
            .manager
            .spawn(&command, TerminalSize::default()),
        Err(SessionError::WorkingDirectoryUnavailable { path }) if path == missing
    ));

    let session_id = runtime.spawn_shell("IFS= read -r line");
    let oversized = vec![b'x'; test_config().max_write_bytes + 1];
    assert!(matches!(
        runtime.manager.write(session_id, &oversized),
        Err(SessionError::InputTooLarge { .. })
    ));
}

#[test]
fn live_sessions_cannot_be_removed_and_unknown_ids_are_explicit() {
    let runtime = TestRuntime::new();
    let session_id = runtime.spawn_shell("while :; do sleep 1; done");

    assert!(matches!(
        runtime.manager.remove(session_id),
        Err(SessionError::RemoveLive { .. })
    ));
    assert!(matches!(
        runtime.manager.snapshot(SessionId::new()),
        Err(SessionError::NotFound { .. })
    ));

    runtime.manager.kill(session_id).unwrap();
    runtime.manager.remove(session_id).unwrap();
    assert!(runtime.manager.list().is_empty());
}
