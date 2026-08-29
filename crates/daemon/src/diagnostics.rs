//! Bounded, secret-free diagnostic notes for daemon operators.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use cli_master_core::{redact_text, wire::DiagnosticIssue};

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

    /// Records an issue after defensively redacting every free-form field.
    /// Terminal bytes and environment maps still must never be passed here.
    pub fn record(&self, issue: DiagnosticIssue) {
        let issue = DiagnosticIssue {
            code: redact_text(&issue.code),
            message: redact_text(&issue.message),
            action: issue.action.map(|action| redact_text(&action)),
        };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_redacts_untrusted_issue_fields() {
        let log = DiagnosticLog::new(2);
        log.record(DiagnosticIssue {
            code: "probe_failed".to_owned(),
            message: "TOKEN=message-secret".to_owned(),
            action: Some("Retry with Authorization: Bearer action-secret".to_owned()),
        });

        let issues = log.recent();
        let encoded = serde_json::to_string(&issues).expect("issues should serialize");
        assert!(!encoded.contains("message-secret"));
        assert!(!encoded.contains("action-secret"));
        assert!(encoded.contains("[redacted]"));
    }
}
