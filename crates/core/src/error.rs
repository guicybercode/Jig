use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
            message: message.into(),
            action: None,
            details: BTreeMap::new(),
        }
    }

    /// Adds a user-facing remediation action.
    #[must_use]
    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = Some(action.into());
        self
    }

    /// Adds one structured diagnostic detail.
    #[must_use]
    pub fn with_detail(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.details.insert(key.into(), value.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::ApiError;

    #[test]
    fn serializes_actionable_error_without_empty_details() {
        let error = ApiError::new("AGENT_EXECUTABLE_NOT_FOUND", "Could not start Codex")
            .with_action("Install Codex or configure an executable search path.")
            .with_detail("executable", "codex");
        let value = serde_json::to_value(&error).expect("error should serialize");
        assert_eq!(value["code"], "AGENT_EXECUTABLE_NOT_FOUND");
        assert_eq!(value["details"]["executable"], "codex");
        assert!(value.get("message").is_some());

        let compact = ApiError::new("INTERNAL", "failed");
        let compact_json = serde_json::to_string(&compact).expect("compact error");
        assert!(!compact_json.contains("details"));
        assert!(!compact_json.contains("action"));
    }

    #[test]
    fn debug_and_display_do_not_require_secret_fields() {
        let error = ApiError::new("GIT_FAILED", "git status failed");
        let rendered = format!("{error:?}");
        assert!(rendered.contains("GIT_FAILED"));
        assert!(!rendered.contains("TOKEN"));
        assert!(!rendered.contains("secret"));
    }
}
