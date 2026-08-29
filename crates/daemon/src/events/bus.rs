use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cli_master_core::wire::{
    DiagnosticIssue, OutputCursor, OutputSequence, SessionExitedEvent, SessionOutputEvent,
    SessionOutputGapEvent, SessionReplayCompleteEvent, SessionStatusChangedEvent,
};
use cli_master_core::{SessionId, SessionStatus};

use crate::diagnostics::DiagnosticLog;
use crate::events::buffer::ReplayBuffer;
use crate::events::encode::{encode_output_chunk, split_output};
use crate::events::fanout::FanoutEvent;
use crate::events::queue::ClientQueue;
use crate::events::types::{
    ClientHandle, ClientId, EventBusLimits, SubscribeError, SubscribeOutcome,
};

/// In-memory session event bus with bounded replay and per-client queues.
#[derive(Clone, Debug)]
pub struct EventBus {
    limits: EventBusLimits,
    diagnostics: DiagnosticLog,
    envelope_seq: Arc<AtomicU64>,
    inner: Arc<Mutex<HashMap<SessionId, SessionStream>>>,
}

#[derive(Debug)]
struct SessionStream {
    next_output: u64,
    terminated: bool,
    buffer: ReplayBuffer,
    subscribers: Vec<SessionSubscriber>,
}

#[derive(Clone, Debug)]
struct SessionSubscriber {
    client_id: ClientId,
    last_output: u64,
    live: Arc<ClientQueue>,
}

impl EventBus {
    /// Creates a bus with default Beta limits.
    #[must_use]
    pub fn new(diagnostics: DiagnosticLog) -> Self {
        Self::with_limits(EventBusLimits::default(), diagnostics)
    }

    /// Creates a bus with explicit replay and queue limits.
    #[must_use]
    pub fn with_limits(limits: EventBusLimits, diagnostics: DiagnosticLog) -> Self {
        Self {
            limits,
            diagnostics,
            envelope_seq: Arc::new(AtomicU64::new(0)),
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Limits used when connecting clients and retaining replay.
    #[must_use]
    pub const fn limits(&self) -> EventBusLimits {
        self.limits
    }

    /// Diagnostic log shared with `diagnostics.get`.
    #[must_use]
    pub fn diagnostics(&self) -> &DiagnosticLog {
        &self.diagnostics
    }

    /// Allocates a client live queue that cannot block publishers.
    #[must_use]
    pub fn connect_client(&self) -> ClientHandle {
        ClientHandle::new(
            ClientId::new(),
            ClientQueue::new(self.limits.client_queue_capacity),
        )
    }

    /// Opens an empty stream for `session_id`, replacing any previous lifetime.
    pub fn open_session(&self, session_id: SessionId) {
        lock(&self.inner).insert(session_id, new_stream(self.limits));
    }

    /// Clears replay and output sequence as a new PTY lifetime.
    pub fn reset_lifetime(&self, session_id: SessionId) {
        if let Some(session) = lock(&self.inner).get_mut(&session_id) {
            *session = new_stream(self.limits);
        }
    }

    /// Drops a session stream and its subscribers.
    pub fn close_session(&self, session_id: SessionId) {
        lock(&self.inner).remove(&session_id);
    }

    /// Removes a disconnected client from every session.
    pub fn disconnect_client(&self, client_id: ClientId) {
        for session in lock(&self.inner).values_mut() {
            session
                .subscribers
                .retain(|subscriber| subscriber.client_id != client_id);
        }
    }

    /// Publishes terminal bytes as one or more sequenced output events.
    pub fn publish_output(&self, session_id: SessionId, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let chunks: Vec<Vec<u8>> = split_output(bytes)
            .into_iter()
            .map(<[u8]>::to_vec)
            .collect();
        let mut sessions = lock(&self.inner);
        let Some(session) = sessions.get_mut(&session_id) else {
            self.diagnose(
                "session_stream_missing",
                "Output dropped because the session stream is closed",
            );
            return;
        };
        if session.terminated {
            self.diagnose(
                "output_after_exit",
                "Output dropped because the session already emitted an exit event",
            );
            return;
        }
        for chunk in chunks {
            match encode_output_chunk(&chunk) {
                Ok(base64) => {
                    let sequence = session.next_output;
                    session.next_output = session.next_output.saturating_add(1);
                    session.buffer.push(sequence, chunk);
                    let event = FanoutEvent::output(
                        self.next_envelope(),
                        SessionOutputEvent {
                            session_id,
                            base64,
                            output_sequence: OutputSequence::new(sequence),
                            replay: false,
                        },
                    );
                    self.fanout_output(session_id, session, &event, sequence);
                }
                Err(_) => {
                    self.diagnose(
                        "output_encode_failed",
                        "A terminal chunk could not be encoded for IPC",
                    );
                }
            }
        }
    }

    /// Publishes a lifecycle transition. Does not mark the stream terminal.
    pub fn publish_status(
        &self,
        session_id: SessionId,
        previous_status: SessionStatus,
        status: SessionStatus,
        changed_at_ms: i64,
        reason_code: Option<String>,
    ) {
        let mut sessions = lock(&self.inner);
        let Some(session) = sessions.get_mut(&session_id) else {
            self.diagnose(
                "session_stream_missing",
                "Status event dropped because the session stream is closed",
            );
            return;
        };
        let event = FanoutEvent::status(
            self.next_envelope(),
            SessionStatusChangedEvent {
                session_id,
                previous_status,
                status,
                changed_at_ms,
                reason_code,
            },
        );
        self.fanout_control(session_id, session, &event);
    }

    /// Publishes the terminal exit event. Later output is ignored.
    pub fn publish_exit(
        &self,
        session_id: SessionId,
        exit_code: Option<i32>,
        status: SessionStatus,
        exited_at_ms: i64,
    ) {
        let mut sessions = lock(&self.inner);
        let Some(session) = sessions.get_mut(&session_id) else {
            self.diagnose(
                "session_stream_missing",
                "Exit event dropped because the session stream is closed",
            );
            return;
        };
        session.terminated = true;
        let event = FanoutEvent::exited(
            self.next_envelope(),
            SessionExitedEvent {
                session_id,
                exit_code,
                status,
                exited_at_ms,
            },
        );
        self.fanout_control(session_id, session, &event);
        session.subscribers.clear();
    }

    /// Replays retained output after `cursor`, then follows live events.
    ///
    /// # Errors
    ///
    /// Returns [`SubscribeError::SessionNotFound`] when the session was never opened.
    pub fn subscribe(
        &self,
        client: &ClientHandle,
        session_id: SessionId,
        cursor: Option<OutputCursor>,
    ) -> Result<SubscribeOutcome, SubscribeError> {
        let mut sessions = lock(&self.inner);
        let Some(session) = sessions.get_mut(&session_id) else {
            return Err(SubscribeError::SessionNotFound);
        };
        session
            .subscribers
            .retain(|subscriber| subscriber.client_id != client.id);

        let latest = session.buffer.latest_sequence().unwrap_or(0);
        let first = session.buffer.first_sequence().unwrap_or(1);
        let requested = cursor.map_or(0, OutputCursor::get);

        if cursor.is_some() && !cursor_is_replayable(requested, first, latest) {
            return Ok(SubscribeOutcome {
                replay: vec![self.gap_event(session_id, requested, first, latest)],
                attaches_live: false,
            });
        }

        let mut replay = Vec::new();
        for chunk in session.buffer.after(requested) {
            match encode_output_chunk(&chunk.bytes) {
                Ok(base64) => {
                    replay.push(FanoutEvent::output(
                        self.next_envelope(),
                        SessionOutputEvent {
                            session_id,
                            base64,
                            output_sequence: OutputSequence::new(chunk.sequence),
                            replay: true,
                        },
                    ));
                }
                Err(_) => {
                    self.diagnose(
                        "replay_encode_failed",
                        "A retained terminal chunk could not be encoded for replay",
                    );
                }
            }
        }
        replay.push(FanoutEvent::replay_complete(
            self.next_envelope(),
            SessionReplayCompleteEvent {
                session_id,
                output_sequence: OutputSequence::new(latest),
            },
        ));

        if !session.terminated {
            session.subscribers.push(SessionSubscriber {
                client_id: client.id,
                last_output: latest.max(requested),
                live: Arc::clone(client.queue()),
            });
        }

        Ok(SubscribeOutcome {
            replay,
            attaches_live: !session.terminated,
        })
    }

    /// Stops live delivery for one session on one client.
    pub fn unsubscribe(&self, client_id: ClientId, session_id: SessionId) {
        if let Some(session) = lock(&self.inner).get_mut(&session_id) {
            session
                .subscribers
                .retain(|subscriber| subscriber.client_id != client_id);
        }
    }

    fn next_envelope(&self) -> u64 {
        self.envelope_seq.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn gap_event(
        &self,
        session_id: SessionId,
        requested: u64,
        first: u64,
        latest: u64,
    ) -> FanoutEvent {
        FanoutEvent::gap(
            self.next_envelope(),
            SessionOutputGapEvent {
                session_id,
                requested_cursor: OutputCursor::new(requested),
                first_available_sequence: OutputSequence::new(first),
                latest_sequence: OutputSequence::new(latest),
            },
        )
    }

    fn fanout_output(
        &self,
        session_id: SessionId,
        session: &mut SessionStream,
        event: &FanoutEvent,
        sequence: u64,
    ) {
        let mut lagged = Vec::new();
        session.subscribers.retain_mut(|subscriber| {
            if subscriber.live.try_push(event.clone()) {
                subscriber.last_output = sequence;
                true
            } else {
                lagged.push(subscriber.clone());
                false
            }
        });
        let first = session.buffer.first_sequence().unwrap_or(1);
        for subscriber in lagged {
            subscriber.live.replace_with_gap(self.gap_event(
                session_id,
                subscriber.last_output,
                first,
                sequence,
            ));
            self.diagnose(
                "subscriber_lagged",
                "A client fell behind terminal output and must resubscribe",
            );
        }
    }

    fn fanout_control(
        &self,
        session_id: SessionId,
        session: &mut SessionStream,
        event: &FanoutEvent,
    ) {
        let latest = session.buffer.latest_sequence().unwrap_or(0);
        let first = session.buffer.first_sequence().unwrap_or(1);
        session.subscribers.retain(|subscriber| {
            if subscriber.live.try_push(event.clone()) {
                true
            } else {
                subscriber.live.replace_with_gap(self.gap_event(
                    session_id,
                    subscriber.last_output,
                    first,
                    latest,
                ));
                self.diagnose(
                    "subscriber_lagged",
                    "A client fell behind session events and must resubscribe",
                );
                false
            }
        });
    }

    fn diagnose(&self, code: &str, message: &str) {
        self.diagnostics.record(DiagnosticIssue {
            code: code.to_owned(),
            message: message.to_owned(),
            action: Some(
                "Resubscribe from a retained cursor or inspect daemon diagnostics".to_owned(),
            ),
        });
    }
}

fn new_stream(limits: EventBusLimits) -> SessionStream {
    SessionStream {
        next_output: 1,
        terminated: false,
        buffer: ReplayBuffer::new(limits.replay_max_bytes, limits.replay_max_chunks),
        subscribers: Vec::new(),
    }
}

fn cursor_is_replayable(requested: u64, first: u64, latest: u64) -> bool {
    if requested > latest {
        false
    } else if requested == 0 {
        true
    } else {
        requested + 1 >= first
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
