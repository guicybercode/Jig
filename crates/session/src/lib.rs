//! PTY session manager. Process lifetime is independent of any UI client.

#![warn(missing_docs)]

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use cli_master_core::{CommandSpec, SessionId};
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};

const REPLAY_LIMIT: usize = 8 * 1024 * 1024;
const READ_CHUNK: usize = 32 * 1024;
const BATCH_WINDOW: Duration = Duration::from_millis(8);
const STOP_GRACE: Duration = Duration::from_millis(400);

/// Failure while starting or controlling a live PTY session.
#[derive(Debug)]
pub enum SessionError {
    /// The session is already starting or running.
    AlreadyRunning,
    /// No live PTY exists for the session.
    NotRunning,
    /// The PTY or child process could not be created.
    Spawn(String),
    /// Writing to the PTY failed.
    Write(String),
    /// Resizing the PTY failed.
    Resize(String),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyRunning => formatter.write_str("session is already running"),
            Self::NotRunning => formatter.write_str("session is not running"),
            Self::Spawn(message) | Self::Write(message) | Self::Resize(message) => {
                formatter.write_str(message)
            }
        }
    }
}

impl std::error::Error for SessionError {}

/// Events delivered to PTY subscribers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PtyEvent {
    /// Ordered output bytes.
    Output {
        /// Per-session sequence.
        sequence: u64,
        /// Raw PTY bytes.
        bytes: Vec<u8>,
    },
    /// The process exited.
    Exited {
        /// OS exit code, when available.
        exit_code: Option<i32>,
    },
}

/// Snapshot returned to a new subscriber.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplaySnapshot {
    /// Last sequence currently buffered.
    pub last_sequence: u64,
    /// Concatenated replay bytes.
    pub bytes: Vec<u8>,
}

struct LiveSession {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Box<dyn MasterPty + Send>,
    pid: i32,
    sequence: u64,
    replay: VecDeque<(u64, Vec<u8>)>,
    replay_bytes: usize,
    subscribers: Vec<Sender<PtyEvent>>,
    running: bool,
}

/// Owns live PTY masters and child process groups.
pub struct SessionManager {
    inner: Mutex<HashMap<SessionId, LiveSession>>,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    /// Creates an empty manager.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// Starts a process in a new PTY. The child is placed in its own process group.
    ///
    /// # Errors
    ///
    /// Returns an error when the session is already live or spawn fails.
    pub fn start(
        self: &Arc<Self>,
        session_id: SessionId,
        spec: &CommandSpec,
        cols: u16,
        rows: u16,
    ) -> Result<u32, SessionError> {
        {
            let mut live = self.sessions();
            if live.get(&session_id).is_some_and(|session| session.running) {
                return Err(SessionError::AlreadyRunning);
            }
            live.remove(&session_id);
        }

        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| SessionError::Spawn(error.to_string()))?;
        let portable_pty::PtyPair { master, slave } = pair;

        let mut command = CommandBuilder::new(spec.executable());
        for argument in spec.args() {
            command.arg(argument);
        }
        command.cwd(spec.cwd());
        for (key, value) in spec.env() {
            command.env(key, value);
        }

        let mut child = slave
            .spawn_command(command)
            .map_err(|error| SessionError::Spawn(error.to_string()))?;
        drop(slave);
        let pid = i32::try_from(child.process_id().unwrap_or(0))
            .map_err(|_| SessionError::Spawn("child pid is not a valid process id".to_owned()))?;
        let reader = master
            .try_clone_reader()
            .map_err(|error| SessionError::Spawn(error.to_string()))?;
        let writer = master
            .take_writer()
            .map_err(|error| SessionError::Spawn(error.to_string()))?;

        let live = LiveSession {
            writer: Arc::new(Mutex::new(writer)),
            master,
            pid,
            sequence: 0,
            replay: VecDeque::new(),
            replay_bytes: 0,
            subscribers: Vec::new(),
            running: true,
        };

        self.sessions().insert(session_id, live);

        let manager = Arc::clone(self);
        thread::Builder::new()
            .name(format!("pty-read-{session_id}"))
            .spawn(move || read_loop(&manager, session_id, reader))
            .map_err(|error| SessionError::Spawn(error.to_string()))?;

        thread::Builder::new()
            .name(format!("pty-wait-{session_id}"))
            .spawn(move || {
                let _ = child.wait();
            })
            .map_err(|error| SessionError::Spawn(error.to_string()))?;

        u32::try_from(pid).map_err(|_| SessionError::Spawn("child pid is negative".to_owned()))
    }

    /// Writes bytes to the PTY master. Ctrl+C is ordinary input (`0x03`).
    ///
    /// # Errors
    ///
    /// Returns an error when the session is not live or the write fails.
    pub fn write(&self, session_id: SessionId, bytes: &[u8]) -> Result<(), SessionError> {
        let writer = {
            let live = self.sessions();
            let session = live.get(&session_id).ok_or(SessionError::NotRunning)?;
            if !session.running {
                return Err(SessionError::NotRunning);
            }
            Arc::clone(&session.writer)
        };
        let mut guard = writer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard
            .write_all(bytes)
            .and_then(|()| guard.flush())
            .map_err(|error| SessionError::Write(error.to_string()))
    }

    /// Resizes the PTY.
    ///
    /// # Errors
    ///
    /// Returns an error when the session is not live or resize fails.
    pub fn resize(&self, session_id: SessionId, cols: u16, rows: u16) -> Result<(), SessionError> {
        let live = self.sessions();
        let session = live.get(&session_id).ok_or(SessionError::NotRunning)?;
        if !session.running {
            return Err(SessionError::NotRunning);
        }
        session
            .master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| SessionError::Resize(error.to_string()))
    }

    /// Sends SIGTERM to the process group, then SIGKILL after a short grace period.
    ///
    /// # Errors
    ///
    /// Returns an error when the session is not live.
    pub fn stop(&self, session_id: SessionId) -> Result<(), SessionError> {
        self.signal(session_id, Signal::SIGTERM)?;
        let manager_pid = {
            let live = self.sessions();
            live.get(&session_id).map(|session| session.pid)
        };
        thread::sleep(STOP_GRACE);
        if let Some(pid) = manager_pid {
            if self.sessions().get(&session_id).is_some_and(|s| s.running) {
                let _ = kill(Pid::from_raw(-pid), Signal::SIGKILL);
                let _ = kill(Pid::from_raw(pid), Signal::SIGKILL);
            }
        }
        Ok(())
    }

    /// Force-kills the process group immediately.
    ///
    /// # Errors
    ///
    /// Returns an error when the session is not live.
    pub fn kill(&self, session_id: SessionId) -> Result<(), SessionError> {
        self.signal(session_id, Signal::SIGKILL)
    }

    /// Subscribes to replay bytes plus live events.
    ///
    /// # Errors
    ///
    /// Returns an error when the session is not live.
    pub fn subscribe(
        &self,
        session_id: SessionId,
    ) -> Result<(ReplaySnapshot, Receiver<PtyEvent>), SessionError> {
        let (sender, receiver) = mpsc::channel();
        let mut live = self.sessions();
        let session = live.get_mut(&session_id).ok_or(SessionError::NotRunning)?;
        let mut bytes = Vec::with_capacity(session.replay_bytes);
        for (_, chunk) in &session.replay {
            bytes.extend_from_slice(chunk);
        }
        let snapshot = ReplaySnapshot {
            last_sequence: session.sequence,
            bytes,
        };
        session.subscribers.push(sender);
        Ok((snapshot, receiver))
    }

    /// Returns whether a live PTY currently exists.
    #[must_use]
    pub fn is_live(&self, session_id: SessionId) -> bool {
        self.sessions()
            .get(&session_id)
            .is_some_and(|session| session.running)
    }

    /// Returns the number of live PTYs.
    #[must_use]
    pub fn live_count(&self) -> usize {
        self.sessions()
            .values()
            .filter(|session| session.running)
            .count()
    }

    fn sessions(&self) -> std::sync::MutexGuard<'_, HashMap<SessionId, LiveSession>> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn signal(&self, session_id: SessionId, signal: Signal) -> Result<(), SessionError> {
        let live = self.sessions();
        let session = live.get(&session_id).ok_or(SessionError::NotRunning)?;
        if !session.running {
            return Err(SessionError::NotRunning);
        }
        let pid = session.pid;
        drop(live);
        if pid > 0 {
            let _ = kill(Pid::from_raw(-pid), signal);
            let _ = kill(Pid::from_raw(pid), signal);
        }
        Ok(())
    }

    fn push_output(&self, session_id: SessionId, bytes: &[u8]) {
        let mut live = self.sessions();
        let Some(session) = live.get_mut(&session_id) else {
            return;
        };
        session.sequence += 1;
        let sequence = session.sequence;
        session.replay_bytes += bytes.len();
        session.replay.push_back((sequence, bytes.to_vec()));
        while session.replay_bytes > REPLAY_LIMIT {
            if let Some((_, removed)) = session.replay.pop_front() {
                session.replay_bytes = session.replay_bytes.saturating_sub(removed.len());
            } else {
                break;
            }
        }
        session.subscribers.retain(|subscriber| {
            subscriber
                .send(PtyEvent::Output {
                    sequence,
                    bytes: bytes.to_vec(),
                })
                .is_ok()
        });
    }

    fn finish(&self, session_id: SessionId, exit_code: Option<i32>) {
        let mut live = self.sessions();
        let Some(session) = live.get_mut(&session_id) else {
            return;
        };
        session.running = false;
        session
            .subscribers
            .retain(|subscriber| subscriber.send(PtyEvent::Exited { exit_code }).is_ok());
    }
}

fn read_loop(manager: &SessionManager, session_id: SessionId, mut reader: Box<dyn Read + Send>) {
    let mut buffer = vec![0_u8; READ_CHUNK];
    let mut pending = Vec::new();
    let mut last_flush = Instant::now();
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => {
                if !pending.is_empty() {
                    manager.push_output(session_id, &pending);
                }
                manager.finish(session_id, None);
                break;
            }
            Ok(read) => {
                pending.extend_from_slice(&buffer[..read]);
                if pending.len() >= READ_CHUNK || last_flush.elapsed() >= BATCH_WINDOW {
                    manager.push_output(session_id, &pending);
                    pending.clear();
                    last_flush = Instant::now();
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
}

/// Helper used by tests to collect output until a predicate matches or a timeout elapses.
#[must_use]
pub fn wait_for_output(receiver: &Receiver<PtyEvent>, timeout: Duration) -> Vec<u8> {
    let deadline = Instant::now() + timeout;
    let mut bytes = Vec::new();
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match receiver.recv_timeout(remaining.min(Duration::from_millis(50))) {
            Ok(PtyEvent::Output { bytes: chunk, .. }) => bytes.extend_from_slice(&chunk),
            Ok(PtyEvent::Exited { .. }) => {
                if !bytes.is_empty() {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    fn cat_spec() -> CommandSpec {
        CommandSpec::try_from_parts("/bin/cat", Vec::<String>::new(), "/", BTreeMap::new())
            .expect("cat command")
    }

    fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    }

    #[test]
    fn writes_and_reads_independent_sessions() {
        let manager = Arc::new(SessionManager::new());
        let first = SessionId::new();
        let second = SessionId::new();
        let echo_spec = CommandSpec::try_from_parts(
            "/usr/bin/python3",
            [
                "-u",
                "-c",
                "import sys; sys.stdout.write('GO\\n'); sys.stdout.flush(); data=sys.stdin.read(5); sys.stdout.write(data); sys.stdout.flush()",
            ],
            "/",
            BTreeMap::new(),
        )
        .expect("python command");
        manager
            .start(first, &echo_spec, 80, 24)
            .expect("first start");
        manager
            .start(second, &cat_spec(), 80, 24)
            .expect("second start");

        std::thread::sleep(Duration::from_millis(150));
        let (snap, first_rx) = manager.subscribe(first).expect("sub1");
        assert!(
            contains_bytes(&snap.bytes, b"GO"),
            "python should print GO, got {:?}",
            snap.bytes
        );
        manager.write(first, b"alpha").expect("write first");
        let echoed = wait_for_output(&first_rx, Duration::from_secs(2));
        let mut combined = snap.bytes.clone();
        combined.extend_from_slice(&echoed);
        assert!(
            contains_bytes(&combined, b"alpha"),
            "python should echo alpha, got {combined:?}"
        );

        assert!(manager.is_live(second));
        manager.stop(first).expect("stop first");
        std::thread::sleep(Duration::from_millis(500));
        assert!(!manager.is_live(first));
        assert!(manager.is_live(second));
        manager.kill(second).expect("kill second");
    }

    #[test]
    fn child_stdout_is_readable_without_input() {
        let manager = Arc::new(SessionManager::new());
        let id = SessionId::new();
        let spec = CommandSpec::try_from_parts(
            "/bin/sh",
            ["-c", "printf 'READY\\n'"],
            "/",
            BTreeMap::new(),
        )
        .expect("shell command");
        manager.start(id, &spec, 80, 24).expect("start");
        std::thread::sleep(Duration::from_millis(100));
        let (snap, rx) = manager.subscribe(id).expect("subscribe");
        let mut output = snap.bytes;
        output.extend(wait_for_output(&rx, Duration::from_secs(2)));
        assert!(
            contains_bytes(&output, b"READY"),
            "session should emit child stdout, got {output:?}"
        );
        let _ = manager.kill(id);
    }

    #[test]
    fn raw_pty_prints_without_session_manager() {
        use portable_pty::PtyPair;
        use std::io::Read;

        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("openpty");
        let PtyPair { master, slave } = pair;
        let cmd = CommandBuilder::new("/bin/sh");
        let mut cmd = cmd;
        cmd.arg("-c");
        cmd.arg("printf READY");
        let mut child = slave.spawn_command(cmd).expect("spawn");
        drop(slave);
        let mut reader = master.try_clone_reader().expect("reader");
        let _writer = master.take_writer().expect("writer");
        let mut buf = vec![0_u8; 32];
        let handle = std::thread::spawn(move || reader.read(&mut buf).map(|n| buf[..n].to_vec()));
        let output = handle.join().expect("join").expect("read");
        let _ = child.wait();
        assert!(contains_bytes(&output, b"READY"), "raw pty got {output:?}");
    }

    #[test]
    fn duplicate_start_is_rejected() {
        let manager = Arc::new(SessionManager::new());
        let id = SessionId::new();
        manager.start(id, &cat_spec(), 40, 12).expect("start");
        let error = manager.start(id, &cat_spec(), 40, 12).expect_err("dup");
        assert!(matches!(error, SessionError::AlreadyRunning));
        manager.kill(id).expect("cleanup");
    }

    #[test]
    fn resize_does_not_stop_the_process() {
        let manager = Arc::new(SessionManager::new());
        let id = SessionId::new();
        manager.start(id, &cat_spec(), 80, 24).expect("start");
        manager.resize(id, 40, 12).expect("resize");
        assert!(manager.is_live(id));
        manager.kill(id).expect("cleanup");
    }
}
