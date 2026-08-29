use cli_master_core::EventEnvelope;
use serde_json::Value;
use tauri::{AppHandle, Emitter};

use super::log;
use super::status::{DAEMON_EVENT, DAEMON_STATUS, DaemonStatus};

/// Receives daemon events and status updates from the bridge actor.
pub trait BridgeRelay: Send + Sync {
    /// Relays a decoded event envelope to interested listeners.
    fn event(&self, envelope: EventEnvelope<Value>);
    /// Relays a connection-status change.
    fn status(&self, status: DaemonStatus);
}

/// Relays envelopes and status to the Tauri webview.
pub struct TauriRelay {
    app: AppHandle,
}

impl TauriRelay {
    /// Creates a relay bound to the running application handle.
    #[must_use]
    pub const fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl BridgeRelay for TauriRelay {
    fn event(&self, envelope: EventEnvelope<Value>) {
        if let Err(error) = self.app.emit(DAEMON_EVENT, &envelope) {
            tracing::warn!(%error, event = %envelope.event, "could not relay daemon event");
        }
    }

    fn status(&self, status: DaemonStatus) {
        log::status_changed(&status);
        if let Err(error) = self.app.emit(DAEMON_STATUS, &status) {
            tracing::warn!(%error, "could not relay daemon status");
        }
    }
}

/// Test double that records events and status without a webview.
#[cfg(test)]
#[derive(Default)]
pub struct RecordingRelay {
    events: std::sync::Mutex<Vec<EventEnvelope<Value>>>,
}

#[cfg(test)]
impl RecordingRelay {
    pub fn events(&self) -> Vec<EventEnvelope<Value>> {
        self.events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

#[cfg(test)]
impl BridgeRelay for RecordingRelay {
    fn event(&self, envelope: EventEnvelope<Value>) {
        self.events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(envelope);
    }

    fn status(&self, status: DaemonStatus) {
        log::status_changed(&status);
    }
}

/// Relay that discards events. Status is still logged by the actor's watch sender.
#[cfg(test)]
pub struct NoopRelay;

#[cfg(test)]
impl BridgeRelay for NoopRelay {
    fn event(&self, _envelope: EventEnvelope<Value>) {}

    fn status(&self, status: DaemonStatus) {
        log::status_changed(&status);
    }
}
