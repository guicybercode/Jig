use std::{
    io,
    path::Path,
    sync::{Arc, mpsc},
    time::Instant,
};

use cli_master_core::{CommandSpec, SessionId, SessionStatus};
use nix::unistd::Pid;
use parking_lot::{Condvar, Mutex};
use portable_pty::{Child, CommandBuilder, ExitStatus, MasterPty, native_pty_system};
use tokio::sync::broadcast;

use crate::{
    IoOperation, SessionError, SessionEvent, SessionHandle, SessionManagerConfig, TerminalSize,
    replay::ReplayBuffer,
    state::{IoAccess, RuntimeState, unix_epoch_ms},
    worker::{
        WorkerHandle, WriteProgress, WriteRequest, WriterCommand, WriterOutcome, attach_reader,
        start_reader_before_spawn, start_runtime_workers,
    },
};

mod lifecycle;
mod process_tree;
mod signals;
mod status;

use process_tree::ProcessTree;

pub(crate) struct SessionRuntime {
    id: SessionId,
    pid: Option<u32>,
    process_group_id: Mutex<Option<Pid>>,
    process_tree: Mutex<ProcessTree>,
    master: Mutex<Option<Box<dyn MasterPty + Send>>>,
    leader: Mutex<LeaderProcess>,
    lifecycle: Mutex<()>,
    writer_sender: Mutex<Option<mpsc::SyncSender<WriterCommand>>>,
    state: Mutex<RuntimeState>,
    state_changed: Condvar,
    replay: Mutex<ReplayBuffer>,
    events: broadcast::Sender<SessionEvent>,
    workers: Mutex<Vec<WorkerHandle>>,
    config: Arc<SessionManagerConfig>,
}

pub(super) enum ChildPoll {
    Running,
    Exited(ExitStatus),
    Reaped,
    Unavailable,
}

struct LeaderProcess {
    child: Option<Box<dyn Child + Send + Sync>>,
    reaped: bool,
}

impl SessionRuntime {
    pub fn spawn(
        id: SessionId,
        command: &CommandSpec,
        terminal_size: TerminalSize,
        config: Arc<SessionManagerConfig>,
    ) -> Result<Arc<Self>, SessionError> {
        terminal_size.validate()?;
        validate_working_directory(command.cwd())?;
        let pair = native_pty_system()
            .openpty(terminal_size.into())
            .map_err(|source| SessionError::OpenPty { source })?;
        let portable_pty::PtyPair { slave, master } = pair;
        let reader = master
            .try_clone_reader()
            .map_err(|source| SessionError::OpenPtyStream {
                stream: "reader",
                source,
            })?;
        let writer = master
            .take_writer()
            .map_err(|source| SessionError::OpenPtyStream {
                stream: "writer",
                source,
            })?;
        let pending_reader = start_reader_before_spawn(id, reader, config.read_chunk_bytes)?;
        let mut child = match slave.spawn_command(build_command(command)) {
            Ok(child) => child,
            Err(source) => {
                drop(slave);
                cleanup_unattached_reader(pending_reader, writer, master, config.kill_wait)?;
                return Err(SessionError::Spawn {
                    executable: command.executable().to_owned(),
                    source,
                });
            }
        };
        drop(slave);

        let Some((pid, process_group_id)) = validated_process_identity(child.process_id()) else {
            let _ = child.kill();
            let _ = child.wait();
            cleanup_unattached_reader(pending_reader, writer, master, config.kill_wait)?;
            return Err(SessionError::ProcessIdUnavailable {
                executable: command.executable().to_owned(),
            });
        };
        let process_tree = match ProcessTree::new(
            process_group_id,
            config.process_scan_timeout,
            config.max_tracked_processes,
        ) {
            Ok(process_tree) => process_tree,
            Err(source) => {
                // The child handle prevents PID reuse until it is reaped. Still
                // verify that this live identity remains in the freshly
                // allocated group before using the numeric PGID for cleanup.
                let leader_is_live = matches!(child.try_wait(), Ok(None));
                let leader_owns_group = matches!(
                    nix::unistd::getpgid(Some(process_group_id)),
                    Ok(actual_group) if actual_group == process_group_id
                );
                if leader_is_live && leader_owns_group {
                    let _ = nix::sys::signal::killpg(process_group_id, nix::sys::signal::SIGKILL);
                }
                let _ = child.kill();
                let _ = child.wait();
                cleanup_unattached_reader(pending_reader, writer, master, config.kill_wait)?;
                return Err(SessionError::ProcessInspection {
                    session_id: id,
                    source,
                });
            }
        };
        let now_ms = unix_epoch_ms();
        let (events, _) = broadcast::channel(config.event_capacity);
        let runtime = Arc::new(Self {
            id,
            pid: Some(pid),
            process_group_id: Mutex::new(Some(process_group_id)),
            process_tree: Mutex::new(process_tree),
            master: Mutex::new(Some(master)),
            leader: Mutex::new(LeaderProcess {
                child: Some(child),
                reaped: false,
            }),
            lifecycle: Mutex::new(()),
            writer_sender: Mutex::new(None),
            state: Mutex::new(initial_runtime_state(terminal_size, now_ms)),
            state_changed: Condvar::new(),
            replay: Mutex::new(ReplayBuffer::new(
                config.replay_max_bytes,
                config.replay_max_chunks,
            )),
            events,
            workers: Mutex::new(Vec::new()),
            config,
        });
        if let Err(error) = attach_reader(&runtime, pending_reader) {
            runtime.cleanup_failed_start();
            return Err(error);
        }
        if let Err(error) = start_runtime_workers(&runtime, writer) {
            runtime.cleanup_failed_start();
            return Err(error);
        }
        Ok(runtime)
    }

    pub fn handle(&self) -> SessionHandle {
        SessionHandle {
            id: self.id,
            pid: self.pid,
        }
    }

    pub fn write(&self, bytes: &[u8]) -> Result<(), SessionError> {
        self.ensure_live()?;
        if bytes.len() > self.config.max_write_bytes {
            return Err(SessionError::InputTooLarge {
                session_id: self.id,
                actual: bytes.len(),
                maximum: self.config.max_write_bytes,
            });
        }
        if bytes.is_empty() {
            return Ok(());
        }
        self.write_with_deadline(bytes.to_vec())
    }

    pub fn resize(&self, terminal_size: TerminalSize) -> Result<(), SessionError> {
        terminal_size.validate()?;
        self.ensure_live()?;
        let master = self.master.lock();
        let Some(master) = master.as_ref() else {
            return Err(SessionError::NotLive {
                session_id: self.id,
                status: self.state.lock().status,
            });
        };
        master
            .resize(terminal_size.into())
            .map_err(|source| SessionError::Resize {
                session_id: self.id,
                source,
            })?;
        self.state.lock().terminal_size = terminal_size;
        Ok(())
    }

    pub(super) fn id(&self) -> SessionId {
        self.id
    }

    pub(super) fn config(&self) -> &SessionManagerConfig {
        &self.config
    }

    pub(super) fn set_writer_sender(&self, sender: mpsc::SyncSender<WriterCommand>) {
        *self.writer_sender.lock() = Some(sender);
    }

    pub(super) fn register_worker(&self, worker: WorkerHandle) {
        self.workers.lock().push(worker);
    }

    pub(super) fn poll_child_exit(&self) -> io::Result<ChildPoll> {
        let mut leader = self.leader.lock();
        if leader.reaped {
            return Ok(ChildPoll::Reaped);
        }
        let Some(child) = leader.child.as_mut() else {
            return Ok(ChildPoll::Unavailable);
        };
        match child.try_wait()? {
            Some(exit_status) => {
                leader.child.take();
                leader.reaped = true;
                Ok(ChildPoll::Exited(exit_status))
            }
            None => Ok(ChildPoll::Running),
        }
    }

    fn write_with_deadline(&self, bytes: Vec<u8>) -> Result<(), SessionError> {
        let (reply_sender, reply_receiver) = mpsc::sync_channel(1);
        let request = Arc::new(WriteRequest::new(
            bytes,
            Instant::now() + self.config.write_timeout,
            reply_sender,
        ));
        let send_result = {
            let writer_guard = self.writer_sender.lock();
            let Some(writer) = writer_guard.as_ref() else {
                drop(writer_guard);
                return self.fail_input_unavailable();
            };
            writer.try_send(WriterCommand::Write(Arc::clone(&request)))
        };
        match send_result {
            Ok(()) => {}
            Err(mpsc::TrySendError::Full(_)) => {
                return Err(SessionError::InputBackpressure {
                    session_id: self.id,
                });
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                return self.fail_input_unavailable();
            }
        }
        match reply_receiver.recv_timeout(self.config.write_timeout) {
            Ok(WriterOutcome::Written) => Ok(()),
            Ok(WriterOutcome::Cancelled) => Err(SessionError::InputTimedOut {
                session_id: self.id,
            }),
            Ok(WriterOutcome::FailedBeforeDelivery) => self.fail_write_before_delivery(),
            Ok(WriterOutcome::DeliveryAmbiguous) => self.fail_ambiguous_input(),
            Err(mpsc::RecvTimeoutError::Timeout) => self.resolve_write_timeout(&request),
            Err(mpsc::RecvTimeoutError::Disconnected) => self.resolve_writer_disconnect(&request),
        }
    }

    fn resolve_write_timeout(&self, request: &WriteRequest) -> Result<(), SessionError> {
        if request.cancel_if_queued() {
            return Err(SessionError::InputTimedOut {
                session_id: self.id,
            });
        }
        match request.progress() {
            WriteProgress::Written => Ok(()),
            WriteProgress::Cancelled | WriteProgress::Queued => Err(SessionError::InputTimedOut {
                session_id: self.id,
            }),
            WriteProgress::FailedBeforeDelivery => self.fail_write_before_delivery(),
            WriteProgress::Writing | WriteProgress::DeliveryAmbiguous => {
                self.fail_ambiguous_input()
            }
        }
    }

    fn fail_ambiguous_input(&self) -> Result<(), SessionError> {
        self.invalidate_after_ambiguous_write();
        Err(SessionError::InputDeliveryAmbiguous {
            session_id: self.id,
        })
    }

    fn fail_write_before_delivery(&self) -> Result<(), SessionError> {
        self.emit_io_failure(IoOperation::Write);
        self.invalidate_supervision();
        Err(SessionError::WriteFailed {
            session_id: self.id,
        })
    }

    fn fail_input_unavailable(&self) -> Result<(), SessionError> {
        self.emit_io_failure(IoOperation::Write);
        self.invalidate_supervision();
        Err(SessionError::InputUnavailable {
            session_id: self.id,
        })
    }

    fn resolve_writer_disconnect(&self, request: &WriteRequest) -> Result<(), SessionError> {
        if request.cancel_if_queued() {
            return self.fail_input_unavailable();
        }
        match request.progress() {
            WriteProgress::Written => Ok(()),
            WriteProgress::Cancelled | WriteProgress::Queued => self.fail_input_unavailable(),
            WriteProgress::FailedBeforeDelivery => self.fail_write_before_delivery(),
            WriteProgress::Writing | WriteProgress::DeliveryAmbiguous => {
                self.fail_ambiguous_input()
            }
        }
    }
}

fn initial_runtime_state(terminal_size: TerminalSize, now_ms: i64) -> RuntimeState {
    RuntimeState {
        // Session creation is synchronous: no observer can access this runtime
        // until the child and all required workers are ready. Publishing an
        // unobservable `Starting -> Running` event would give callers a false
        // lifecycle contract, so registered runtimes begin in `Running`.
        status: SessionStatus::Running,
        exit_code: None,
        created_at_ms: now_ms,
        last_activity_at_ms: now_ms,
        last_activity_instant: Instant::now(),
        terminal_size,
        stop_requested: false,
        reader_finished: false,
        io_access: IoAccess::Open,
        io_failure_reported: false,
    }
}

fn build_command(command: &CommandSpec) -> CommandBuilder {
    let mut builder = CommandBuilder::new(command.executable());
    builder.args(command.args());
    builder.cwd(command.cwd());
    if !command.env().contains_key("TERM") {
        builder.env("TERM", "xterm-256color");
    }
    if !command.env().contains_key("COLORTERM") {
        builder.env("COLORTERM", "truecolor");
    }
    for (key, value) in command.env() {
        builder.env(key, value);
    }
    builder
}

fn validate_working_directory(path: &Path) -> Result<(), SessionError> {
    if path.is_dir() {
        Ok(())
    } else {
        Err(SessionError::WorkingDirectoryUnavailable {
            path: path.to_path_buf(),
        })
    }
}

fn validated_process_identity(pid: Option<u32>) -> Option<(u32, Pid)> {
    let pid = pid?;
    let process_group_id = i32::try_from(pid).ok()?;
    (process_group_id > 0).then_some((pid, Pid::from_raw(process_group_id)))
}

fn cleanup_unattached_reader(
    pending_reader: crate::worker::PendingReader,
    writer: Box<dyn io::Write + Send>,
    master: Box<dyn MasterPty + Send>,
    timeout: std::time::Duration,
) -> Result<(), SessionError> {
    drop(writer);
    drop(master);
    pending_reader.cancel_and_join(timeout)
}
