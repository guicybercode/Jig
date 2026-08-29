use std::collections::HashMap;
use std::io::{Read, Write};
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use cli_master_core::{CommandSpec, SessionId, SessionStatus};
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use portable_pty::{CommandBuilder, MasterPty, NativePtySystem, PtySize, PtySystem};

use crate::SessionError;

#[cfg(test)]
static SIGNAL_ATTEMPTS: AtomicUsize = AtomicUsize::new(0);

/// PTY dimensions applied at spawn and resize.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalSize {
    /// Column count.
    pub cols: u16,
    /// Row count.
    pub rows: u16,
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self { cols: 80, rows: 24 }
    }
}

impl TerminalSize {
    fn to_pty(self) -> PtySize {
        PtySize {
            rows: self.rows,
            cols: self.cols,
            pixel_width: 0,
            pixel_height: 0,
        }
    }
}

/// Tunables for replay buffering and cooperative stop.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionManagerConfig {
    /// Maximum retained replay bytes per session.
    pub max_buffer_bytes: usize,
    /// How long `stop` waits after SIGTERM before SIGKILL.
    pub stop_grace: Duration,
}

impl Default for SessionManagerConfig {
    fn default() -> Self {
        Self {
            max_buffer_bytes: 8 * 1024 * 1024,
            stop_grace: Duration::from_secs(2),
        }
    }
}

struct OutputBuffer {
    bytes: Vec<u8>,
    max_bytes: usize,
}

impl OutputBuffer {
    fn new(max_bytes: usize) -> Self {
        Self {
            bytes: Vec::new(),
            max_bytes,
        }
    }

    fn push(&mut self, data: &[u8]) {
        self.bytes.extend_from_slice(data);
        if self.bytes.len() > self.max_bytes {
            let overflow = self.bytes.len() - self.max_bytes;
            self.bytes.drain(..overflow);
        }
    }
}

struct LiveSession {
    spec: CommandSpec,
    size: TerminalSize,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    buffer: Arc<Mutex<OutputBuffer>>,
    pid: Option<u32>,
    exit_code: Option<i32>,
    state: SessionStatus,
}

/// Owns live PTY sessions and their replay buffers.
pub struct SessionManager {
    config: SessionManagerConfig,
    sessions: Mutex<HashMap<SessionId, LiveSession>>,
}

impl SessionManager {
    /// Creates a manager with default buffer and grace settings.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(SessionManagerConfig::default())
    }

    /// Creates a manager with explicit tunables.
    #[must_use]
    pub fn with_config(config: SessionManagerConfig) -> Self {
        Self {
            config,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    /// Spawns `spec` in a PTY and starts an output reader.
    ///
    /// # Errors
    ///
    /// Returns an error when the working directory is invalid or the PTY layer
    /// cannot spawn the process.
    pub fn start(
        &self,
        session_id: SessionId,
        spec: CommandSpec,
        size: TerminalSize,
    ) -> Result<(), SessionError> {
        if !spec.cwd().is_dir() {
            return Err(SessionError::InvalidWorkingDirectory(spec.cwd().clone()));
        }

        let mut sessions = self.lock()?;
        if sessions.contains_key(&session_id) {
            return Err(SessionError::DuplicateSession(session_id));
        }

        let system = NativePtySystem::default();
        let pair = system
            .openpty(size.to_pty())
            .map_err(|error| SessionError::Pty(error.to_string()))?;

        let mut command = CommandBuilder::new(spec.executable());
        for argument in spec.args() {
            command.arg(argument);
        }
        command.cwd(spec.cwd());
        for (key, value) in spec.env() {
            command.env(key, value);
        }

        let mut child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| SessionError::Pty(error.to_string()))?;
        drop(pair.slave);

        let mut reader = match pair.master.try_clone_reader() {
            Ok(reader) => reader,
            Err(error) => {
                let _ = child.kill();
                return Err(SessionError::Pty(error.to_string()));
            }
        };
        let writer = match pair.master.take_writer() {
            Ok(writer) => writer,
            Err(error) => {
                let _ = child.kill();
                return Err(SessionError::Pty(error.to_string()));
            }
        };
        let buffer = Arc::new(Mutex::new(OutputBuffer::new(self.config.max_buffer_bytes)));
        let reader_buffer = Arc::clone(&buffer);
        if let Err(error) = thread::Builder::new()
            .name(format!("pty-reader-{session_id}"))
            .spawn(move || read_output(&mut *reader, &reader_buffer))
        {
            let _ = child.kill();
            return Err(SessionError::Pty(error.to_string()));
        }

        let pid = child.process_id();
        let live = LiveSession {
            spec,
            size,
            child,
            writer,
            master: pair.master,
            buffer,
            pid,
            exit_code: None,
            state: SessionStatus::Running,
        };

        sessions.insert(session_id, live);
        Ok(())
    }

    /// Writes bytes to the PTY master. Ctrl+C is the byte `0x03`.
    ///
    /// # Errors
    ///
    /// Returns an error when the session is unknown or the write fails.
    pub fn write(&self, session_id: SessionId, bytes: &[u8]) -> Result<(), SessionError> {
        let mut sessions = self.lock()?;
        let live = sessions
            .get_mut(&session_id)
            .ok_or(SessionError::UnknownSession(session_id))?;
        live.writer.write_all(bytes)?;
        live.writer.flush()?;
        Ok(())
    }

    /// Resizes the PTY.
    ///
    /// # Errors
    ///
    /// Returns an error when the session is unknown or the PTY cannot resize.
    pub fn resize(&self, session_id: SessionId, size: TerminalSize) -> Result<(), SessionError> {
        let mut sessions = self.lock()?;
        let live = sessions
            .get_mut(&session_id)
            .ok_or(SessionError::UnknownSession(session_id))?;
        live.master
            .resize(size.to_pty())
            .map_err(|error| SessionError::Pty(error.to_string()))?;
        live.size = size;
        Ok(())
    }

    /// Returns a copy of the bounded replay buffer.
    ///
    /// # Errors
    ///
    /// Returns an error when the session is unknown.
    pub fn snapshot(&self, session_id: SessionId) -> Result<Vec<u8>, SessionError> {
        let sessions = self.lock()?;
        let live = sessions
            .get(&session_id)
            .ok_or(SessionError::UnknownSession(session_id))?;
        let buffer = live
            .buffer
            .lock()
            .map_err(|_| SessionError::Pty("output buffer lock poisoned".to_owned()))?;
        Ok(buffer.bytes.clone())
    }

    /// Polls until `needle` appears in the replay buffer or `timeout` elapses.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::Timeout`] when the needle is not observed.
    pub fn wait_for_output(
        &self,
        session_id: SessionId,
        needle: &str,
        timeout: Duration,
    ) -> Result<Vec<u8>, SessionError> {
        let started = Instant::now();
        loop {
            let snapshot = self.snapshot(session_id)?;
            if String::from_utf8_lossy(&snapshot).contains(needle) {
                return Ok(snapshot);
            }
            if started.elapsed() >= timeout {
                return Err(SessionError::Timeout {
                    session_id,
                    observed: String::from_utf8_lossy(&snapshot).into_owned(),
                });
            }
            thread::sleep(Duration::from_millis(25));
        }
    }

    /// Returns the inferred lifecycle state, refreshing from the child status.
    ///
    /// # Errors
    ///
    /// Returns an error when the session is unknown.
    pub fn status(&self, session_id: SessionId) -> Result<SessionStatus, SessionError> {
        let mut sessions = self.lock()?;
        let live = sessions
            .get_mut(&session_id)
            .ok_or(SessionError::UnknownSession(session_id))?;
        refresh_child(live);
        Ok(live.state)
    }

    /// Returns the recorded exit code once the process has ended.
    ///
    /// # Errors
    ///
    /// Returns an error when the session is unknown.
    pub fn exit_code(&self, session_id: SessionId) -> Result<Option<i32>, SessionError> {
        let mut sessions = self.lock()?;
        let live = sessions
            .get_mut(&session_id)
            .ok_or(SessionError::UnknownSession(session_id))?;
        refresh_child(live);
        Ok(live.exit_code)
    }

    /// Sends interrupt, SIGTERM, then SIGKILL after the configured grace period.
    ///
    /// # Errors
    ///
    /// Returns an error when the session is unknown.
    pub fn stop(&self, session_id: SessionId) -> Result<Option<i32>, SessionError> {
        self.terminate(session_id, self.config.stop_grace)
    }

    /// Sends SIGKILL immediately.
    ///
    /// # Errors
    ///
    /// Returns an error when the session is unknown.
    pub fn kill(&self, session_id: SessionId) -> Result<Option<i32>, SessionError> {
        self.terminate(session_id, Duration::ZERO)
    }

    /// Stops the current process and starts a new PTY with the same command.
    ///
    /// # Errors
    ///
    /// Returns an error when stop or start fails.
    pub fn restart(&self, session_id: SessionId) -> Result<(), SessionError> {
        let (spec, size) = {
            let sessions = self.lock()?;
            let live = sessions
                .get(&session_id)
                .ok_or(SessionError::UnknownSession(session_id))?;
            (live.spec.clone(), live.size)
        };
        let _ = self.stop(session_id)?;
        {
            let mut sessions = self.lock()?;
            sessions.remove(&session_id);
        }
        self.start(session_id, spec, size)
    }

    /// Drops session bookkeeping after the process has exited.
    ///
    /// # Errors
    ///
    /// Returns an error when the session is still running.
    pub fn forget(&self, session_id: SessionId) -> Result<(), SessionError> {
        let mut sessions = self.lock()?;
        let live = sessions
            .get_mut(&session_id)
            .ok_or(SessionError::UnknownSession(session_id))?;
        refresh_child(live);
        if matches!(
            live.state,
            SessionStatus::Running | SessionStatus::Starting | SessionStatus::Idle
        ) {
            return Err(SessionError::Pty(
                "cannot forget a live session; stop it first".to_owned(),
            ));
        }
        sessions.remove(&session_id);
        Ok(())
    }

    fn terminate(
        &self,
        session_id: SessionId,
        grace: Duration,
    ) -> Result<Option<i32>, SessionError> {
        {
            let mut sessions = self.lock()?;
            let live = sessions
                .get_mut(&session_id)
                .ok_or(SessionError::UnknownSession(session_id))?;
            refresh_child(live);
            if matches!(live.state, SessionStatus::Exited | SessionStatus::Failed) {
                return Ok(live.exit_code);
            }
            let _ = live.writer.write_all(&[0x03]);
            let _ = live.writer.flush();
            if let Some(pid) = live.pid {
                send_signal(pid, Signal::SIGTERM);
            }
        }

        let deadline = Instant::now() + grace;
        while Instant::now() < deadline {
            if matches!(
                self.status(session_id)?,
                SessionStatus::Exited | SessionStatus::Failed
            ) {
                return self.exit_code(session_id);
            }
            thread::sleep(Duration::from_millis(25));
        }

        {
            let mut sessions = self.lock()?;
            if let Some(live) = sessions.get_mut(&session_id) {
                if let Some(pid) = live.pid {
                    send_signal(pid, Signal::SIGKILL);
                }
                let _ = live.child.kill();
            }
        }

        let wait_deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < wait_deadline {
            if matches!(
                self.status(session_id)?,
                SessionStatus::Exited | SessionStatus::Failed
            ) {
                return self.exit_code(session_id);
            }
            thread::sleep(Duration::from_millis(25));
        }
        self.exit_code(session_id)
    }

    fn lock(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, HashMap<SessionId, LiveSession>>, SessionError> {
        self.sessions
            .lock()
            .map_err(|_| SessionError::Pty("session map lock poisoned".to_owned()))
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

fn read_output(reader: &mut dyn Read, buffer: &Arc<Mutex<OutputBuffer>>) {
    let mut chunk = [0_u8; 4096];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => {
                if let Ok(mut buffer) = buffer.lock() {
                    buffer.push(&chunk[..count]);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
}

fn refresh_child(live: &mut LiveSession) {
    if !matches!(
        live.state,
        SessionStatus::Running | SessionStatus::Starting | SessionStatus::Idle
    ) {
        return;
    }
    if let Ok(Some(status)) = live.child.try_wait() {
        let code = i32::try_from(status.exit_code()).unwrap_or(1);
        live.exit_code = Some(code);
        live.state = if status.success() {
            SessionStatus::Exited
        } else {
            SessionStatus::Failed
        };
    }
}

fn send_signal(pid: u32, signal: Signal) {
    #[cfg(test)]
    SIGNAL_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
    if let Ok(raw) = i32::try_from(pid) {
        let _ = kill(Pid::from_raw(raw), signal);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn stop_does_not_signal_a_process_that_already_exited() {
        let temp = TempDir::new().expect("temporary directory");
        let executable = std::env::current_exe().expect("current test executable");
        let spec = CommandSpec::try_from_parts(
            executable.to_string_lossy(),
            ["--list"],
            temp.path(),
            BTreeMap::new(),
        )
        .expect("test command");
        let manager = SessionManager::with_config(SessionManagerConfig {
            max_buffer_bytes: 64 * 1024,
            stop_grace: Duration::from_secs(30),
        });
        let session_id = SessionId::new();
        manager
            .start(session_id, spec, TerminalSize::default())
            .expect("session should start");

        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if manager.status(session_id).expect("session status") == SessionStatus::Exited {
                break;
            }
            assert!(Instant::now() < deadline, "test child did not exit");
            thread::sleep(Duration::from_millis(10));
        }

        SIGNAL_ATTEMPTS.store(0, Ordering::Relaxed);
        assert_eq!(
            manager.stop(session_id).expect("stop completed session"),
            Some(0)
        );
        assert_eq!(SIGNAL_ATTEMPTS.load(Ordering::Relaxed), 0);
    }
}
