use std::{
    collections::BTreeMap,
    thread,
    time::{Duration, Instant},
};

use cli_master_core::{CommandSpec, SessionStatus};
use cli_master_session::{SessionError, SessionEvent, SessionManagerConfig, TerminalSize};
use nix::unistd::close;
use tokio::sync::broadcast::error::TryRecvError;

use super::support::*;

#[test]
fn starts_writes_and_reads_raw_terminal_output() {
    let runtime = TestRuntime::new();
    let session_id = runtime
        .spawn_shell("printf 'ready\\n'; IFS= read -r line; printf 'received:%s\\n' \"$line\"");
    wait_for_output(&runtime.manager, session_id, b"ready", TEST_TIMEOUT);

    runtime.manager.write(session_id, b"hello world\n").unwrap();
    let output = wait_for_output(
        &runtime.manager,
        session_id,
        b"received:hello world",
        TEST_TIMEOUT,
    );

    assert!(
        output
            .windows(b"hello world".len())
            .any(|part| part == b"hello world")
    );
    assert_eq!(
        wait_for_terminal(&runtime.manager, session_id, TEST_TIMEOUT).status,
        SessionStatus::Exited
    );
}

#[test]
fn launches_structured_arguments_without_shell_interpolation() {
    let runtime = TestRuntime::new();
    let marker = runtime.working_directory.path().join("must-not-exist");
    let literal = format!("$(touch {})", marker.display());
    let command = CommandSpec::try_from_parts(
        "/usr/bin/printf",
        ["%s\\n", literal.as_str()],
        runtime.working_directory.path(),
        BTreeMap::new(),
    )
    .unwrap();

    let session_id = runtime
        .manager
        .spawn(&command, TerminalSize::default())
        .unwrap()
        .id;
    let output = wait_for_output(
        &runtime.manager,
        session_id,
        literal.as_bytes(),
        TEST_TIMEOUT,
    );

    assert!(
        output
            .windows(literal.len())
            .any(|part| part == literal.as_bytes())
    );
    assert!(!marker.exists());
}

#[test]
fn captures_output_from_many_short_lived_processes() {
    let runtime = TestRuntime::new();
    let mut sessions = Vec::new();
    for index in 0..24 {
        let expected = format!("short-lived-{index}");
        let command = CommandSpec::try_from_parts(
            "/usr/bin/printf",
            ["%s", expected.as_str()],
            runtime.working_directory.path(),
            BTreeMap::new(),
        )
        .unwrap();
        let session_id = runtime
            .manager
            .spawn(&command, TerminalSize::default())
            .unwrap()
            .id;
        sessions.push((session_id, expected));
    }

    for (session_id, expected) in sessions {
        let output = wait_for_output(
            &runtime.manager,
            session_id,
            expected.as_bytes(),
            TEST_TIMEOUT,
        );
        assert_eq!(output, expected.as_bytes());
        assert_eq!(
            wait_for_terminal(&runtime.manager, session_id, TEST_TIMEOUT).status,
            SessionStatus::Exited
        );
    }
}

#[test]
fn inherits_the_login_environment_and_applies_structured_overrides() {
    let runtime = TestRuntime::new();
    let command = CommandSpec::try_from_parts(
        "/bin/sh",
        [
            "-c",
            "test -n \"$PATH\" && printf 'env:%s\\n' \"$SESSION_TEST_VALUE\"",
        ],
        runtime.working_directory.path(),
        BTreeMap::from([("SESSION_TEST_VALUE".to_owned(), "override".to_owned())]),
    )
    .unwrap();

    let session_id = runtime
        .manager
        .spawn(&command, TerminalSize::default())
        .unwrap()
        .id;
    let output = wait_for_output(&runtime.manager, session_id, b"env:override", TEST_TIMEOUT);

    assert!(
        output
            .windows(b"env:override".len())
            .any(|part| part == b"env:override")
    );
    assert_eq!(
        wait_for_terminal(&runtime.manager, session_id, TEST_TIMEOUT).status,
        SessionStatus::Exited
    );
}

#[test]
fn terminal_capability_defaults_respect_structured_overrides() {
    let runtime = TestRuntime::new();
    for (env, expected) in [
        (BTreeMap::new(), "terminal-env:xterm-256color:truecolor"),
        (
            BTreeMap::from([
                ("TERM".to_owned(), "explicit-term".to_owned()),
                ("COLORTERM".to_owned(), "explicit-color".to_owned()),
            ]),
            "terminal-env:explicit-term:explicit-color",
        ),
    ] {
        let command = CommandSpec::try_from_parts(
            "/bin/sh",
            ["-c", "printf 'terminal-env:%s:%s' \"$TERM\" \"$COLORTERM\""],
            runtime.working_directory.path(),
            env,
        )
        .unwrap();
        let session_id = runtime
            .manager
            .spawn(&command, TerminalSize::default())
            .unwrap()
            .id;

        wait_for_output(
            &runtime.manager,
            session_id,
            expected.as_bytes(),
            TEST_TIMEOUT,
        );
        wait_for_terminal(&runtime.manager, session_id, TEST_TIMEOUT);
    }
}

#[test]
fn preserves_ansi_escape_bytes_without_utf8_interpretation() {
    let runtime = TestRuntime::new();
    let session_id = runtime.spawn_shell(
        "printf 'raw-ready\\n'; stty raw -echo; \
         bytes=$(dd bs=1 count=3 2>/dev/null | od -An -tx1); \
         stty sane; printf '\\nraw:%s\\n' \"$bytes\"",
    );
    wait_for_output(&runtime.manager, session_id, b"raw-ready", TEST_TIMEOUT);

    runtime.manager.write(session_id, b"\x1b[A").unwrap();
    let output = wait_for_output(&runtime.manager, session_id, b"raw:", TEST_TIMEOUT);

    for encoded_byte in [b"1b".as_slice(), b"5b".as_slice(), b"41".as_slice()] {
        assert!(output.windows(2).any(|part| part == encoded_byte));
    }
}

#[test]
fn resize_reaches_the_child_terminal() {
    let runtime = TestRuntime::new();
    let session_id =
        runtime.spawn_shell("printf 'resize-ready\\n'; IFS= read -r ignored; stty size");
    wait_for_output(&runtime.manager, session_id, b"resize-ready", TEST_TIMEOUT);

    let size = TerminalSize::with_pixels(37, 119, 952, 592).unwrap();
    runtime.manager.resize(session_id, size).unwrap();
    runtime.manager.write(session_id, b"continue\n").unwrap();
    let output = wait_for_output(&runtime.manager, session_id, b"37 119", TEST_TIMEOUT);

    assert!(output.windows(6).any(|part| part == b"37 119"));
    assert_eq!(
        runtime.manager.snapshot(session_id).unwrap().terminal_size,
        size
    );
}

#[test]
fn ctrl_c_bytes_interrupt_the_foreground_process_group() {
    let runtime = TestRuntime::new();
    let session_id = runtime.spawn_shell(
        r#"trap 'printf "\ninterrupted\n"; exit 130' INT;
           printf 'interrupt-ready\n'; while :; do sleep 1; done"#,
    );
    wait_for_output(
        &runtime.manager,
        session_id,
        b"interrupt-ready",
        TEST_TIMEOUT,
    );

    runtime.manager.write(session_id, b"\x03").unwrap();
    let output = wait_for_output(&runtime.manager, session_id, b"interrupted", TEST_TIMEOUT);
    let snapshot = wait_for_terminal(&runtime.manager, session_id, TEST_TIMEOUT);

    assert!(output.windows(11).any(|part| part == b"interrupted"));
    assert_eq!(snapshot.status, SessionStatus::Failed);
    assert_eq!(snapshot.exit_code, Some(130));
}

#[test]
fn failed_exit_code_is_retained() {
    let runtime = TestRuntime::new();
    let session_id = runtime.spawn_shell("exit 7");

    let snapshot = wait_for_terminal(&runtime.manager, session_id, TEST_TIMEOUT);

    assert_eq!(snapshot.status, SessionStatus::Failed);
    assert_eq!(snapshot.exit_code, Some(7));
    assert_eq!(snapshot.pid, None);
}

#[test]
fn replay_is_bounded_and_reports_evicted_output() {
    let config = SessionManagerConfig {
        replay_max_bytes: 8,
        replay_max_chunks: 2,
        read_chunk_bytes: 4,
        ..test_config()
    };
    let runtime = TestRuntime::with_config(config);
    let session_id = runtime.spawn_shell(
        "printf 1111; sleep 0.1; printf 2222; sleep 0.1; \
         printf 3333; sleep 0.1; printf 4444",
    );
    wait_for_terminal(&runtime.manager, session_id, TEST_TIMEOUT);

    let reconnect = runtime.manager.reconnect(session_id, 0).unwrap().snapshot;
    let bytes = reconnect
        .output
        .iter()
        .flat_map(|chunk| chunk.bytes.iter().copied())
        .collect::<Vec<_>>();

    assert!(bytes.len() <= 8);
    assert!(reconnect.output.len() <= 2);
    assert!(reconnect.gap);
    assert!(reconnect.first_available_sequence.unwrap_or_default() > 1);
    assert!(bytes.ends_with(b"4444"));
}

#[test]
fn timed_out_in_flight_input_is_ambiguous_and_terminates_the_session() {
    let config = SessionManagerConfig {
        max_write_bytes: 2 * 1024 * 1024,
        write_timeout: Duration::from_millis(40),
        kill_wait: Duration::from_secs(1),
        ..test_config()
    };
    let runtime = TestRuntime::with_config(config);
    let session_id = runtime
        .spawn_shell("stty raw -echo; trap '' HUP INT; printf 'blocked-write-ready\\n'; sleep 30");
    wait_for_output(
        &runtime.manager,
        session_id,
        b"blocked-write-ready",
        TEST_TIMEOUT,
    );

    let error = runtime
        .manager
        .write(session_id, &vec![b'x'; 2 * 1024 * 1024])
        .unwrap_err();

    assert!(matches!(
        error,
        SessionError::InputDeliveryAmbiguous { session_id: actual } if actual == session_id
    ));
    assert_eq!(
        runtime.manager.snapshot(session_id).unwrap().status,
        SessionStatus::Unknown
    );
    assert_eq!(runtime.manager.snapshot(session_id).unwrap().pid, None);
    assert!(matches!(
        runtime.manager.write(session_id, b"must-not-retry"),
        Err(SessionError::NotLive { .. })
    ));
}

#[test]
fn permanent_reader_eof_invalidates_interaction_and_terminates_the_tree() {
    let runtime = TestRuntime::new();
    let executable = std::env::current_exe().expect("test executable should be available");
    let command = CommandSpec::try_from_parts(
        executable.to_string_lossy().as_ref(),
        [
            "--exact",
            "io_contracts::closed_stdio_helper",
            "--nocapture",
        ],
        runtime.working_directory.path(),
        BTreeMap::from([("CLI_MASTER_CLOSE_STDIO".to_owned(), "1".to_owned())]),
    )
    .unwrap();
    let session_id = runtime
        .manager
        .spawn(&command, TerminalSize::default())
        .unwrap()
        .id;

    let snapshot = wait_for_terminal(&runtime.manager, session_id, TEST_TIMEOUT);

    assert_eq!(snapshot.status, SessionStatus::Unknown);
    assert_eq!(snapshot.pid, None);
    assert!(matches!(
        runtime.manager.write(session_id, b"must-not-arrive"),
        Err(SessionError::NotLive { .. })
    ));
}

#[test]
fn closed_stdio_helper() {
    if std::env::var_os("CLI_MASTER_CLOSE_STDIO").is_none() {
        return;
    }
    for descriptor in [0, 1, 2] {
        let _ = close(descriptor);
    }
    thread::sleep(Duration::from_secs(30));
}

#[test]
fn no_output_event_is_published_after_a_terminal_transition() {
    let runtime = TestRuntime::new();
    let session_id = runtime.spawn_shell(
        "printf 'terminal-order-ready\\n'; IFS= read -r ignored; printf 'final-output\\n'",
    );
    wait_for_output(
        &runtime.manager,
        session_id,
        b"terminal-order-ready",
        TEST_TIMEOUT,
    );
    let mut receiver = runtime.manager.reconnect(session_id, 0).unwrap().receiver;
    runtime.manager.write(session_id, b"finish\n").unwrap();

    let deadline = Instant::now() + TEST_TIMEOUT;
    let mut terminal_seen = false;
    let mut exited_seen = false;
    while !exited_seen {
        match receiver.try_recv() {
            Ok(SessionEvent::Output(_)) if terminal_seen => {
                panic!("output was published after the terminal transition")
            }
            Ok(SessionEvent::StatusChanged {
                current: SessionStatus::Exited | SessionStatus::Failed | SessionStatus::Unknown,
                ..
            }) => terminal_seen = true,
            Ok(SessionEvent::Exited { .. }) => exited_seen = true,
            Ok(_) | Err(TryRecvError::Empty | TryRecvError::Lagged(_)) => {}
            Err(TryRecvError::Closed) => panic!("session event channel closed unexpectedly"),
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for the terminal event"
        );
        thread::sleep(Duration::from_millis(5));
    }
    thread::sleep(test_config().output_drain_timeout);
    while let Ok(event) = receiver.try_recv() {
        assert!(
            !matches!(event, SessionEvent::Output(_)),
            "output was published after the terminal event: {event:?}"
        );
    }
}

#[test]
fn reconnect_registers_live_events_and_replays_ordered_bytes() {
    let runtime = TestRuntime::new();
    let session_id = runtime
        .spawn_shell("printf 'event-ready\\n'; IFS= read -r line; printf 'event:%s\\n' \"$line\"");
    wait_for_output(&runtime.manager, session_id, b"event-ready", TEST_TIMEOUT);
    let mut subscription = runtime.manager.reconnect(session_id, 0).unwrap();
    let prior_next_sequence = subscription.snapshot.next_sequence;

    runtime.manager.write(session_id, b"payload\n").unwrap();
    let event = wait_for_output_event(&mut subscription.receiver, TEST_TIMEOUT);

    assert!(event.sequence >= prior_next_sequence);
    assert_eq!(event.session_id, session_id);
    let replay = runtime.manager.reconnect(session_id, 0).unwrap().snapshot;
    assert!(
        replay
            .output
            .windows(2)
            .all(|pair| pair[0].sequence < pair[1].sequence)
    );
}

#[test]
fn idle_session_returns_to_running_after_input() {
    let config = SessionManagerConfig {
        idle_after: Duration::from_millis(100),
        ..test_config()
    };
    let runtime = TestRuntime::with_config(config);
    let session_id = runtime
        .spawn_shell("printf 'idle-ready\\n'; IFS= read -r line; printf 'awake:%s\\n' \"$line\"");
    wait_for_output(&runtime.manager, session_id, b"idle-ready", TEST_TIMEOUT);
    let mut receiver = runtime.manager.reconnect(session_id, 0).unwrap().receiver;
    wait_for_status_event(&mut receiver, SessionStatus::Idle, TEST_TIMEOUT);
    assert_eq!(
        runtime.manager.snapshot(session_id).unwrap().status,
        SessionStatus::Idle
    );

    runtime.manager.write(session_id, b"now\n").unwrap();
    wait_for_status_event(&mut receiver, SessionStatus::Running, TEST_TIMEOUT);

    assert_eq!(
        runtime.manager.snapshot(session_id).unwrap().status,
        SessionStatus::Running
    );
}
