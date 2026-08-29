use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use cli_master_core::{
    AgentId, CommandSpec, ProjectId, Session, SessionId, SessionStatus, StatusReason,
};
use portable_pty::{ChildKiller, ExitStatus};
use tokio::sync::{broadcast, mpsc};

use crate::buffer::ReplayBuffer;
use crate::config::SessionManagerConfig;
use crate::error::SessionError;
use crate::event::{OutputChunk, SessionEvent, SessionSubscription};
use crate::pty::{NativePtyBackend, PtyBackend, PtySize, SpawnedPty};

/// Parameters for creating a managed session and spawning its process.
pub struct CreateSession {
    /// Project that owns the session metadata.
    pub project_id: ProjectId,
    /// Agent definition used to build the command.
    pub agent_id: AgentId,
    /// User-visible session name.
    pub name: String,
    /// Structured executable, arguments, cwd, and env overrides.
    pub command: CommandSpec,
    /// Initial PTY columns.
    pub cols: u16,
    /// Initial PTY rows.
    pub rows: u16,
}

/// Runtime owner of live PTY sessions.
///
/// Cloning shares the same session table. Dropping the last clone best-effort
/// kills remaining process groups so tests and daemon shutdown do not leak
/// children.
#[derive(Clone)]
pub struct SessionManager {
    inner: Arc<Inner>,
}

struct Inner {
    config: SessionManagerConfig,
    backend: Arc<dyn PtyBackend>,
    sessions: Mutex<HashMap<SessionId, Arc<LiveSession>>>,
    events: broadcast::Sender<SessionEvent>,
}

struct LiveSession {
    id: SessionId,
    config: SessionManagerConfig,
    state: Mutex<LiveState>,
    output: broadcast::Sender<OutputChunk>,
    writer_tx: Mutex<Option<std::sync::mpsc::SyncSender<WriteOp>>>,
    events: broadcast::Sender<SessionEvent>,
}

struct LiveState {
    record: Session,
    command: CommandSpec,
    buffer: ReplayBuffer,
    master: Option<Box<dyn portable_pty::MasterPty + Send>>,
    killer: Option<Box<dyn ChildKiller + Send + Sync>>,
    pid: Option<u32>,
    pgid: Option<i32>,
    generation: u64,
    last_activity: Instant,
    cols: u16,
    rows: u16,
    stop_requested: bool,
    kill_requested: bool,
}

enum WriteOp {
    Bytes(Vec<u8>),
    Shutdown,
}

impl SessionManager {
    /// Creates a manager that uses the native PTY backend.
    ///
    /// # Panics
    ///
    /// Panics if called outside a Tokio runtime.
    #[must_use]
    pub fn new(config: SessionManagerConfig) -> Self {
        Self::with_backend(config, Arc::new(NativePtyBackend))
    }

    fn with_backend(config: SessionManagerConfig, backend: Arc<dyn PtyBackend>) -> Self {
        let idle_scan = config.idle_scan;
        let (events, _) = broadcast::channel(config.event_capacity.max(1));
        let inner = Arc::new(Inner {
            config,
            backend,
            sessions: Mutex::new(HashMap::new()),
            events,
        });
        spawn_idle_scanner(Arc::downgrade(&inner), idle_scan);
        Self { inner }
    }

    /// Subscribes to manager-wide lifecycle and output events.
    #[must_use]
    pub fn subscribe_events(&self) -> broadcast::Receiver<SessionEvent> {
        self.inner.events.subscribe()
    }

    /// Creates a session, inserts it as `starting`, and spawns the process.
    ///
    /// # Errors
    ///
    /// Returns an error when the request is invalid or spawn fails.
    pub fn create(&self, request: CreateSession) -> Result<Session, SessionError> {
        let size = PtySize::new(request.cols, request.rows)?;
        let name = request.name.trim();
        if name.is_empty() {
            return Err(SessionError::InvalidName);
        }

        let id = SessionId::new();
        let now = unix_now_ms();
        let record = Session {
            id,
            project_id: request.project_id,
            name: name.to_owned(),
            agent_id: request.agent_id,
            cwd: request.command.cwd().clone(),
            pid: None,
            pty_id: None,
            branch: None,
            worktree_id: None,
            worktree_path: None,
            status: SessionStatus::Starting,
            exit_code: None,
            created_at_ms: now,
            updated_at_ms: now,
        };

        tracing::info!(
            event = "session.created",
            session_id = %id,
            executable = request.command.executable(),
            cols = size.cols,
            rows = size.rows,
            "created session"
        );

        let live = Arc::new(LiveSession {
            id,
            config: self.inner.config.clone(),
            state: Mutex::new(LiveState {
                record: record.clone(),
                command: request.command,
                buffer: ReplayBuffer::new(self.inner.config.replay_buffer_bytes),
                master: None,
                killer: None,
                pid: None,
                pgid: None,
                generation: 0,
                last_activity: Instant::now(),
                cols: size.cols,
                rows: size.rows,
                stop_requested: false,
                kill_requested: false,
            }),
            output: broadcast::channel(self.inner.config.subscriber_capacity.max(1)).0,
            writer_tx: Mutex::new(None),
            events: self.inner.events.clone(),
        });

        lock(&self.inner.sessions).insert(id, Arc::clone(&live));
        let _ = self.inner.events.send(SessionEvent::Created(record));
        self.spawn_into(&live)
    }

    /// Spawns a process for a session that is not live.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::AlreadyRunning`] when the session is live, or a
    /// spawn error.
    pub fn start(&self, id: SessionId) -> Result<Session, SessionError> {
        let live = self.lookup(id)?;
        let event = {
            let mut state = lock(&live.state);
            if state.record.status.is_live() {
                return Err(SessionError::AlreadyRunning(id));
            }
            state.generation += 1;
            state.stop_requested = false;
            state.kill_requested = false;
            state.record.exit_code = None;
            state.transition(SessionStatus::Starting, StatusReason::RestartRequested)
        };
        publish(&live, event);
        self.spawn_into(&live)
    }

    /// Returns a copy of the public session record.
    #[must_use]
    pub fn get(&self, id: SessionId) -> Option<Session> {
        let live = lock(&self.inner.sessions).get(&id).cloned()?;
        Some(lock(&live.state).record.clone())
    }

    /// Returns every known session, including exited ones still retained.
    #[must_use]
    pub fn list(&self) -> Vec<Session> {
        lock(&self.inner.sessions)
            .values()
            .map(|live| lock(&live.state).record.clone())
            .collect()
    }

    /// Counts sessions whose status implies a live process.
    #[must_use]
    pub fn live_count(&self) -> usize {
        lock(&self.inner.sessions)
            .values()
            .filter(|live| lock(&live.state).record.status.is_live())
            .count()
    }

    /// Subscribes to replay history plus live output without stopping the session.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::NotFound`] when the session is unknown.
    pub fn subscribe(&self, id: SessionId) -> Result<SessionSubscription, SessionError> {
        let live = self.lookup(id)?;
        let receiver = live.output.subscribe();
        let (session, snapshot) = {
            let state = lock(&live.state);
            (state.record.clone(), state.buffer.snapshot())
        };
        Ok(SessionSubscription::new(session, snapshot, receiver))
    }

    /// Writes raw bytes to the PTY master. `0x03` is Ctrl+C and `0x04` is Ctrl+D.
    ///
    /// # Errors
    ///
    /// Returns an error when the session is not running or the writer queue
    /// stays full past the write timeout.
    pub fn write(&self, id: SessionId, bytes: &[u8]) -> Result<(), SessionError> {
        if bytes.is_empty() {
            return Ok(());
        }
        let live = self.lookup(id)?;
        let sender = lock(&live.writer_tx)
            .clone()
            .ok_or(SessionError::NotRunning(id))?;
        sender
            .try_send(WriteOp::Bytes(bytes.to_vec()))
            .map_err(|error| match error {
                std::sync::mpsc::TrySendError::Full(_) => SessionError::WriteTimeout,
                std::sync::mpsc::TrySendError::Disconnected(_) => SessionError::NotRunning(id),
            })?;
        let event = {
            let mut state = lock(&live.state);
            state.last_activity = Instant::now();
            if state.record.status == SessionStatus::Idle {
                state.transition(SessionStatus::Running, StatusReason::Activity)
            } else {
                None
            }
        };
        publish(&live, event);
        Ok(())
    }

    /// Updates the PTY grid size. The size is stored even if the process has
    /// already exited so a later restart can reuse it.
    ///
    /// # Errors
    ///
    /// Returns an error when the size is invalid, the session is missing, or
    /// the kernel rejects the resize.
    pub fn resize(&self, id: SessionId, cols: u16, rows: u16) -> Result<(), SessionError> {
        let size = PtySize::new(cols, rows)?;
        let live = self.lookup(id)?;
        let mut state = lock(&live.state);
        state.cols = size.cols;
        state.rows = size.rows;
        if let Some(master) = state.master.as_ref() {
            master
                .resize(portable_pty::PtySize {
                    rows: size.rows,
                    cols: size.cols,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(|error| SessionError::Pty(error.to_string()))?;
        }
        tracing::info!(
            event = "session.resize",
            session_id = %id,
            cols = size.cols,
            rows = size.rows,
            "resized PTY"
        );
        Ok(())
    }

    /// Requests graceful shutdown, then escalates the signal if needed.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::NotFound`] when the session is unknown.
    pub async fn stop(&self, id: SessionId) -> Result<Session, SessionError> {
        self.request_stop(id, false).await
    }

    /// Sends SIGKILL to the session process group.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::NotFound`] when the session is unknown.
    pub async fn kill(&self, id: SessionId) -> Result<Session, SessionError> {
        self.request_stop(id, true).await
    }

    /// Stops a live session if needed, then spawns it again with the same id.
    ///
    /// # Errors
    ///
    /// Returns spawn or stop errors.
    pub async fn restart(&self, id: SessionId) -> Result<Session, SessionError> {
        let live = self.lookup(id)?;
        if lock(&live.state).record.status.is_live() {
            self.stop(id).await?;
        }
        self.start(id)
    }

    /// Removes an exited session and its replay buffer.
    ///
    /// # Errors
    ///
    /// Returns an error when the session is missing or still live.
    pub fn delete(&self, id: SessionId) -> Result<Session, SessionError> {
        let mut sessions = lock(&self.inner.sessions);
        let live = sessions
            .get(&id)
            .cloned()
            .ok_or(SessionError::NotFound(id))?;
        if lock(&live.state).record.status.is_live() {
            return Err(SessionError::StillRunning(id));
        }
        let record = lock(&live.state).record.clone();
        sessions.remove(&id);
        drop(sessions);
        tracing::info!(event = "session.cleanup", session_id = %id, "removed session record");
        let _ = self
            .inner
            .events
            .send(SessionEvent::Deleted(record.clone()));
        Ok(record)
    }

    /// Force-kills every live session. Used by daemon shutdown.
    pub async fn shutdown(&self) {
        let ids: Vec<SessionId> = lock(&self.inner.sessions).keys().copied().collect();
        for id in ids {
            let _ = self.kill(id).await;
        }
    }

    fn spawn_into(&self, live: &Arc<LiveSession>) -> Result<Session, SessionError> {
        let (command, size, generation) = {
            let state = lock(&live.state);
            (
                state.command.clone(),
                PtySize {
                    cols: state.cols,
                    rows: state.rows,
                },
                state.generation,
            )
        };

        tracing::info!(
            event = "session.spawn",
            session_id = %live.id,
            executable = command.executable(),
            cols = size.cols,
            rows = size.rows,
            generation,
            "spawning process"
        );

        let spawned = match self.inner.backend.spawn(&command, size) {
            Ok(spawned) => spawned,
            Err(error) => {
                fail_session(live, generation, StatusReason::SpawnFailed);
                return Err(error);
            }
        };

        tracing::info!(
            event = "session.spawned",
            session_id = %live.id,
            pid = spawned.pid,
            pgid = spawned.pgid,
            generation,
            "process spawned"
        );

        if let Err(error) = start_io(live, spawned, generation) {
            live.force_cleanup();
            fail_session(live, generation, StatusReason::SpawnFailed);
            return Err(error);
        }

        let mismatch = {
            let state = lock(&live.state);
            state.generation != generation
        };
        if mismatch {
            live.force_cleanup();
            return Err(SessionError::AlreadyRunning(live.id));
        }

        let (record, event) = {
            let mut state = lock(&live.state);
            state.record.pid = state.pid;
            state.record.pty_id = Some(format!("{}:{generation}", live.id));
            let next = if state.kill_requested || state.stop_requested {
                SessionStatus::Stopping
            } else {
                SessionStatus::Running
            };
            let reason = if state.kill_requested {
                StatusReason::KillRequested
            } else if state.stop_requested {
                StatusReason::StopRequested
            } else {
                StatusReason::Spawned
            };
            let event = state.transition(next, reason);
            (state.record.clone(), event)
        };
        publish(live, event);
        Ok(record)
    }

    async fn request_stop(&self, id: SessionId, force: bool) -> Result<Session, SessionError> {
        let live = self.lookup(id)?;
        let event = {
            let mut state = lock(&live.state);
            if !state.record.status.is_live() {
                return Ok(state.record.clone());
            }
            state.stop_requested = true;
            state.kill_requested = force || state.kill_requested;
            let reason = if force {
                StatusReason::KillRequested
            } else {
                StatusReason::StopRequested
            };
            state.transition(SessionStatus::Stopping, reason)
        };
        publish(&live, event);

        tracing::info!(
            event = "session.stop",
            session_id = %id,
            force,
            "stop requested"
        );

        let pgid = wait_for_pgid(&live, self.inner.config.interrupt_timeout).await;
        let mut killer = lock(&live.state)
            .killer
            .as_mut()
            .map(|killer| killer.clone_killer());

        if force {
            if let Some(pgid) = pgid {
                crate::unix::signal_group(pgid, crate::unix::kill_signal())?;
            }
            if let Some(killer) = killer.as_mut() {
                let _ = killer.kill();
            }
            wait_until_stopped(&live, self.inner.config.kill_timeout).await;
        } else {
            if let Some(pgid) = pgid {
                crate::unix::signal_group(pgid, crate::unix::interrupt_signal())?;
            }
            if !wait_until_stopped(&live, self.inner.config.interrupt_timeout).await {
                if let Some(pgid) = pgid {
                    crate::unix::signal_group(pgid, crate::unix::terminate_signal())?;
                }
            }
            if !wait_until_stopped(&live, self.inner.config.terminate_timeout).await {
                if let Some(pgid) = pgid {
                    crate::unix::signal_group(pgid, crate::unix::kill_signal())?;
                }
                if let Some(killer) = killer.as_mut() {
                    let _ = killer.kill();
                }
                wait_until_stopped(&live, self.inner.config.kill_timeout).await;
            }
        }

        self.get(id).ok_or(SessionError::NotFound(id))
    }

    fn lookup(&self, id: SessionId) -> Result<Arc<LiveSession>, SessionError> {
        lock(&self.inner.sessions)
            .get(&id)
            .cloned()
            .ok_or(SessionError::NotFound(id))
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        let sessions = std::mem::take(&mut *lock(&self.sessions));
        for live in sessions.into_values() {
            live.force_cleanup();
        }
    }
}

impl LiveSession {
    fn force_cleanup(&self) {
        let (pgid, master) = {
            let mut state = lock(&self.state);
            let pgid = state.pgid;
            let master = state.master.take();
            state.killer.take();
            (pgid, master)
        };
        if let Some(pgid) = pgid {
            let _ = crate::unix::signal_group(pgid, crate::unix::kill_signal());
        }
        if let Some(sender) = lock(&self.writer_tx).take() {
            let _ = sender.send(WriteOp::Shutdown);
        }
        drop(master);
    }
}

impl LiveState {
    fn transition(&mut self, next: SessionStatus, reason: StatusReason) -> Option<SessionEvent> {
        let previous = self.record.status;
        if previous == next {
            return None;
        }
        let at_ms = unix_now_ms();
        self.record.status = next;
        self.record.updated_at_ms = at_ms;
        Some(SessionEvent::StatusChanged {
            session_id: self.record.id,
            previous,
            current: next,
            reason,
            at_ms,
        })
    }
}

fn start_io(
    live: &Arc<LiveSession>,
    spawned: SpawnedPty,
    generation: u64,
) -> Result<(), SessionError> {
    let SpawnedPty {
        pid,
        pgid,
        mut child,
        master,
        reader,
        writer,
    } = spawned;

    {
        let mut state = lock(&live.state);
        state.pid = pid;
        state.pgid = pgid;
        state.master = Some(master);
        state.killer = Some(child.clone_killer());
        state.record.pid = pid;
    }

    let short_id = short_id(live.id);
    let writer_queue = live.config.writer_queue.max(1);
    let (writer_tx, writer_rx) = std::sync::mpsc::sync_channel(writer_queue);
    *lock(&live.writer_tx) = Some(writer_tx);

    std::thread::Builder::new()
        .name(format!("pty-w-{short_id}"))
        .spawn(move || writer_loop(&writer_rx, writer))
        .map_err(|error| SessionError::io(&error))?;

    let reader_queue = live.config.reader_queue.max(1);
    let (byte_tx, byte_rx) = mpsc::channel(reader_queue);
    tracing::info!(
        event = "session.reader_start",
        session_id = %live.id,
        pid,
        "PTY reader started"
    );
    let session_id = live.id;
    std::thread::Builder::new()
        .name(format!("pty-r-{short_id}"))
        .spawn(move || {
            reader_loop(reader, &byte_tx);
            tracing::info!(
                event = "session.reader_end",
                session_id = %session_id,
                "PTY reader ended"
            );
        })
        .map_err(|error| SessionError::io(&error))?;

    let live_batch = Arc::clone(live);
    tokio::spawn(async move {
        batch_loop(live_batch, byte_rx, generation).await;
    });

    let live_wait = Arc::clone(live);
    tokio::spawn(async move {
        let wait_result = tokio::task::spawn_blocking(move || child.wait()).await;
        on_child_exit(&live_wait, generation, wait_result);
    });

    Ok(())
}

fn reader_loop(mut reader: Box<dyn Read + Send>, tx: &mpsc::Sender<Vec<u8>>) {
    let mut buf = vec![0_u8; 8192];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if tx.blocking_send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
}

fn writer_loop(rx: &std::sync::mpsc::Receiver<WriteOp>, mut writer: Box<dyn Write + Send>) {
    while let Ok(op) = rx.recv() {
        match op {
            WriteOp::Bytes(bytes) => {
                if writer.write_all(&bytes).is_err() {
                    break;
                }
                let _ = writer.flush();
            }
            WriteOp::Shutdown => break,
        }
    }
}

async fn batch_loop(live: Arc<LiveSession>, mut rx: mpsc::Receiver<Vec<u8>>, generation: u64) {
    let window = live.config.output_batch_window;
    let max_bytes = live.config.output_batch_bytes.max(1);
    let mut pending = Vec::new();

    loop {
        if pending.is_empty() {
            match rx.recv().await {
                Some(bytes) => {
                    pending = bytes;
                    if pending.len() >= max_bytes {
                        flush_output(&live, generation, std::mem::take(&mut pending));
                    }
                }
                None => break,
            }
            continue;
        }

        tokio::select! {
            sample = rx.recv() => {
                if let Some(bytes) = sample {
                    pending.extend(bytes);
                    if pending.len() >= max_bytes {
                        flush_output(&live, generation, std::mem::take(&mut pending));
                    }
                } else {
                    flush_output(&live, generation, std::mem::take(&mut pending));
                    break;
                }
            }
            () = tokio::time::sleep(window) => {
                flush_output(&live, generation, std::mem::take(&mut pending));
            }
        }
    }
}

fn flush_output(live: &LiveSession, generation: u64, bytes: Vec<u8>) {
    if bytes.is_empty() {
        return;
    }

    let (chunk, event) = {
        let mut state = lock(&live.state);
        if state.generation != generation {
            return;
        }
        let sequence = state.buffer.push(bytes.clone());
        state.last_activity = Instant::now();
        let event = if state.record.status == SessionStatus::Idle {
            state.transition(SessionStatus::Running, StatusReason::Activity)
        } else {
            None
        };
        (
            OutputChunk {
                sequence,
                data: bytes,
            },
            event,
        )
    };

    let _ = live.output.send(chunk.clone());
    let _ = live.events.send(SessionEvent::Output {
        session_id: live.id,
        chunk,
    });
    publish(live, event);
}

fn on_child_exit(
    live: &LiveSession,
    generation: u64,
    wait_result: Result<Result<ExitStatus, std::io::Error>, tokio::task::JoinError>,
) {
    let exit_code = match wait_result {
        Ok(Ok(status)) => i32::try_from(status.exit_code()).ok(),
        Ok(Err(error)) => {
            tracing::warn!(
                event = "session.wait_failed",
                session_id = %live.id,
                error = %error,
                "wait on child failed"
            );
            None
        }
        Err(error) => {
            tracing::warn!(
                event = "session.wait_join_failed",
                session_id = %live.id,
                error = %error,
                "wait task join failed"
            );
            None
        }
    };

    let (master, record, status_event) = {
        let mut state = lock(&live.state);
        if state.generation != generation {
            return;
        }
        let master = state.master.take();
        state.killer.take();
        state.pid = None;
        state.pgid = None;
        state.record.pid = None;
        state.record.pty_id = None;
        state.record.exit_code = exit_code;
        let status_event = state.transition(SessionStatus::Exited, StatusReason::ProcessExited);
        (master, state.record.clone(), status_event)
    };
    drop(master);
    if let Some(sender) = lock(&live.writer_tx).take() {
        let _ = sender.send(WriteOp::Shutdown);
    }

    tracing::info!(
        event = "session.exit",
        session_id = %live.id,
        exit_code,
        "session process exited"
    );

    publish(live, status_event);
    let _ = live.events.send(SessionEvent::Exited {
        session_id: live.id,
        exit_code,
        status: record.status,
    });
    tracing::info!(
        event = "session.cleanup",
        session_id = %live.id,
        "released PTY handles"
    );
}

fn fail_session(live: &LiveSession, generation: u64, reason: StatusReason) {
    tracing::warn!(
        event = "session.failed",
        session_id = %live.id,
        reason = ?reason,
        "session failed"
    );
    let event = {
        let mut state = lock(&live.state);
        if state.generation != generation {
            return;
        }
        state.record.pid = None;
        state.record.pty_id = None;
        state.transition(SessionStatus::Failed, reason)
    };
    publish(live, event);
}

fn publish(live: &LiveSession, event: Option<SessionEvent>) {
    let Some(event) = event else {
        return;
    };
    let _ = live.events.send(event);
    let record = lock(&live.state).record.clone();
    let _ = live.events.send(SessionEvent::Updated(record));
}

fn spawn_idle_scanner(inner: Weak<Inner>, idle_scan: Duration) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(idle_scan);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            let Some(inner) = inner.upgrade() else {
                break;
            };
            scan_idle(&inner);
        }
    });
}

fn scan_idle(inner: &Inner) {
    let sessions: Vec<Arc<LiveSession>> = lock(&inner.sessions).values().cloned().collect();
    for live in sessions {
        let event = {
            let mut state = lock(&live.state);
            if state.record.status != SessionStatus::Running
                || state.last_activity.elapsed() < inner.config.idle_after
            {
                None
            } else {
                state.transition(SessionStatus::Idle, StatusReason::IdleTimeout)
            }
        };
        publish(&live, event);
    }
}

async fn wait_for_pgid(live: &LiveSession, timeout: Duration) -> Option<i32> {
    let deadline = Instant::now() + timeout;
    loop {
        {
            let state = lock(&live.state);
            if let Some(pgid) = state.pgid {
                return Some(pgid);
            }
            if !state.record.status.is_live() {
                return None;
            }
        }
        if Instant::now() >= deadline {
            return lock(&live.state).pgid;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn wait_until_stopped(live: &LiveSession, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if !lock(&live.state).record.status.is_live() {
            return true;
        }
        if Instant::now() >= deadline {
            return !lock(&live.state).record.status.is_live();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn unix_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or(0)
}

fn short_id(id: SessionId) -> String {
    let rendered = id.to_string();
    rendered.chars().take(8).collect()
}
