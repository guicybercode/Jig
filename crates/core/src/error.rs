use std::{collections::BTreeMap, error::Error, fmt};

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
    /// Short dialog title when the backend has a stable one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<Box<str>>,
    /// Whether correcting input or retrying can recover safely.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recoverable: Option<bool>,
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
            title: None,
            recoverable: None,
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

    /// Adds a concise dialog title.
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(redact_text(&title.into()).into_boxed_str());
        self
    }

    /// Marks whether the caller can recover without losing data.
    #[must_use]
    pub const fn with_recoverable(mut self, recoverable: bool) -> Self {
        self.recoverable = Some(recoverable);
        self
    }
}

/// Internal failure with a safe wire projection and log-only technical context.
///
/// Domain crates should retain their specialized error enums. This type is the
/// application-boundary envelope used when an adapter needs to preserve a
/// redacted source chain for local logs while returning only [`ApiError`] over
/// IPC. Technical fields are deliberately not serializable.
#[derive(Clone, Debug)]
pub struct ApplicationError {
    inner: Box<ApplicationErrorInner>,
}

#[derive(Clone, Debug)]
struct ApplicationErrorInner {
    code: String,
    title: String,
    user_message: String,
    technical_message: String,
    recoverable: bool,
    suggested_action: Option<String>,
    context: BTreeMap<String, Value>,
    source_chain: Option<String>,
}

impl ApplicationError {
    /// Creates a recoverable application-boundary error.
    #[must_use]
    pub fn new(
        code: impl Into<String>,
        title: impl Into<String>,
        user_message: impl Into<String>,
    ) -> Self {
        let user_message = redact_text(&user_message.into());
        Self {
            inner: Box::new(ApplicationErrorInner {
                code: redact_text(&code.into()),
                title: redact_text(&title.into()),
                technical_message: user_message.clone(),
                user_message,
                recoverable: true,
                suggested_action: None,
                context: BTreeMap::new(),
                source_chain: None,
            }),
        }
    }

    /// Marks the failure as not recoverable by retrying the same action.
    #[must_use]
    pub fn not_recoverable(mut self) -> Self {
        self.inner.recoverable = false;
        self
    }

    /// Adds a user-facing remediation action.
    #[must_use]
    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.inner.suggested_action = Some(redact_text(&action.into()));
        self
    }

    /// Replaces the redacted, log-only technical explanation.
    #[must_use]
    pub fn with_technical(mut self, message: impl Into<String>) -> Self {
        self.inner.technical_message = redact_text(&message.into());
        self
    }

    /// Adds sanitized diagnostic context to the safe IPC projection.
    #[must_use]
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        let key = key.into();
        self.inner
            .context
            .insert(key.clone(), redact_json_value(&key, value.into()));
        self
    }

    /// Captures a redacted error source chain for local structured logs only.
    #[must_use]
    pub fn with_source(mut self, error: &dyn Error) -> Self {
        self.inner.source_chain = Some(format_error_chain(error));
        self
    }

    /// Returns the stable error code.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.inner.code
    }

    /// Returns the redacted, log-only technical explanation.
    #[must_use]
    pub fn technical_message(&self) -> &str {
        &self.inner.technical_message
    }

    /// Returns the redacted, log-only source chain.
    #[must_use]
    pub fn source_chain(&self) -> Option<&str> {
        self.inner.source_chain.as_deref()
    }

    /// Projects this failure to the stable wire error without technical fields.
    #[must_use]
    pub fn to_api_error(&self) -> ApiError {
        ApiError {
            code: self.inner.code.clone(),
            message: self.inner.user_message.clone(),
            action: self.inner.suggested_action.clone(),
            details: self.inner.context.clone(),
            title: Some(self.inner.title.clone().into_boxed_str()),
            recoverable: Some(self.inner.recoverable),
        }
    }
}

impl fmt::Display for ApplicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}: {}",
            self.inner.code, self.inner.user_message
        )
    }
}

impl Error for ApplicationError {}

impl From<ApplicationError> for ApiError {
    fn from(value: ApplicationError) -> Self {
        value.to_api_error()
    }
}

impl From<&ApplicationError> for ApiError {
    fn from(value: &ApplicationError) -> Self {
        value.to_api_error()
    }
}

fn format_error_chain(error: &dyn Error) -> String {
    let mut parts = vec![redact_text(&error.to_string())];
    let mut current = error.source();
    while let Some(source) = current {
        parts.push(redact_text(&source.to_string()));
        current = source.source();
    }
    parts.join(" => ")
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

    #[test]
    fn application_error_keeps_technical_fields_off_the_wire() {
        let source = std::io::Error::other("password=source-secret");
        let error = ApplicationError::new(
            "git_inspection_failed",
            "Git inspection failed",
            "Git could not inspect the selected repository",
        )
        .with_action("Retry without TOKEN=action-secret")
        .with_technical("git status exited 128 with API_KEY=technical-secret")
        .with_context("AUTHORIZATION", "Bearer context-secret")
        .with_source(&source)
        .not_recoverable();

        let value = serde_json::to_value(error.to_api_error()).expect("API error should serialize");
        let encoded = value.to_string();
        for secret in [
            "action-secret",
            "technical-secret",
            "context-secret",
            "source-secret",
        ] {
            assert!(!encoded.contains(secret), "wire leaked {secret}: {encoded}");
        }
        assert_eq!(value["title"], "Git inspection failed");
        assert_eq!(value["recoverable"], false);
        assert!(value.get("technicalMessage").is_none());
        assert!(value.get("sourceChain").is_none());
        assert!(error.technical_message().contains("[redacted]"));
        assert!(
            error
                .source_chain()
                .is_some_and(|chain| chain.contains("[redacted]"))
        );
    }
}
