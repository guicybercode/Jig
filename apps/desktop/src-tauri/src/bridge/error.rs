use cli_master_core::{ApiError, RequestId};
use thiserror::Error;

use super::frame::FrameError;

/// Transport or handshake failure between the desktop process and the daemon.
///
/// Application-level daemon errors stay inside a successful response envelope.
/// These variants are used only when the bridge cannot complete the wire round
/// trip, or when the connected daemon speaks an incompatible protocol.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum BridgeError {
    /// No daemon is listening, or a sidecar could not be started.
    #[error("daemon is unavailable")]
    Unavailable {
        /// Non-secret diagnostic reason.
        detail: String,
    },
    /// The daemon completed a handshake with an unsupported protocol version.
    #[error("daemon protocol version is incompatible")]
    Incompatible {
        /// Protocol version reported by the daemon, when known.
        protocol_version: Option<u16>,
        /// Daemon semantic version, when known.
        daemon_version: Option<String>,
        /// Non-secret diagnostic reason.
        detail: String,
    },
    /// A request was not answered within the configured timeout.
    #[error("daemon request timed out")]
    Timeout {
        /// Wire method that timed out.
        method: String,
        /// Correlation identifier of the unanswered request.
        request_id: RequestId,
    },
    /// The socket closed before a correlated response arrived.
    #[error("daemon connection closed")]
    Disconnected {
        /// Wire method that was in flight.
        method: String,
        /// Correlation identifier of the in-flight request.
        request_id: RequestId,
    },
    /// The caller supplied a method name that is not a dotted identifier.
    #[error("method name is not a valid dotted identifier")]
    InvalidMethod {
        /// Rejected method string, without a payload.
        method: String,
    },
    /// Length-prefix framing failed.
    #[error(transparent)]
    Frame(#[from] FrameError),
    /// A frame decoded as JSON but was not a usable envelope.
    #[error("daemon frame was not a valid protocol envelope")]
    Protocol {
        /// Non-secret diagnostic reason.
        detail: String,
    },
}

impl BridgeError {
    /// Machine-readable error code suitable for the webview.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unavailable { .. } => "daemon_unavailable",
            Self::Incompatible { .. } => "unsupported_protocol_version",
            Self::Timeout { .. } => "daemon_timeout",
            Self::Disconnected { .. } => "daemon_disconnected",
            Self::InvalidMethod { .. } => "invalid_method",
            Self::Frame(_) => "invalid_frame",
            Self::Protocol { .. } => "daemon_protocol_error",
        }
    }

    /// Concise user-facing explanation that does not include payloads.
    #[must_use]
    pub fn message(&self) -> &'static str {
        match self {
            Self::Unavailable { .. } => "The local daemon is not available",
            Self::Incompatible { .. } => "The local daemon speaks an incompatible protocol",
            Self::Timeout { .. } => "The daemon did not respond in time",
            Self::Disconnected { .. } => "The daemon connection closed before a response arrived",
            Self::InvalidMethod { .. } => "The request method is not a dotted identifier",
            Self::Frame(_) => "The daemon sent an invalid transport frame",
            Self::Protocol { .. } => "The daemon sent a frame that is not a protocol envelope",
        }
    }
}

impl From<BridgeError> for ApiError {
    fn from(error: BridgeError) -> Self {
        let mut api = Self::new(error.code(), error.message());
        match &error {
            BridgeError::Unavailable { detail }
            | BridgeError::Incompatible { detail, .. }
            | BridgeError::Protocol { detail } => {
                api = api.with_detail("reason", detail.clone());
            }
            BridgeError::Timeout { method, request_id }
            | BridgeError::Disconnected { method, request_id } => {
                api = api
                    .with_detail("method", method.clone())
                    .with_detail("requestId", request_id.to_string());
            }
            BridgeError::InvalidMethod { method } => {
                api = api.with_detail("method", method.clone());
            }
            BridgeError::Frame(frame) => {
                api = api.with_detail("reason", frame.to_string());
            }
        }
        if let BridgeError::Incompatible {
            protocol_version,
            daemon_version,
            ..
        } = &error
        {
            if let Some(version) = protocol_version {
                api = api.with_detail("protocolVersion", i64::from(*version));
            }
            if let Some(version) = daemon_version {
                api = api.with_detail("daemonVersion", version.clone());
            }
        }
        api
    }
}
