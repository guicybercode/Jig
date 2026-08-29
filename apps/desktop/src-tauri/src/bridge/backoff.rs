use std::time::Duration;

/// Exponential reconnect delay with a hard cap on both duration and attempts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundedBackoff {
    initial: Duration,
    cap: Duration,
    max_attempts: u32,
    attempt: u32,
}

impl BoundedBackoff {
    /// Creates a backoff that starts at 100ms, caps at 2s, and stops after 8 waits.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            initial: Duration::from_millis(100),
            cap: Duration::from_secs(2),
            max_attempts: 8,
            attempt: 0,
        }
    }

    /// Creates a backoff with explicit limits. Used by tests that cannot wait
    /// for production delays.
    #[must_use]
    pub const fn with_limits(initial: Duration, cap: Duration, max_attempts: u32) -> Self {
        Self {
            initial,
            cap,
            max_attempts,
            attempt: 0,
        }
    }

    /// Returns the next delay, or `None` when the attempt budget is exhausted.
    pub fn next_delay(&mut self) -> Option<Duration> {
        if self.attempt >= self.max_attempts {
            return None;
        }
        let shift = self.attempt.min(16);
        let delay = self
            .initial
            .saturating_mul(2_u32.saturating_pow(shift))
            .min(self.cap);
        self.attempt += 1;
        Some(delay)
    }

    /// One-based count of delays already issued.
    #[must_use]
    pub const fn attempt(&self) -> u32 {
        self.attempt
    }

    /// Restarts the sequence after a successful handshake or an explicit reconnect.
    pub const fn reset(&mut self) {
        self.attempt = 0;
    }
}

impl Default for BoundedBackoff {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::BoundedBackoff;

    #[test]
    fn delays_double_until_the_cap_then_stop() {
        let mut backoff =
            BoundedBackoff::with_limits(Duration::from_millis(100), Duration::from_secs(2), 8);
        let delays: Vec<_> = (0..8).map(|_| backoff.next_delay()).collect();
        assert_eq!(
            delays,
            vec![
                Some(Duration::from_millis(100)),
                Some(Duration::from_millis(200)),
                Some(Duration::from_millis(400)),
                Some(Duration::from_millis(800)),
                Some(Duration::from_millis(1600)),
                Some(Duration::from_secs(2)),
                Some(Duration::from_secs(2)),
                Some(Duration::from_secs(2)),
            ]
        );
        assert_eq!(backoff.next_delay(), None);
        backoff.reset();
        assert_eq!(backoff.next_delay(), Some(Duration::from_millis(100)));
    }
}
