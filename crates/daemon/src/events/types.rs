use std::fmt;
use std::sync::Arc;

use uuid::Uuid;

use crate::events::fanout::FanoutEvent;
use crate::events::queue::ClientQueue;

/// Default replay and subscriber limits from the Beta architecture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EventBusLimits {
    /// Maximum decoded bytes retained per session.
    pub replay_max_bytes: usize,
    /// Maximum retained chunks per session.
    pub replay_max_chunks: usize,
    /// Maximum live events queued for one client.
    pub client_queue_capacity: usize,
}

impl Default for EventBusLimits {
    fn default() -> Self {
        Self {
            replay_max_bytes: 8 * 1024 * 1024,
            replay_max_chunks: 8_192,
            client_queue_capacity: 64,
        }
    }
}

/// Opaque identifier for one Unix-socket client lifetime.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ClientId(Uuid);

impl ClientId {
    /// Allocates a new client identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for ClientId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ClientId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Handle a client uses to receive live events after subscribe.
#[derive(Clone, Debug)]
pub struct ClientHandle {
    /// Client identity used for subscribe and lag diagnostics.
    pub id: ClientId,
    pub(crate) queue: Arc<ClientQueue>,
}

impl ClientHandle {
    pub(crate) fn new(id: ClientId, queue: Arc<ClientQueue>) -> Self {
        Self { id, queue }
    }

    pub(crate) fn queue(&self) -> &Arc<ClientQueue> {
        &self.queue
    }

    /// Pops the next live event if one is queued.
    #[must_use]
    pub fn try_recv(&self) -> Option<FanoutEvent> {
        self.queue.try_recv()
    }

    /// Drains every currently queued live event.
    #[must_use]
    pub fn drain(&self) -> Vec<FanoutEvent> {
        self.queue.drain()
    }

    /// Notifies when live events may be available.
    #[must_use]
    pub fn notify(&self) -> &tokio::sync::Notify {
        self.queue.notify()
    }
}

/// Replay snapshot produced while holding the session lock.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubscribeOutcome {
    /// Ordered replay, gap, and replay-complete events. Delivered before live.
    pub replay: Vec<FanoutEvent>,
    /// Whether later output is pushed onto the client's live queue.
    pub attaches_live: bool,
}

/// Failure while subscribing to a session stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubscribeError {
    /// No stream exists for this session yet.
    SessionNotFound,
}
