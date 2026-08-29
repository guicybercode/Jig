use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{redact_json_value, redact_text};

/// A stable, actionable error returned across the IPC boundary.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ApiError {
    /// Machine-readable error code suitable for branching and diagnostics.
    pub code: String,
    /// Concise human-readable explanation of the failure.
    pub message: String,
    /// Suggested action the user can take to resolve the failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// Structured diagnostic context. Secrets must never be inserted here.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, Value>,
}

impl ApiError {
    /// Creates an error without an action or diagnostic details.
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: redact_text(&message.into()),
            action: None,
            details: BTreeMap::new(),
        }
    }

    /// Adds a user-facing remediation action.
    #[must_use]
    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = Some(redact_text(&action.into()));
        self
    }

    /// Adds one structured diagnostic detail.
    #[must_use]
    pub fn with_detail(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        let key = key.into();
        self.details
            .insert(key.clone(), redact_json_value(&key, value.into()));
        self
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn constructors_redact_every_wire_visible_field() {
        let error = ApiError::new(
            "request_failed",
            "Could not continue with TOKEN=message-secret",
        )
        .with_action("Remove Authorization: Bearer action-secret and retry")
        .with_detail("API_KEY", "detail-secret")
        .with_detail(
            "context",
            json!({
                "cookie": "cookie-secret",
                "safe": "kept",
            }),
        );

        let encoded = serde_json::to_string(&error).expect("API error should serialize");
        for secret in [
            "message-secret",
            "action-secret",
            "detail-secret",
            "cookie-secret",
        ] {
            assert!(!encoded.contains(secret), "leaked {secret}: {encoded}");
        }
        assert_eq!(error.details["API_KEY"], "[redacted]");
        assert_eq!(error.details["context"]["safe"], "kept");
    }

    #[test]
    fn safe_legacy_shape_is_unchanged() {
        let error = ApiError::new("executable_not_found", "Could not start Codex")
            .with_action("Install Codex")
            .with_detail("executable", "codex");

        assert_eq!(
            serde_json::to_value(error).expect("API error should serialize"),
            json!({
                "code": "executable_not_found",
                "message": "Could not start Codex",
                "action": "Install Codex",
                "details": { "executable": "codex" },
            })
        );
    }
}
