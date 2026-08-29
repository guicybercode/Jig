use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use nix::sys::signal::Signal;
use portable_pty::ExitStatus;

use crate::{
    IoOperation, SessionError, SessionSnapshot,
    worker::{WorkerHandle, WriterCommand},
};

use super::{ChildPoll, SessionRuntime};

const INTERRUPT_BYTE: u8 = 0x03;
const REAP_POLL_INTERVAL: Duration = Duration::from_millis(5);

impl SessionRuntime {
    pub(crate) fn stop(&self) -> Result<SessionSnapshot, SessionError> {
        self.mark_stop_requested();
        // Ctrl+C can reap the leader and reparent a child that moved to a new
        // process group. Capture ancestry first so later rescans retain proof.
        self.tracked_processes()?;
        if !self.is_terminal() {
            let _ = self.write_with_deadline(vec![INTERRUPT_BYTE]);
        }

        let _lifecycle = self.lifecycle.lock();
        if self.is_terminal() {
            self.terminate_remaining_process_group()?;
            return Ok(self.snapshot());
        }
        if let Some(exit_status) = self.reap_leader_until(self.config.interrupt_grace)? {
            self.finalize_graceful_exit(&exit_status)?;
            return Ok(self.snapshot());
        }

        if let Err(error) = self.signal_process_group(Signal::SIGHUP, "SIGHUP") {
            self.invalidate_supervision_locked(None);
            return Err(error);
        }
        if let Some(exit_status) = self.reap_leader_until(self.config.hangup_grace)? {
            self.finalize_graceful_exit(&exit_status)?;
            return Ok(self.snapshot());
        }

        self.force_kill_locked()
    }

    pub(crate) fn kill(&self) -> Result<SessionSnapshot, SessionError> {
        self.mark_stop_requested();
        let _lifecycle = self.lifecycle.lock();
        self.force_kill_locked()
    }

    pub(crate) fn close_and_join_workers(&self) -> Result<(), SessionError> {
        let mut first_error = self.terminate_for_cleanup().err();
        self.close_writer();
        self.master.lock().take();

        let workers = std::mem::take(&mut *self.workers.lock());
        let mut pending = Vec::new();
        for worker in workers {
            match join_worker(worker, self.config.kill_wait) {
                WorkerJoin::Complete => {}
                WorkerJoin::Pending(worker) => {
                    if first_error.is_none() {
                        first_error = Some(SessionError::WorkerJoinTimedOut { role: worker.role });
                    }
                    pending.push(worker);
                }
                WorkerJoin::Panicked { role } => {
                    if first_error.is_none() {
                        first_error = Some(SessionError::WorkerPanicked { role });
                    }
                }
            }
        }
        self.workers.lock().extend(pending);

        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    pub(crate) fn best_effort_terminate(&self) {
        let _ = self.stop();
        let _ = self.terminate_for_cleanup();
        self.close_writer();
        self.master.lock().take();
    }

    pub(super) fn cleanup_failed_start(&self) {
        self.mark_stop_requested();
        let _ = self.terminate_for_cleanup();
        self.close_writer();
        self.master.lock().take();
        let _ = self.close_and_join_workers();
    }

    pub(super) fn close_writer(&self) {
        if let Some(writer) = self.writer_sender.lock().take() {
            let _ = writer.try_send(WriterCommand::Close);
        }
    }

    pub(crate) fn supervise_once(&self) -> bool {
        self.refresh_idle_status();
        let _lifecycle = self.lifecycle.lock();
        if self.is_terminal() {
            return false;
        }
        if self.cached_tracked_processes().is_err() {
            self.invalidate_supervision_locked(None);
            return false;
        }
        match self.poll_child_exit() {
            Ok(ChildPoll::Running) => true,
            Ok(ChildPoll::Exited(exit_status)) => {
                let _ = self.finalize_graceful_exit(&exit_status);
                false
            }
            Ok(ChildPoll::Reaped | ChildPoll::Unavailable) | Err(_) => {
                self.invalidate_supervision_locked(None);
                false
            }
        }
    }

    pub(crate) fn reader_stream_ended(&self) {
        let _lifecycle = self.lifecycle.lock();
        if self.is_terminal() {
            return;
        }
        if let Some(exit_status) =
            self.reap_leader_until_without_invalidation(self.config.output_drain_timeout)
        {
            if self.finalize_graceful_exit(&exit_status).is_err() && !self.is_terminal() {
                self.invalidate_supervision_locked(Some(exit_code(&exit_status)));
            }
            return;
        }
        self.emit_io_failure(IoOperation::Read);
        self.invalidate_supervision_locked(None);
    }

    pub(super) fn invalidate_after_ambiguous_write(&self) {
        self.emit_io_failure(IoOperation::Write);
        self.invalidate_supervision();
    }

    pub(super) fn invalidate_supervision(&self) {
        self.seal_interaction(true);
        let _lifecycle = self.lifecycle.lock();
        self.invalidate_supervision_locked(None);
    }

    fn force_kill_locked(&self) -> Result<SessionSnapshot, SessionError> {
        if let Err(error) = self.signal_process_group(Signal::SIGKILL, "SIGKILL") {
            self.invalidate_supervision_locked(None);
            return Err(error);
        }
        if self.is_terminal() {
            self.confirm_process_group_exit(self.config.kill_wait)?;
            return Ok(self.snapshot());
        }
        let Some(exit_status) = self.reap_leader_until(self.config.kill_wait)? else {
            self.retire_process_group();
            self.invalidate_supervision_locked(None);
            return Err(SessionError::StopTimedOut {
                session_id: self.id,
            });
        };
        if let Err(error) = self.confirm_process_group_exit(self.config.kill_wait) {
            self.invalidate_supervision_locked(Some(exit_code(&exit_status)));
            return Err(error);
        }
        self.finish_exit_status(&exit_status);
        Ok(self.snapshot())
    }

    fn finalize_graceful_exit(&self, exit_status: &ExitStatus) -> Result<(), SessionError> {
        if let Err(error) = self.terminate_remaining_process_group() {
            self.invalidate_supervision_locked(Some(exit_code(exit_status)));
            return Err(error);
        }
        self.finish_exit_status(exit_status);
        Ok(())
    }

    fn finish_exit_status(&self, exit_status: &ExitStatus) {
        self.finish_exit(exit_code(exit_status), exit_status.success());
    }

    fn terminate_remaining_process_group(&self) -> Result<(), SessionError> {
        if !self.process_group_exists()? {
            return Ok(());
        }
        self.signal_process_group(Signal::SIGHUP, "SIGHUP")?;
        match self.wait_for_process_group_exit(self.config.hangup_grace) {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(error) => {
                let _ = self.signal_process_group(Signal::SIGKILL, "SIGKILL");
                self.retire_process_group();
                return Err(error);
            }
        }
        self.signal_process_group(Signal::SIGKILL, "SIGKILL")?;
        self.confirm_process_group_exit(self.config.kill_wait)
    }

    fn confirm_process_group_exit(&self, timeout: Duration) -> Result<(), SessionError> {
        match self.wait_for_process_group_exit(timeout) {
            Ok(true) => Ok(()),
            Ok(false) => {
                self.retire_process_group();
                Err(SessionError::StopTimedOut {
                    session_id: self.id,
                })
            }
            Err(error) => {
                self.retire_process_group();
                Err(error)
            }
        }
    }

    fn wait_for_process_group_exit(&self, timeout: Duration) -> Result<bool, SessionError> {
        let deadline = Instant::now() + timeout;
        loop {
            if !self.process_group_exists()? {
                return Ok(true);
            }
            let now = Instant::now();
            if now >= deadline {
                return Ok(false);
            }
            thread::sleep(REAP_POLL_INTERVAL.min(deadline - now));
        }
    }

    fn reap_leader_until(&self, timeout: Duration) -> Result<Option<ExitStatus>, SessionError> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.poll_child_exit() {
                Ok(ChildPoll::Exited(exit_status)) => return Ok(Some(exit_status)),
                Ok(ChildPoll::Running) => {}
                Ok(ChildPoll::Reaped | ChildPoll::Unavailable) => {
                    self.invalidate_supervision_locked(None);
                    return Ok(None);
                }
                Err(source) => {
                    self.invalidate_supervision_locked(None);
                    return Err(SessionError::Supervision {
                        session_id: self.id,
                        source,
                    });
                }
            }
            let now = Instant::now();
            if now >= deadline {
                return Ok(None);
            }
            thread::sleep(REAP_POLL_INTERVAL.min(deadline - now));
        }
    }

    fn terminate_for_cleanup(&self) -> Result<(), SessionError> {
        let _lifecycle = self.lifecycle.lock();
        if self.is_terminal() {
            return self.terminate_remaining_process_group();
        }
        let mut first_error = self.signal_process_group(Signal::SIGKILL, "SIGKILL").err();

        let mut observed_exit_code = None;
        if !self.leader.lock().reaped {
            match self.reap_leader_until(self.config.kill_wait) {
                Ok(Some(exit_status)) => {
                    observed_exit_code =
                        Some(i32::try_from(exit_status.exit_code()).unwrap_or(i32::MAX));
                }
                Ok(None) => {
                    if first_error.is_none() {
                        first_error = Some(SessionError::StopTimedOut {
                            session_id: self.id,
                        });
                    }
                }
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        if let Err(error) = self.confirm_process_group_exit(self.config.kill_wait) {
            if first_error.is_none() {
                first_error = Some(error);
            }
        }
        if !self.is_terminal() {
            self.invalidate_supervision_locked(observed_exit_code);
        }

        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn invalidate_supervision_locked(&self, exit_code: Option<i32>) {
        self.seal_interaction(true);
        let _ = self.signal_process_group(Signal::SIGKILL, "SIGKILL");
        let observed_exit_code = match self
            .reap_leader_until_without_invalidation(self.config.kill_wait)
        {
            Some(exit_status) => Some(i32::try_from(exit_status.exit_code()).unwrap_or(i32::MAX)),
            None => exit_code,
        };
        let _ = self.confirm_process_group_exit(self.config.kill_wait);
        self.close_writer();
        self.master.lock().take();
        self.finish_unknown(observed_exit_code);
    }

    fn reap_leader_until_without_invalidation(&self, timeout: Duration) -> Option<ExitStatus> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.poll_child_exit() {
                Ok(ChildPoll::Exited(exit_status)) => return Some(exit_status),
                Ok(ChildPoll::Running) => {}
                Ok(ChildPoll::Reaped | ChildPoll::Unavailable) | Err(_) => return None,
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            thread::sleep(REAP_POLL_INTERVAL.min(deadline - now));
        }
    }
}

enum WorkerJoin {
    Complete,
    Pending(WorkerHandle),
    Panicked { role: &'static str },
}

fn join_worker(worker: WorkerHandle, timeout: Duration) -> WorkerJoin {
    match worker.completed.recv_timeout(timeout) {
        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => {
            if worker.handle.join().is_err() {
                WorkerJoin::Panicked { role: worker.role }
            } else {
                WorkerJoin::Complete
            }
        }
        Err(mpsc::RecvTimeoutError::Timeout) => WorkerJoin::Pending(worker),
    }
}

fn exit_code(exit_status: &ExitStatus) -> i32 {
    i32::try_from(exit_status.exit_code()).unwrap_or(i32::MAX)
}
