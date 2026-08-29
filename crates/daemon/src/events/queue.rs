use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use tokio::sync::Notify;

use super::fanout::FanoutEvent;

/// Bounded per-client live queue. Push never waits.
#[derive(Debug)]
pub struct ClientQueue {
    capacity: usize,
    events: Mutex<VecDeque<FanoutEvent>>,
    notify: Notify,
}

impl ClientQueue {
    pub fn new(capacity: usize) -> Arc<Self> {
        Arc::new(Self {
            capacity: capacity.max(1),
            events: Mutex::new(VecDeque::new()),
            notify: Notify::new(),
        })
    }

    /// Returns `false` when the client is behind and the queue was replaced
    /// with `gap` so the publisher can continue.
    pub fn try_push(&self, event: FanoutEvent) -> bool {
        let mut guard = self
            .events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if guard.len() >= self.capacity {
            return false;
        }
        guard.push_back(event);
        drop(guard);
        self.notify.notify_waiters();
        true
    }

    /// Drops undelivered live events and enqueues a single gap marker.
    pub fn replace_with_gap(&self, gap: FanoutEvent) {
        let mut guard = self
            .events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.clear();
        guard.push_back(gap);
        drop(guard);
        self.notify.notify_waiters();
    }

    pub fn try_recv(&self) -> Option<FanoutEvent> {
        self.events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pop_front()
    }

    pub fn drain(&self) -> Vec<FanoutEvent> {
        let mut guard = self
            .events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.drain(..).collect()
    }

    pub fn notify(&self) -> &Notify {
        &self.notify
    }
}
