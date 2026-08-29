//! Deterministic coverage for session event replay and backpressure.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::thread;

use cli_master_core::wire::OutputCursor;
use cli_master_core::wire::event_name;
use cli_master_core::{SessionId, SessionStatus};
use cli_master_daemon::{DiagnosticLog, EventBus, EventBusLimits, FanoutEvent};

fn tiny_bus() -> EventBus {
    EventBus::with_limits(
        EventBusLimits {
            replay_max_bytes: 8,
            replay_max_chunks: 2,
            client_queue_capacity: 2,
        },
        DiagnosticLog::default(),
    )
}

fn names(events: &[FanoutEvent]) -> Vec<&str> {
    events.iter().map(|event| event.name.as_str()).collect()
}

fn output_sequences(events: &[FanoutEvent]) -> Vec<u64> {
    events
        .iter()
        .filter(|event| event.name == event_name::SESSION_OUTPUT)
        .map(|event| event.output_sequence().expect("output sequence"))
        .collect()
}

#[test]
fn replay_starts_after_accepted_cursor_without_duplicates() {
    let bus = EventBus::new(DiagnosticLog::default());
    let session = SessionId::new();
    bus.open_session(session);
    bus.publish_output(session, b"a");
    bus.publish_output(session, b"b");
    bus.publish_output(session, b"c");

    let client = bus.connect_client();
    let first = bus.subscribe(&client, session, None).expect("subscribe");
    assert_eq!(output_sequences(&first.replay), [1, 2, 3]);
    assert!(
        first
            .replay
            .iter()
            .all(|event| { event.name != event_name::SESSION_OUTPUT || event.is_replay_output() })
    );
    assert_eq!(
        first.replay.last().map(|event| event.name.as_str()),
        Some(event_name::SESSION_REPLAY_COMPLETE)
    );

    bus.unsubscribe(client.id, session);
    let again = bus
        .subscribe(&client, session, Some(OutputCursor::new(3)))
        .expect("reconnect");
    assert_eq!(output_sequences(&again.replay), [] as [u64; 0]);
    bus.publish_output(session, b"d");
    let live = client.drain();
    assert_eq!(output_sequences(&live), [4]);
    assert!(live.iter().all(|event| !event.is_replay_output()));
}

#[test]
fn invalid_or_expired_cursor_returns_gap_without_live_attach() {
    let bus = tiny_bus();
    let session = SessionId::new();
    bus.open_session(session);
    bus.publish_output(session, b"a");
    bus.publish_output(session, b"b");
    bus.publish_output(session, b"c");
    bus.publish_output(session, b"d");

    let client = bus.connect_client();
    let future = bus
        .subscribe(&client, session, Some(OutputCursor::new(99)))
        .expect("future cursor");
    assert_eq!(names(&future.replay), [event_name::SESSION_OUTPUT_GAP]);
    assert!(!future.attaches_live);
    assert_eq!(future.replay[0].payload["requestedCursor"], 99);

    let hole = bus
        .subscribe(&client, session, Some(OutputCursor::new(1)))
        .expect("dropped cursor");
    assert_eq!(names(&hole.replay), [event_name::SESSION_OUTPUT_GAP]);
    assert!(!hole.attaches_live);
    assert_eq!(hole.replay[0].payload["firstAvailableSequence"], 3);
    assert_eq!(hole.replay[0].payload["latestSequence"], 4);
}

#[test]
fn overflow_retains_latest_chunks_by_count_and_bytes() {
    let bus = tiny_bus();
    let session = SessionId::new();
    bus.open_session(session);
    bus.publish_output(session, b"w");
    bus.publish_output(session, b"x");
    bus.publish_output(session, b"y");

    let client = bus.connect_client();
    let outcome = bus
        .subscribe(&client, session, None)
        .expect("retained start");
    assert_eq!(output_sequences(&outcome.replay), [2, 3]);
}

#[test]
fn slow_client_gets_a_gap_without_blocking_a_fast_client() {
    let bus = tiny_bus();
    let session = SessionId::new();
    bus.open_session(session);
    let slow = bus.connect_client();
    let fast = bus.connect_client();
    assert!(
        bus.subscribe(&slow, session, None)
            .expect("slow")
            .attaches_live
    );
    assert!(
        bus.subscribe(&fast, session, None)
            .expect("fast")
            .attaches_live
    );

    for byte in [b"1", b"2", b"3", b"4", b"5"] {
        bus.publish_output(session, byte);
        let _ = fast.drain();
    }

    let slow_events = slow.drain();
    assert!(
        slow_events
            .iter()
            .any(|event| event.name == event_name::SESSION_OUTPUT_GAP),
        "slow client must observe an explicit gap, got {slow_events:?}"
    );
    assert!(
        !bus.diagnostics()
            .recent()
            .iter()
            .any(|issue| issue.message.contains('=') || issue.message.contains("PATH")),
        "diagnostics must stay free of env and payload bytes"
    );

    bus.publish_output(session, b"z");
    assert_eq!(output_sequences(&fast.drain()), [6]);
}

#[test]
fn concurrent_publishers_assign_monotonic_unique_output_sequences() {
    let bus = Arc::new(EventBus::with_limits(
        EventBusLimits {
            replay_max_bytes: 1024,
            replay_max_chunks: 256,
            client_queue_capacity: 256,
        },
        DiagnosticLog::default(),
    ));
    let session = SessionId::new();
    bus.open_session(session);

    let mut joins = Vec::new();
    for _ in 0..4 {
        let bus = Arc::clone(&bus);
        joins.push(thread::spawn(move || {
            for _ in 0..25 {
                bus.publish_output(session, b"x");
            }
        }));
    }
    for join in joins {
        join.join().expect("publisher thread");
    }

    let client = bus.connect_client();
    let replay = bus
        .subscribe(&client, session, None)
        .expect("replay")
        .replay;
    let sequences = output_sequences(&replay);
    let unique: BTreeSet<_> = sequences.iter().copied().collect();
    assert_eq!(sequences.len(), 100);
    assert_eq!(unique.len(), 100);
    assert!(sequences.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn no_output_is_emitted_after_the_terminal_exit_event() {
    let bus = EventBus::new(DiagnosticLog::default());
    let session = SessionId::new();
    bus.open_session(session);
    let client = bus.connect_client();
    bus.subscribe(&client, session, None).expect("subscribe");
    bus.publish_status(
        session,
        SessionStatus::Starting,
        SessionStatus::Running,
        1,
        None,
    );
    bus.publish_output(session, b"hello");
    bus.publish_exit(session, Some(0), SessionStatus::Exited, 2);
    bus.publish_output(session, b"late");

    let live = client.drain();
    assert_eq!(
        names(&live),
        [
            event_name::SESSION_STATUS_CHANGED,
            event_name::SESSION_OUTPUT,
            event_name::SESSION_EXITED
        ]
    );
    assert!(
        bus.diagnostics()
            .recent()
            .iter()
            .any(|issue| issue.code == "output_after_exit")
    );
}

#[test]
fn subscribe_unknown_session_is_an_error() {
    let bus = EventBus::new(DiagnosticLog::default());
    let client = bus.connect_client();
    let error = bus
        .subscribe(&client, SessionId::new(), None)
        .expect_err("missing session");
    assert_eq!(error, cli_master_daemon::SubscribeError::SessionNotFound);
}
