use std::time::Duration;

/// Tunables for PTY I/O, replay, and process-group shutdown.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionManagerConfig {
    /// Maximum retained replay bytes per live session.
    pub replay_buffer_bytes: usize,
    /// Maximum time to wait before flushing a non-empty output batch.
    pub output_batch_window: Duration,
    /// Maximum batched output size before an immediate flush.
    pub output_batch_bytes: usize,
    /// Capacity of the per-session live output broadcast queue.
    pub subscriber_capacity: usize,
    /// Capacity of the manager-wide event broadcast queue.
    pub event_capacity: usize,
    /// Time without PTY activity before `running` becomes `idle`.
    pub idle_after: Duration,
    /// How often the idle scanner inspects live sessions.
    pub idle_scan: Duration,
    /// Time to wait after SIGINT before escalating to SIGTERM.
    pub interrupt_timeout: Duration,
    /// Time to wait after SIGTERM before escalating to SIGKILL.
    pub terminate_timeout: Duration,
    /// Time to wait after SIGKILL before giving up on `wait`.
    pub kill_timeout: Duration,
    /// Bounded queue of pending PTY writes.
    pub writer_queue: usize,
    /// Bounded queue between the reader thread and the batcher.
    pub reader_queue: usize,
}

impl SessionManagerConfig {
    /// Values used by integration tests that should not wait on production
    /// grace periods.
    #[must_use]
    pub fn for_tests() -> Self {
        Self {
            replay_buffer_bytes: 1024 * 1024,
            output_batch_window: Duration::from_millis(8),
            output_batch_bytes: 32 * 1024,
            subscriber_capacity: 64,
            event_capacity: 256,
            idle_after: Duration::from_millis(250),
            idle_scan: Duration::from_millis(50),
            interrupt_timeout: Duration::from_millis(400),
            terminate_timeout: Duration::from_millis(400),
            kill_timeout: Duration::from_millis(400),
            writer_queue: 32,
            reader_queue: 64,
        }
    }
}

impl Default for SessionManagerConfig {
    fn default() -> Self {
        Self {
            replay_buffer_bytes: 8 * 1024 * 1024,
            output_batch_window: Duration::from_millis(8),
            output_batch_bytes: 32 * 1024,
            subscriber_capacity: 64,
            event_capacity: 256,
            idle_after: Duration::from_secs(10),
            idle_scan: Duration::from_secs(1),
            interrupt_timeout: Duration::from_secs(2),
            terminate_timeout: Duration::from_secs(2),
            kill_timeout: Duration::from_secs(1),
            writer_queue: 32,
            reader_queue: 64,
        }
    }
}
