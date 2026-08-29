use std::time::Instant;

use cli_master_core::SessionStatus;

use crate::{
    IoOperation, ReconnectSnapshot, SessionError, SessionEvent, SessionSnapshot,
    StatusChangeReason,
    state::{IoAccess, RuntimeState, is_live, is_terminal, unix_epoch_ms},
};

use super::SessionRuntime;

impl SessionRuntime {
    pub(crate) fn snapshot(&self) -> SessionSnapshot {
        self.refresh_idle_status();
        self.snapshot_locked(&self.state.lock())
    }

    pub(crate) fn reconnect(
        &self,
        after_sequence: u64,
    ) -> Result<crate::SessionSubscription, SessionError> {
        let receiver = self.events.subscribe();
        self.refresh_idle_status();

        // `record_output` takes these locks in the same order. Consequently an
        // output chunk is either present in this replay view or delivered to
        // the receiver registered above (and may safely appear in both).
        let state = self.state.lock();
        let replay = self.replay.lock().view(self.id, after_sequence)?;
        let snapshot = ReconnectSnapshot {
            session: self.snapshot_locked(&state),
            output: replay.chunks,
            first_available_sequence: replay.first_available_sequence,
            next_sequence: replay.next_sequence,
            gap: replay.gap,
        };
        Ok(crate::SessionSubscription { snapshot, receiver })
    }

    pub(super) fn ensure_live(&self) -> Result<(), SessionError> {
        self.refresh_idle_status();
        let state = self.state.lock();
        if !is_live(state.status) {
            return Err(SessionError::NotLive {
                session_id: self.id,
                status: state.status,
            });
        }
        if !state.io_access.accepts_input() {
            return Err(SessionError::InteractionUnavailable {
                session_id: self.id,
            });
        }
        Ok(())
    }

    pub(crate) fn record_activity(&self) {
        let mut state = self.state.lock();
        if !is_live(state.status) || !state.io_access.accepts_input() {
            return;
        }
        let now_ms = unix_epoch_ms();
        state.last_activity_at_ms = now_ms;
        state.last_activity_instant = Instant::now();
        if state.status == SessionStatus::Idle {
            set_status_and_publish(
                &self.events,
                self.id,
                &mut state,
                SessionStatus::Running,
                now_ms,
                StatusChangeReason::Activity,
            );
        }
    }

    pub(crate) fn record_output(&self, bytes: Vec<u8>) {
        let replay_failed = {
            let mut state = self.state.lock();
            if !state.io_access.accepts_output() || is_terminal(state.status) {
                return;
            }

            let occurred_at_ms = unix_epoch_ms();
            if let Ok(chunk) = self.replay.lock().append(self.id, bytes, occurred_at_ms) {
                state.last_activity_at_ms = occurred_at_ms;
                state.last_activity_instant = Instant::now();
                if state.status == SessionStatus::Idle && !state.stop_requested {
                    set_status_and_publish(
                        &self.events,
                        self.id,
                        &mut state,
                        SessionStatus::Running,
                        occurred_at_ms,
                        StatusChangeReason::Activity,
                    );
                }
                self.emit_locked(SessionEvent::Output(chunk));
                false
            } else {
                state.io_access = IoAccess::Closed;
                true
            }
        };

        if replay_failed {
            self.supervision_lost();
        }
    }

    pub(super) fn refresh_idle_status(&self) {
        let mut state = self.state.lock();
        if state.status != SessionStatus::Running
            || state.stop_requested
            || !state.io_access.accepts_input()
            || state.last_activity_instant.elapsed() < self.config.idle_after
        {
            return;
        }
        set_status_and_publish(
            &self.events,
            self.id,
            &mut state,
            SessionStatus::Idle,
            unix_epoch_ms(),
            StatusChangeReason::IdleTimeout,
        );
    }

    pub(crate) fn mark_reader_finished(&self) {
        self.state.lock().reader_finished = true;
        self.state_changed.notify_all();
    }

    pub(super) fn finish_exit(&self, exit_code: i32, success: bool) {
        self.close_writer();
        self.wait_for_reader_drain();
        self.master.lock().take();

        let mut state = self.state.lock();
        if is_terminal(state.status) {
            return;
        }
        let status = if success || state.stop_requested {
            SessionStatus::Exited
        } else {
            SessionStatus::Failed
        };
        let reason = if state.stop_requested {
            StatusChangeReason::StopRequested
        } else {
            StatusChangeReason::ProcessExited
        };
        let occurred_at_ms = unix_epoch_ms();
        state.io_access = IoAccess::Closed;
        state.exit_code = Some(exit_code);
        set_status_and_publish(
            &self.events,
            self.id,
            &mut state,
            status,
            occurred_at_ms,
            reason,
        );
        self.emit_locked(SessionEvent::Exited {
            session_id: self.id,
            status,
            exit_code,
            occurred_at_ms,
        });
        self.state_changed.notify_all();
    }

    pub(super) fn finish_unknown(&self, exit_code: Option<i32>) {
        self.wait_for_reader_drain();
        self.master.lock().take();

        let mut state = self.state.lock();
        if is_terminal(state.status) {
            return;
        }
        state.io_access = IoAccess::Closed;
        state.exit_code = exit_code;
        set_status_and_publish(
            &self.events,
            self.id,
            &mut state,
            SessionStatus::Unknown,
            unix_epoch_ms(),
            StatusChangeReason::SupervisionLost,
        );
        self.state_changed.notify_all();
    }

    pub(super) fn seal_interaction(&self, close_output: bool) {
        let mut state = self.state.lock();
        state.io_access = if close_output {
            IoAccess::Closed
        } else {
            IoAccess::InputClosed
        };
    }

    pub(super) fn mark_stop_requested(&self) {
        let mut state = self.state.lock();
        state.stop_requested = true;
        state.io_access = IoAccess::InputClosed;
    }

    pub(super) fn wait_for_reader_drain(&self) {
        let mut state = self.state.lock();
        if !state.reader_finished {
            self.state_changed
                .wait_for(&mut state, self.config.output_drain_timeout);
        }
    }

    pub(super) fn is_terminal(&self) -> bool {
        is_terminal(self.state.lock().status)
    }

    pub(super) fn supervision_lost(&self) {
        self.invalidate_supervision();
    }

    pub(super) fn emit_io_failure(&self, operation: IoOperation) {
        let mut state = self.state.lock();
        if is_terminal(state.status) || state.io_failure_reported {
            return;
        }
        state.io_failure_reported = true;
        self.emit_locked(SessionEvent::IoFailure {
            session_id: self.id,
            operation,
            occurred_at_ms: unix_epoch_ms(),
        });
    }

    pub(crate) fn writer_stream_failed(&self) {
        self.emit_io_failure(IoOperation::Write);
        self.invalidate_supervision();
    }

    pub(super) fn emit_locked(&self, event: SessionEvent) {
        let _ = self.events.send(event);
    }

    fn snapshot_locked(&self, state: &RuntimeState) -> SessionSnapshot {
        SessionSnapshot {
            id: self.id,
            pid: snapshot_pid(state.status, self.pid),
            status: state.status,
            exit_code: state.exit_code,
            created_at_ms: state.created_at_ms,
            last_activity_at_ms: state.last_activity_at_ms,
            terminal_size: state.terminal_size,
        }
    }
}

fn snapshot_pid(status: SessionStatus, pid: Option<u32>) -> Option<u32> {
    is_live(status).then_some(pid).flatten()
}

fn set_status_and_publish(
    events: &tokio::sync::broadcast::Sender<SessionEvent>,
    session_id: cli_master_core::SessionId,
    state: &mut RuntimeState,
    current: SessionStatus,
    occurred_at_ms: i64,
    reason: StatusChangeReason,
) {
    set_status_and_publish_after(
        events,
        session_id,
        state,
        current,
        occurred_at_ms,
        reason,
        || {},
    );
}

fn set_status_and_publish_after<F>(
    events: &tokio::sync::broadcast::Sender<SessionEvent>,
    session_id: cli_master_core::SessionId,
    state: &mut RuntimeState,
    current: SessionStatus,
    occurred_at_ms: i64,
    reason: StatusChangeReason,
    after_mutation: F,
) where
    F: FnOnce(),
{
    let previous = state.status;
    state.status = current;
    after_mutation();
    let _ = events.send(SessionEvent::StatusChanged {
        session_id,
        previous,
        current,
        occurred_at_ms,
        reason,
    });
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier, mpsc};

    use parking_lot::Mutex;
    use tokio::sync::broadcast::error::TryRecvError;

    use super::*;
    use crate::TerminalSize;

    #[test]
    fn snapshots_only_expose_a_pid_for_live_states() {
        let pid = Some(42);

        for status in [
            SessionStatus::Starting,
            SessionStatus::Running,
            SessionStatus::Idle,
        ] {
            assert_eq!(snapshot_pid(status, pid), pid);
        }
        for status in [
            SessionStatus::Exited,
            SessionStatus::Failed,
            SessionStatus::Unknown,
        ] {
            assert_eq!(snapshot_pid(status, pid), None);
        }
    }

    #[test]
    fn status_mutation_and_publication_are_one_ordered_critical_section() {
        let session_id = cli_master_core::SessionId::new();
        let (events, _) = tokio::sync::broadcast::channel(8);
        let mut receiver = events.subscribe();
        let state = Arc::new(Mutex::new(RuntimeState {
            status: SessionStatus::Running,
            exit_code: None,
            created_at_ms: 0,
            last_activity_at_ms: 0,
            last_activity_instant: Instant::now(),
            terminal_size: TerminalSize::default(),
            stop_requested: false,
            reader_finished: false,
            io_access: IoAccess::Open,
            io_failure_reported: false,
        }));
        let idle_mutated = Arc::new(Barrier::new(2));
        let publish_idle = Arc::new(Barrier::new(2));

        let idle_state = Arc::clone(&state);
        let idle_events = events.clone();
        let idle_mutated_thread = Arc::clone(&idle_mutated);
        let publish_idle_thread = Arc::clone(&publish_idle);
        let idle = std::thread::spawn(move || {
            let mut state = idle_state.lock();
            set_status_and_publish_after(
                &idle_events,
                session_id,
                &mut state,
                SessionStatus::Idle,
                1,
                StatusChangeReason::IdleTimeout,
                || {
                    idle_mutated_thread.wait();
                    publish_idle_thread.wait();
                },
            );
        });

        idle_mutated.wait();
        let terminal_state = Arc::clone(&state);
        let terminal_events = events.clone();
        let (attempted, attempted_receiver) = mpsc::sync_channel(0);
        let terminal = std::thread::spawn(move || {
            attempted.send(()).expect("test coordinator should remain");
            let mut state = terminal_state.lock();
            set_status_and_publish(
                &terminal_events,
                session_id,
                &mut state,
                SessionStatus::Exited,
                2,
                StatusChangeReason::ProcessExited,
            );
        });
        attempted_receiver
            .recv()
            .expect("terminal contender should reach the state lock");
        assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));

        publish_idle.wait();
        idle.join().expect("idle publisher should finish");
        terminal.join().expect("terminal publisher should finish");

        let statuses = [receiver.try_recv(), receiver.try_recv()]
            .into_iter()
            .map(
                |event| match event.expect("both transitions should be published") {
                    SessionEvent::StatusChanged { current, .. } => current,
                    event => panic!("unexpected event: {event:?}"),
                },
            )
            .collect::<Vec<_>>();
        assert_eq!(statuses, [SessionStatus::Idle, SessionStatus::Exited]);
    }
}
