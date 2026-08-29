//! Real PTY lifecycle tests using ordinary Unix executables.
//!
//! These tests do not require Codex, Claude, or Gemini to be installed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use cli_master_core::wire::MAX_PTY_OUTPUT_BYTES;
use cli_master_core::{AgentId, CommandSpec, ProjectId, Session, SessionId, SessionStatus};
use cli_master_session::{
    CreateSession, SessionError, SessionEvent, SessionManager, SessionManagerConfig,
    SessionSubscription,
};
use tempfile::TempDir;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn starts_a_shell_and_exchanges_input_output() {
    let fixture = Fixture::new();
    let session = fixture
        .create(command(&fixture, "sh", &[]))
        .expect("shell should start");
    let mut subscription = fixture
        .manager
        .subscribe(session.id)
        .expect("subscribe should succeed");

    fixture
        .manager
        .write(session.id, b"printf 'hello-shell \xCF\x80\\n'\n")
        .expect("input should write");

    let mut output = Vec::new();
    wait_for_bytes(&mut subscription, &mut output, b"hello-shell").await;
    wait_for_bytes(&mut subscription, &mut output, "π".as_bytes()).await;
    assert!(
        fixture
            .manager
            .get(session.id)
            .expect("session should remain available")
            .last_activity_at_ms
            .is_some()
    );
    fixture.manager.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runs_an_interactive_command_then_ctrl_d() {
    let fixture = Fixture::new();
    let session = fixture
        .create(command(&fixture, "cat", &[]))
        .expect("cat should start");
    let mut subscription = fixture
        .manager
        .subscribe(session.id)
        .expect("subscribe should succeed");

    fixture
        .manager
        .write(session.id, b"ping-pong\n")
        .expect("cat input should write");
    let mut output = Vec::new();
    wait_for_bytes(&mut subscription, &mut output, b"ping-pong").await;

    fixture
        .manager
        .write(session.id, b"\x04")
        .expect("Ctrl+D should write");
    let exited = wait_status(&fixture.manager, session.id, SessionStatus::Exited).await;
    assert_eq!(exited.exit_code, Some(0));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn captures_zero_and_nonzero_exit_codes() {
    let fixture = Fixture::new();

    let zero = fixture
        .create(command(&fixture, "true", &[]))
        .expect("true should spawn");
    let zero = wait_status(&fixture.manager, zero.id, SessionStatus::Exited).await;
    assert_eq!(zero.exit_code, Some(0));

    let nonzero = fixture
        .create(command(&fixture, "false", &[]))
        .expect("false should spawn");
    let nonzero = wait_status(&fixture.manager, nonzero.id, SessionStatus::Failed).await;
    assert_eq!(nonzero.exit_code, Some(1));
    assert_eq!(
        nonzero.error_code.as_deref(),
        Some("session_process_failed")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn interrupts_a_process_with_ctrl_c() {
    let fixture = Fixture::new();
    let session = fixture
        .create(command(&fixture, "sleep", &["30"]))
        .expect("sleep should start");
    wait_live(&fixture.manager, session.id).await;

    fixture
        .manager
        .write(session.id, b"\x03")
        .expect("Ctrl+C should write");
    let failed = wait_status(&fixture.manager, session.id, SessionStatus::Failed).await;
    assert_ne!(failed.exit_code, Some(0));
    assert_eq!(failed.error_code.as_deref(), Some("session_process_failed"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resizes_the_pty() {
    let fixture = Fixture::new();
    let session = fixture
        .create(command(&fixture, "sh", &[]))
        .expect("resize helper should start");
    let mut subscription = fixture
        .manager
        .subscribe(session.id)
        .expect("subscribe should succeed");
    fixture
        .manager
        .write(
            session.id,
            b"printf 'READY\\n'; while true; do stty size; sleep 1; done\n",
        )
        .expect("resize script should write");
    let mut output = Vec::new();
    wait_for_bytes(&mut subscription, &mut output, b"READY").await;

    fixture
        .manager
        .resize(session.id, 40, 12)
        .expect("resize should succeed");
    wait_for_bytes(&mut subscription, &mut output, b"12 40").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rejects_duplicate_start_and_keeps_other_sessions() {
    let fixture = Fixture::new();
    let first = fixture
        .create(command(&fixture, "sleep", &["30"]))
        .expect("first session should start");
    wait_live(&fixture.manager, first.id).await;

    let duplicate = fixture.manager.start(first.id);
    assert!(matches!(duplicate, Err(SessionError::AlreadyRunning(_))));

    let mut others = Vec::new();
    for _ in 0..9 {
        let session = fixture
            .create(command(&fixture, "sleep", &["30"]))
            .expect("additional session should start");
        wait_live(&fixture.manager, session.id).await;
        others.push(session.id);
    }
    assert_eq!(fixture.manager.live_count(), 10);

    fixture
        .manager
        .stop(first.id)
        .await
        .expect("stop should succeed");
    wait_status(&fixture.manager, first.id, SessionStatus::Exited).await;

    for id in others {
        let session = fixture
            .manager
            .get(id)
            .expect("other session should remain");
        assert!(
            session.status.is_live(),
            "stopping one session must not stop another: {session:?}"
        );
    }

    fixture.manager.shutdown().await;
    assert_eq!(fixture.manager.live_count(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn spawn_failure_marks_the_session_failed() {
    let fixture = Fixture::new();
    let command = CommandSpec::try_from_parts(
        "/no/such/cli-master-agent",
        Vec::<String>::new(),
        fixture.cwd.clone(),
        BTreeMap::new(),
    )
    .expect("command spec should accept a missing path");
    let error = fixture
        .create(command)
        .expect_err("missing executable should fail to spawn");
    assert!(matches!(error, SessionError::Spawn(_)));

    let sessions = fixture.manager.list();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].status, SessionStatus::Failed);
    assert_eq!(sessions[0].pid, None);
    assert_eq!(
        sessions[0].error_code.as_deref(),
        Some("session_spawn_failed")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconnects_to_bounded_output_without_killing_the_session() {
    let mut config = SessionManagerConfig::for_tests();
    config.replay_buffer_bytes = 2048;
    let fixture = Fixture::with_config(config);
    let session = fixture
        .create(command(&fixture, "sh", &[]))
        .expect("session should start");

    let mut first = fixture
        .manager
        .subscribe(session.id)
        .expect("first subscribe should succeed");
    fixture
        .manager
        .write(session.id, b"printf 'FIRST-LINE\\n'; exec sleep 30\n")
        .expect("reconnect fixture command should write");
    let mut output = Vec::new();
    wait_for_bytes(&mut first, &mut output, b"FIRST-LINE").await;
    drop(first);

    fixture
        .manager
        .write(session.id, b"")
        .expect("empty write is a no-op");
    assert!(
        fixture
            .manager
            .get(session.id)
            .expect("session remains after unsubscribe")
            .status
            .is_live()
    );

    let second = fixture
        .manager
        .subscribe(session.id)
        .expect("reconnect subscribe should succeed");
    let replay = second.snapshot.concatenated();
    assert!(
        replay
            .windows(b"FIRST-LINE".len())
            .any(|window| window == b"FIRST-LINE"),
        "reconnect snapshot should contain recent output"
    );

    fixture.manager.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn replay_buffer_does_not_grow_without_bound() {
    let mut config = SessionManagerConfig::for_tests();
    config.replay_buffer_bytes = 4096;
    let fixture = Fixture::with_config(config);
    let mut events = fixture.manager.subscribe_events();
    let session = fixture
        .create(command(
            &fixture,
            "dd",
            &["if=/dev/zero", "bs=1024", "count=64"],
        ))
        .expect("dd should start");
    wait_status(&fixture.manager, session.id, SessionStatus::Exited).await;

    let mut output_events = 0;
    while let Ok(event) = events.try_recv() {
        if let SessionEvent::Output { chunk, .. } = event {
            output_events += 1;
            assert!(chunk.data.len() <= MAX_PTY_OUTPUT_BYTES);
        }
    }
    assert!(
        output_events > 0,
        "dd should produce at least one output event"
    );

    let snapshot = fixture
        .manager
        .subscribe(session.id)
        .expect("subscribe after exit")
        .snapshot;
    assert!(snapshot.truncated);
    assert!(snapshot.concatenated().len() <= 4096);
    assert!(
        snapshot
            .chunks
            .iter()
            .all(|chunk| chunk.data.len() <= MAX_PTY_OUTPUT_BYTES)
    );

    fixture
        .manager
        .delete(session.id)
        .expect("exited session should delete");
    assert!(fixture.manager.get(session.id).is_none());
}

struct Fixture {
    _tempdir: TempDir,
    cwd: PathBuf,
    manager: SessionManager,
}

impl Fixture {
    fn new() -> Self {
        Self::with_config(SessionManagerConfig::for_tests())
    }

    fn with_config(config: SessionManagerConfig) -> Self {
        let tempdir = TempDir::new().expect("temporary directory");
        let cwd = tempdir.path().to_path_buf();
        let manager = SessionManager::new(config);
        Self {
            _tempdir: tempdir,
            cwd,
            manager,
        }
    }

    fn create(&self, command: CommandSpec) -> Result<Session, SessionError> {
        self.manager.create(CreateSession {
            project_id: ProjectId::new(),
            agent_id: AgentId::new(),
            name: "test".to_owned(),
            command,
            cols: 80,
            rows: 24,
        })
    }
}

fn command(fixture: &Fixture, name: &str, args: &[&str]) -> CommandSpec {
    CommandSpec::try_from_parts(
        which(name).to_string_lossy().into_owned(),
        args.iter().map(ToString::to_string),
        fixture.cwd.clone(),
        BTreeMap::new(),
    )
    .expect("test command should be valid")
}

fn which(name: &str) -> PathBuf {
    ["/usr/bin", "/bin", "/usr/sbin", "/sbin"]
        .into_iter()
        .map(|directory| Path::new(directory).join(name))
        .find(|path| path.is_file())
        .unwrap_or_else(|| panic!("{name} not found"))
}

async fn wait_live(manager: &SessionManager, id: SessionId) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    loop {
        if let Some(session) = manager.get(id) {
            if session.status.is_live() {
                return;
            }
            assert!(
                !matches!(
                    session.status,
                    SessionStatus::Failed | SessionStatus::Exited
                ),
                "session left live states: {session:?}"
            );
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for live session {id}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn wait_status(manager: &SessionManager, id: SessionId, expected: SessionStatus) -> Session {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    loop {
        if let Some(session) = manager.get(id) {
            if session.status == expected {
                return session;
            }
            assert!(
                session.status != SessionStatus::Failed || expected == SessionStatus::Failed,
                "session failed: {session:?}"
            );
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {expected:?}, last={:?}",
            manager.get(id)
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn wait_for_bytes(
    subscription: &mut SessionSubscription,
    collected: &mut Vec<u8>,
    needle: &[u8],
) {
    if collected.is_empty() {
        collected.extend(subscription.snapshot.concatenated());
    }
    if contains(collected, needle) {
        return;
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        assert!(
            !remaining.is_zero(),
            "timed out waiting for {}, got {}",
            String::from_utf8_lossy(needle),
            String::from_utf8_lossy(collected)
        );
        match tokio::time::timeout(remaining, subscription.next_chunk()).await {
            Ok(Ok(chunk)) => {
                collected.extend_from_slice(&chunk.data);
                if contains(collected, needle) {
                    return;
                }
            }
            Ok(Err(error)) => panic!("subscription ended while waiting for output: {error}"),
            Err(elapsed) => panic!(
                "timed out waiting for {}, got {} after {elapsed:?}",
                String::from_utf8_lossy(needle),
                String::from_utf8_lossy(collected)
            ),
        }
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
