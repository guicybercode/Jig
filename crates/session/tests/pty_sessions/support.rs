use std::{
    collections::BTreeMap,
    fs, thread,
    time::{Duration, Instant},
};

use cli_master_core::{CommandSpec, SessionId, SessionStatus};
use cli_master_session::{SessionEvent, SessionManager, SessionManagerConfig, TerminalSize};
use nix::{
    errno::Errno,
    sys::signal::{kill, killpg},
    unistd::Pid,
};
use tempfile::TempDir;
use tokio::sync::broadcast::error::TryRecvError;

pub(crate) const TEST_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) struct TestRuntime {
    pub manager: SessionManager,
    pub working_directory: TempDir,
}

impl TestRuntime {
    pub fn new() -> Self {
        Self::with_config(test_config())
    }

    pub fn with_config(config: SessionManagerConfig) -> Self {
        Self {
            manager: SessionManager::new(config).expect("test manager should be valid"),
            working_directory: tempfile::tempdir().expect("test directory should be created"),
        }
    }

    pub fn spawn_shell(&self, script: &str) -> SessionId {
        let command = CommandSpec::try_from_parts(
            "/bin/sh",
            ["-c", script],
            self.working_directory.path(),
            BTreeMap::new(),
        )
        .expect("shell command should be valid");
        self.manager
            .spawn(&command, TerminalSize::new(24, 80).unwrap())
            .expect("shell should start")
            .id
    }
}

impl Drop for TestRuntime {
    fn drop(&mut self) {
        let _ = self.manager.shutdown();
    }
}

pub(crate) fn test_config() -> SessionManagerConfig {
    SessionManagerConfig {
        write_timeout: Duration::from_secs(1),
        idle_after: Duration::from_secs(10),
        supervisor_interval: Duration::from_millis(20),
        interrupt_grace: Duration::from_millis(300),
        hangup_grace: Duration::from_millis(150),
        kill_wait: Duration::from_secs(1),
        output_drain_timeout: Duration::from_millis(200),
        ..SessionManagerConfig::default()
    }
}

pub(crate) fn spawn_escaped_descendant(
    manager: &SessionManager,
    working_directory: &std::path::Path,
) -> (SessionId, Pid, Pid) {
    let pid_file = working_directory.join("escaped.pid");
    let ready_file = working_directory.join("escaped.ready");
    let executable = std::env::current_exe().expect("test executable should be available");
    let command = CommandSpec::try_from_parts(
        executable.to_string_lossy().as_ref(),
        [
            "--exact",
            "lifecycle::escaped_process_parent_helper",
            "--nocapture",
            "--test-threads=1",
        ],
        working_directory,
        BTreeMap::from([
            (
                "CLI_MASTER_ESCAPED_PID_FILE".to_owned(),
                pid_file.to_string_lossy().into_owned(),
            ),
            (
                "CLI_MASTER_ESCAPED_READY_FILE".to_owned(),
                ready_file.to_string_lossy().into_owned(),
            ),
        ]),
    )
    .expect("test helper command should be valid");
    let handle = manager
        .spawn(&command, TerminalSize::default())
        .expect("test helper session should start");
    wait_for_output(
        manager,
        handle.id,
        b"escaped-descendant-ready",
        TEST_TIMEOUT,
    );
    let descendant_pid = wait_for_pid_file(&pid_file, TEST_TIMEOUT);
    let leader_pid = Pid::from_raw(i32::try_from(handle.pid.unwrap()).unwrap());
    (handle.id, leader_pid, descendant_pid)
}

pub(crate) fn wait_for_output(
    manager: &SessionManager,
    session_id: SessionId,
    needle: &[u8],
    timeout: Duration,
) -> Vec<u8> {
    let deadline = Instant::now() + timeout;
    loop {
        let output = manager
            .reconnect(session_id, 0)
            .expect("session replay should be available")
            .snapshot
            .output
            .into_iter()
            .flat_map(|chunk| chunk.bytes)
            .collect::<Vec<_>>();
        if output.windows(needle.len()).any(|part| part == needle) {
            return output;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for output (needle_len={}, output_len={}); snapshot={:?}",
            needle.len(),
            output.len(),
            manager.snapshot(session_id)
        );
        thread::sleep(Duration::from_millis(10));
    }
}

pub(crate) fn wait_for_terminal(
    manager: &SessionManager,
    session_id: SessionId,
    timeout: Duration,
) -> cli_master_session::SessionSnapshot {
    let deadline = Instant::now() + timeout;
    loop {
        let snapshot = manager.snapshot(session_id).unwrap();
        if matches!(
            snapshot.status,
            SessionStatus::Exited | SessionStatus::Failed | SessionStatus::Unknown
        ) {
            return snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for terminal state: {snapshot:?}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

pub(crate) fn wait_for_output_event(
    receiver: &mut tokio::sync::broadcast::Receiver<SessionEvent>,
    timeout: Duration,
) -> cli_master_session::OutputChunk {
    let deadline = Instant::now() + timeout;
    loop {
        match receiver.try_recv() {
            Ok(SessionEvent::Output(chunk)) => return chunk,
            Ok(_) | Err(TryRecvError::Empty | TryRecvError::Lagged(_)) => {}
            Err(TryRecvError::Closed) => panic!("session event channel closed unexpectedly"),
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for a live output event"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

pub(crate) fn wait_for_status_event(
    receiver: &mut tokio::sync::broadcast::Receiver<SessionEvent>,
    expected: SessionStatus,
    timeout: Duration,
) {
    let deadline = Instant::now() + timeout;
    loop {
        match receiver.try_recv() {
            Ok(SessionEvent::StatusChanged { current, .. }) if current == expected => return,
            Ok(_) | Err(TryRecvError::Empty | TryRecvError::Lagged(_)) => {}
            Err(TryRecvError::Closed) => panic!("session event channel closed unexpectedly"),
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for status event {expected:?}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

pub(crate) fn count_terminal_events(
    receiver: &mut tokio::sync::broadcast::Receiver<SessionEvent>,
) -> (usize, usize) {
    let mut terminal_transitions = 0;
    let mut exit_events = 0;
    loop {
        match receiver.try_recv() {
            Ok(SessionEvent::StatusChanged {
                current: SessionStatus::Exited | SessionStatus::Failed | SessionStatus::Unknown,
                ..
            }) => terminal_transitions += 1,
            Ok(SessionEvent::Exited { .. }) => exit_events += 1,
            Ok(_) | Err(TryRecvError::Lagged(_)) => {}
            Err(TryRecvError::Empty | TryRecvError::Closed) => break,
        }
    }
    (terminal_transitions, exit_events)
}

pub(crate) fn wait_for_pid_file(path: &std::path::Path, timeout: Duration) -> Pid {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(contents) = fs::read_to_string(path) {
            if let Ok(pid) = contents.parse::<i32>() {
                return Pid::from_raw(pid);
            }
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(10));
    }
}

pub(crate) fn wait_for_process_absent(pid: Pid, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        match kill(pid, None) {
            Err(Errno::ESRCH) => return,
            Ok(()) | Err(Errno::EPERM) => {}
            Err(error) => panic!("unexpected process probe failure for {pid}: {error}"),
        }
        assert!(
            Instant::now() < deadline,
            "descendant process {pid} survived session cleanup"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

pub(crate) fn wait_for_process_group_absent(process_group_id: Pid, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        match killpg(process_group_id, None) {
            Err(Errno::ESRCH) => return,
            Ok(()) | Err(Errno::EPERM) => {}
            Err(error) => {
                panic!("unexpected process-group probe failure for {process_group_id}: {error}");
            }
        }
        assert!(
            Instant::now() < deadline,
            "process group {process_group_id} survived manager drop"
        );
        thread::sleep(Duration::from_millis(10));
    }
}
