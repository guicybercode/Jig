use std::{
    io::{Read, Write},
    sync::{
        Arc, Weak,
        atomic::{AtomicU8, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use cli_master_core::SessionId;
use nix::errno::Errno;

use crate::{SessionError, runtime::SessionRuntime};

pub(super) struct WorkerHandle {
    pub role: &'static str,
    pub handle: JoinHandle<()>,
    pub completed: mpsc::Receiver<()>,
}

pub(super) struct PendingReader {
    runtime_sender: mpsc::SyncSender<Weak<SessionRuntime>>,
    worker: WorkerHandle,
}

impl PendingReader {
    pub fn cancel_and_join(self, timeout: Duration) -> Result<(), SessionError> {
        let Self {
            runtime_sender,
            worker,
        } = self;
        drop(runtime_sender);
        match worker.completed.recv_timeout(timeout) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => worker
                .handle
                .join()
                .map_err(|_| SessionError::WorkerPanicked { role: worker.role }),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                Err(SessionError::WorkerJoinTimedOut { role: worker.role })
            }
        }
    }
}

pub(super) enum WriterCommand {
    Write(Arc<WriteRequest>),
    Close,
}

pub(super) struct WriteRequest {
    bytes: Vec<u8>,
    deadline: Instant,
    state: AtomicU8,
    reply: mpsc::SyncSender<WriterOutcome>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum WriterOutcome {
    Written,
    Cancelled,
    FailedBeforeDelivery,
    DeliveryAmbiguous,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum WriteProgress {
    Queued,
    Writing,
    Written,
    Cancelled,
    FailedBeforeDelivery,
    DeliveryAmbiguous,
}

const WRITE_QUEUED: u8 = 0;
const WRITE_IN_PROGRESS: u8 = 1;
const WRITE_COMPLETE: u8 = 2;
const WRITE_CANCELLED: u8 = 3;
const WRITE_FAILED_BEFORE_DELIVERY: u8 = 4;
const WRITE_DELIVERY_AMBIGUOUS: u8 = 5;

impl WriteRequest {
    pub fn new(bytes: Vec<u8>, deadline: Instant, reply: mpsc::SyncSender<WriterOutcome>) -> Self {
        Self {
            bytes,
            deadline,
            state: AtomicU8::new(WRITE_QUEUED),
            reply,
        }
    }

    pub fn progress(&self) -> WriteProgress {
        match self.state.load(Ordering::Acquire) {
            WRITE_QUEUED => WriteProgress::Queued,
            WRITE_IN_PROGRESS => WriteProgress::Writing,
            WRITE_COMPLETE => WriteProgress::Written,
            WRITE_CANCELLED => WriteProgress::Cancelled,
            WRITE_FAILED_BEFORE_DELIVERY => WriteProgress::FailedBeforeDelivery,
            _ => WriteProgress::DeliveryAmbiguous,
        }
    }

    pub fn cancel_if_queued(&self) -> bool {
        self.state
            .compare_exchange(
                WRITE_QUEUED,
                WRITE_CANCELLED,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    fn begin(&self) -> bool {
        if Instant::now() >= self.deadline {
            let _ = self.cancel_if_queued();
            return false;
        }
        self.state
            .compare_exchange(
                WRITE_QUEUED,
                WRITE_IN_PROGRESS,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    fn finish(&self, outcome: WriterOutcome) {
        let state = match outcome {
            WriterOutcome::Written => WRITE_COMPLETE,
            WriterOutcome::Cancelled => WRITE_CANCELLED,
            WriterOutcome::FailedBeforeDelivery => WRITE_FAILED_BEFORE_DELIVERY,
            WriterOutcome::DeliveryAmbiguous => WRITE_DELIVERY_AMBIGUOUS,
        };
        self.state.store(state, Ordering::Release);
        let _ = self.reply.send(outcome);
    }
}

pub(super) fn start_reader_before_spawn(
    session_id: SessionId,
    reader: Box<dyn Read + Send>,
    read_chunk_bytes: usize,
) -> Result<PendingReader, SessionError> {
    let (runtime_sender, runtime_receiver) = mpsc::sync_channel(1);
    let (ready_sender, ready_receiver) = mpsc::sync_channel(0);
    let worker = create_worker(session_id, "pty-reader", move || {
        let _ = ready_sender.send(());
        preattached_reader_loop(reader, read_chunk_bytes, &runtime_receiver);
    })?;
    if ready_receiver.recv().is_err() {
        let _ = worker.handle.join();
        return Err(SessionError::WorkerPanicked { role: worker.role });
    }
    Ok(PendingReader {
        runtime_sender,
        worker,
    })
}

pub(super) fn attach_reader(
    runtime: &Arc<SessionRuntime>,
    reader: PendingReader,
) -> Result<(), SessionError> {
    if reader.runtime_sender.send(Arc::downgrade(runtime)).is_err() {
        let _ = reader.worker.handle.join();
        return Err(SessionError::WorkerPanicked {
            role: reader.worker.role,
        });
    }
    runtime.register_worker(reader.worker);
    Ok(())
}

pub(super) fn start_runtime_workers(
    runtime: &Arc<SessionRuntime>,
    writer: Box<dyn Write + Send>,
) -> Result<(), SessionError> {
    let (writer_sender, writer_receiver) =
        mpsc::sync_channel(runtime.config().write_queue_capacity);
    runtime.set_writer_sender(writer_sender);
    spawn_worker(runtime, "pty-writer", move |weak| {
        writer_loop(&weak, writer, &writer_receiver);
    })?;
    spawn_worker(runtime, "child-supervisor", |weak| {
        supervisor_loop(&weak);
    })
}

fn spawn_worker<F>(
    runtime: &Arc<SessionRuntime>,
    role: &'static str,
    task: F,
) -> Result<(), SessionError>
where
    F: FnOnce(Weak<SessionRuntime>) + Send + 'static,
{
    let weak = Arc::downgrade(runtime);
    let worker = create_worker(runtime.id(), role, move || task(weak))?;
    runtime.register_worker(worker);
    Ok(())
}

fn create_worker<F>(
    session_id: SessionId,
    role: &'static str,
    task: F,
) -> Result<WorkerHandle, SessionError>
where
    F: FnOnce() + Send + 'static,
{
    let (completed_sender, completed_receiver) = mpsc::sync_channel(1);
    let handle = thread::Builder::new()
        .name(format!("cli-master-{session_id}-{role}"))
        .spawn(move || {
            task();
            let _ = completed_sender.send(());
        })
        .map_err(|source| SessionError::WorkerStart { role, source })?;
    Ok(WorkerHandle {
        role,
        handle,
        completed: completed_receiver,
    })
}

fn writer_loop(
    runtime: &Weak<SessionRuntime>,
    writer: Box<dyn Write + Send>,
    receiver: &mpsc::Receiver<WriterCommand>,
) {
    writer_loop_with_failure_handler(runtime, writer, receiver, |_| {
        if let Some(runtime) = runtime.upgrade() {
            runtime.writer_stream_failed();
        }
    });
}

fn writer_loop_with_failure_handler<F>(
    runtime: &Weak<SessionRuntime>,
    mut writer: Box<dyn Write + Send>,
    receiver: &mpsc::Receiver<WriterCommand>,
    mut permanent_failure: F,
) where
    F: FnMut(WriterOutcome),
{
    while let Ok(command) = receiver.recv() {
        match command {
            WriterCommand::Write(request) => {
                if !request.begin() {
                    request.finish(WriterOutcome::Cancelled);
                    continue;
                }
                let outcome = write_bytes(runtime, writer.as_mut(), &request.bytes);
                request.finish(outcome);
                match outcome {
                    WriterOutcome::Written | WriterOutcome::Cancelled => {}
                    WriterOutcome::FailedBeforeDelivery | WriterOutcome::DeliveryAmbiguous => {
                        permanent_failure(outcome);
                        break;
                    }
                }
            }
            WriterCommand::Close => break,
        }
    }
}

fn write_bytes(
    runtime: &Weak<SessionRuntime>,
    writer: &mut dyn Write,
    bytes: &[u8],
) -> WriterOutcome {
    let mut written = 0;
    while written < bytes.len() {
        match writer.write(&bytes[written..]) {
            Ok(0) => return write_failure(written),
            Ok(count) => written += count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return write_failure(written),
        }
    }
    if writer.flush().is_err() {
        return WriterOutcome::DeliveryAmbiguous;
    }
    if let Some(runtime) = runtime.upgrade() {
        runtime.record_activity();
    }
    WriterOutcome::Written
}

fn write_failure(written: usize) -> WriterOutcome {
    if written == 0 {
        WriterOutcome::FailedBeforeDelivery
    } else {
        WriterOutcome::DeliveryAmbiguous
    }
}

fn reader_loop(runtime: &Weak<SessionRuntime>, mut reader: Box<dyn Read + Send>) {
    let Some(runtime_ref) = runtime.upgrade() else {
        return;
    };
    let mut buffer = vec![0_u8; runtime_ref.config().read_chunk_bytes];
    drop(runtime_ref);
    loop {
        match read_once(reader.as_mut(), &mut buffer) {
            ReadOutcome::Bytes(read) => record_read(runtime, &buffer[..read]),
            ReadOutcome::Interrupted => {}
            ReadOutcome::Terminated(_) => break,
        }
    }
    finish_reader(runtime);
}

fn preattached_reader_loop(
    mut reader: Box<dyn Read + Send>,
    read_chunk_bytes: usize,
    runtime_receiver: &mpsc::Receiver<Weak<SessionRuntime>>,
) {
    let mut buffer = vec![0_u8; read_chunk_bytes];
    let first_read = loop {
        match read_once(reader.as_mut(), &mut buffer) {
            ReadOutcome::Interrupted => {}
            result => break result,
        }
    };
    let Ok(runtime) = runtime_receiver.recv() else {
        return;
    };
    match first_read {
        ReadOutcome::Bytes(read) => {
            record_read(&runtime, &buffer[..read]);
            reader_loop(&runtime, reader);
        }
        ReadOutcome::Terminated(_) => finish_reader(&runtime),
        ReadOutcome::Interrupted => unreachable!("interrupted reads are retried above"),
    }
}

fn finish_reader(runtime: &Weak<SessionRuntime>) {
    if let Some(runtime) = runtime.upgrade() {
        runtime.mark_reader_finished();
        runtime.reader_stream_ended();
    }
}

fn record_read(runtime: &Weak<SessionRuntime>, bytes: &[u8]) {
    if let Some(runtime) = runtime.upgrade() {
        runtime.record_output(bytes.to_vec());
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReadTermination {
    Eof,
    Eio,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReadOutcome {
    Bytes(usize),
    Interrupted,
    Terminated(ReadTermination),
}

fn read_once(reader: &mut dyn Read, buffer: &mut [u8]) -> ReadOutcome {
    match reader.read(buffer) {
        Ok(0) => ReadOutcome::Terminated(ReadTermination::Eof),
        Ok(read) => ReadOutcome::Bytes(read),
        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => ReadOutcome::Interrupted,
        // Unix PTY masters commonly report EIO after the final slave
        // descriptor closes; it is the platform's EOF indication.
        Err(error) if error.raw_os_error() == Some(Errno::EIO as i32) => {
            ReadOutcome::Terminated(ReadTermination::Eio)
        }
        Err(_) => ReadOutcome::Terminated(ReadTermination::Error),
    }
}

fn supervisor_loop(runtime: &Weak<SessionRuntime>) {
    loop {
        let Some(runtime_ref) = runtime.upgrade() else {
            return;
        };
        if !runtime_ref.supervise_once() {
            return;
        }
        thread::sleep(runtime_ref.config().supervisor_interval);
    }
}

#[cfg(test)]
mod tests;
