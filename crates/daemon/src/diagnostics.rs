//! Bounded, secret-free diagnostic notes for daemon operators.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use cli_master_core::wire::DiagnosticIssue;

const DEFAULT_CAPACITY: usize = 32;

/// Ring of recent diagnostic issues that is safe to expose over IPC.
#[derive(Clone, Debug)]
pub struct DiagnosticLog {
    inner: Arc<Mutex<VecDeque<DiagnosticIssue>>>,
    capacity: usize,
}

impl Default for DiagnosticLog {
    fn default() -> Self {
        Self::new(DEFAULT_CAPACITY)
    }
}

impl DiagnosticLog {
    /// Creates a log that retains at most `capacity` issues.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::new())),
            capacity: capacity.max(1),
        }
    }

    /// Records a safe issue. Callers must not pass environment values, tokens,
    /// or terminal bytes in `message` or `action`.
    pub fn record(&self, issue: DiagnosticIssue) {
        tracing::warn!(
            code = %issue.code,
            message = %issue.message,
            "daemon diagnostic"
        );
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.push_back(issue);
        while guard.len() > self.capacity {
            guard.pop_front();
        }
    }

    /// Returns issues from oldest to newest.
    #[must_use]
    pub fn recent(&self) -> Vec<DiagnosticIssue> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .cloned()
            .collect()
    }
}
