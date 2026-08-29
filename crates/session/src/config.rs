use std::time::Duration;

use cli_master_core::wire::MAX_PTY_OUTPUT_BYTES;

use crate::SessionError;

/// Default maximum number of terminal bytes retained per session (8 MiB).
pub const DEFAULT_REPLAY_MAX_BYTES: usize = 8 * 1024 * 1024;
/// Default maximum number of terminal chunks retained per session.
pub const DEFAULT_REPLAY_MAX_CHUNKS: usize = 512;
/// Default number of live events buffered for each subscriber.
pub const DEFAULT_EVENT_CAPACITY: usize = 256;
/// Default maximum bytes read from a PTY in one operation.
pub const DEFAULT_READ_CHUNK_BYTES: usize = MAX_PTY_OUTPUT_BYTES;
/// Default maximum bytes accepted by one input call.
pub const DEFAULT_MAX_WRITE_BYTES: usize = 64 * 1024;
/// Upper bound accepted for a broadcast channel capacity.
pub const MAX_EVENT_CAPACITY: usize = 65_536;
/// Upper bound accepted for descendants retained by one session tracker.
pub const MAX_TRACKED_PROCESSES: usize = 65_536;

/// Resource limits and lifecycle deadlines for a [`crate::SessionManager`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionManagerConfig {
    /// Maximum terminal bytes retained for replay per session (8 MiB by default).
    pub replay_max_bytes: usize,
    /// Maximum terminal chunks retained for replay per session.
    pub replay_max_chunks: usize,
    /// Per-subscriber live event channel capacity.
    pub event_capacity: usize,
    /// Maximum number of bytes read from a PTY at once.
    ///
    /// This may not exceed [`MAX_PTY_OUTPUT_BYTES`], ensuring every emitted
    /// output chunk is representable by the core wire contract.
    pub read_chunk_bytes: usize,
    /// Maximum bytes accepted by one input call.
    pub max_write_bytes: usize,
    /// Number of pending input operations allowed per session.
    pub write_queue_capacity: usize,
    /// Deadline for a queued PTY write to complete.
    pub write_timeout: Duration,
    /// Inactivity period after which a live session is reported idle (10 seconds by default).
    pub idle_after: Duration,
    /// Interval between non-blocking child exit checks.
    pub supervisor_interval: Duration,
    /// Time allowed for terminal interrupt handling during a graceful stop.
    pub interrupt_grace: Duration,
    /// Time allowed after `SIGHUP` before the force-kill fallback.
    pub hangup_grace: Duration,
    /// Time allowed for the process to be reaped after `SIGKILL`.
    pub kill_wait: Duration,
    /// Time allowed for final PTY bytes to drain after process exit.
    pub output_drain_timeout: Duration,
    /// Time allowed for one operating-system process-tree snapshot.
    pub process_scan_timeout: Duration,
    /// Maximum proven descendants retained for one session.
    pub max_tracked_processes: usize,
}

impl SessionManagerConfig {
    /// Validates all bounded resources and non-zero lifecycle deadlines.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::InvalidConfiguration`] for the first invalid
    /// field.
    pub fn validate(&self) -> Result<(), SessionError> {
        validate_nonzero("replay_max_bytes", self.replay_max_bytes)?;
        validate_nonzero("replay_max_chunks", self.replay_max_chunks)?;
        validate_range("event_capacity", self.event_capacity, 1, MAX_EVENT_CAPACITY)?;
        validate_nonzero("read_chunk_bytes", self.read_chunk_bytes)?;
        if self.read_chunk_bytes > MAX_PTY_OUTPUT_BYTES {
            return Err(SessionError::InvalidConfiguration {
                field: "read_chunk_bytes",
                reason: "must not exceed the core PTY output wire limit",
            });
        }
        validate_nonzero("max_write_bytes", self.max_write_bytes)?;
        validate_nonzero("write_queue_capacity", self.write_queue_capacity)?;
        if self.read_chunk_bytes > self.replay_max_bytes {
            return Err(SessionError::InvalidConfiguration {
                field: "read_chunk_bytes",
                reason: "must not exceed replay_max_bytes",
            });
        }
        validate_duration("write_timeout", self.write_timeout)?;
        validate_duration("idle_after", self.idle_after)?;
        validate_duration("supervisor_interval", self.supervisor_interval)?;
        validate_duration("interrupt_grace", self.interrupt_grace)?;
        validate_duration("hangup_grace", self.hangup_grace)?;
        validate_duration("kill_wait", self.kill_wait)?;
        validate_duration("output_drain_timeout", self.output_drain_timeout)?;
        validate_duration("process_scan_timeout", self.process_scan_timeout)?;
        validate_range(
            "max_tracked_processes",
            self.max_tracked_processes,
            1,
            MAX_TRACKED_PROCESSES,
        )
    }
}

impl Default for SessionManagerConfig {
    fn default() -> Self {
        Self {
            replay_max_bytes: DEFAULT_REPLAY_MAX_BYTES,
            replay_max_chunks: DEFAULT_REPLAY_MAX_CHUNKS,
            event_capacity: DEFAULT_EVENT_CAPACITY,
            read_chunk_bytes: DEFAULT_READ_CHUNK_BYTES,
            max_write_bytes: DEFAULT_MAX_WRITE_BYTES,
            write_queue_capacity: 64,
            write_timeout: Duration::from_secs(2),
            idle_after: Duration::from_secs(10),
            supervisor_interval: Duration::from_millis(100),
            interrupt_grace: Duration::from_millis(1_500),
            hangup_grace: Duration::from_millis(500),
            kill_wait: Duration::from_secs(1),
            output_drain_timeout: Duration::from_millis(250),
            process_scan_timeout: Duration::from_millis(250),
            max_tracked_processes: 4_096,
        }
    }
}

fn validate_nonzero(field: &'static str, value: usize) -> Result<(), SessionError> {
    validate_range(field, value, 1, usize::MAX)
}

fn validate_range(
    field: &'static str,
    value: usize,
    minimum: usize,
    maximum: usize,
) -> Result<(), SessionError> {
    if (minimum..=maximum).contains(&value) {
        Ok(())
    } else {
        Err(SessionError::InvalidConfiguration {
            field,
            reason: "is outside the supported range",
        })
    }
}

fn validate_duration(field: &'static str, value: Duration) -> Result<(), SessionError> {
    if value.is_zero() {
        Err(SessionError::InvalidConfiguration {
            field,
            reason: "must be greater than zero",
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_configuration_is_valid() {
        SessionManagerConfig::default()
            .validate()
            .expect("default session limits should be valid");
    }

    #[test]
    fn read_chunk_must_fit_inside_replay_budget() {
        let config = SessionManagerConfig {
            replay_max_bytes: 4,
            read_chunk_bytes: 5,
            ..SessionManagerConfig::default()
        };

        assert!(matches!(
            config.validate(),
            Err(SessionError::InvalidConfiguration {
                field: "read_chunk_bytes",
                ..
            })
        ));
    }

    #[test]
    fn read_chunk_must_fit_inside_the_wire_contract() {
        let config = SessionManagerConfig {
            replay_max_bytes: DEFAULT_REPLAY_MAX_BYTES,
            read_chunk_bytes: MAX_PTY_OUTPUT_BYTES + 1,
            ..SessionManagerConfig::default()
        };

        assert!(matches!(
            config.validate(),
            Err(SessionError::InvalidConfiguration {
                field: "read_chunk_bytes",
                ..
            })
        ));
    }
}
