use cli_master_core::wire::{
    SessionExitedEvent, SessionOutputEvent, SessionOutputGapEvent, SessionReplayCompleteEvent,
    SessionStatusChangedEvent, event_name,
};
use cli_master_core::{EnvelopeKind, EventEnvelope, PROTOCOL_V1};
use serde_json::Value;

/// One daemon event ready for a subscriber or the Unix socket.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FanoutEvent {
    /// Monotonic sequence for this daemon lifetime.
    pub envelope_sequence: u64,
    /// Dotted Beta v1 event name.
    pub name: String,
    /// JSON payload matching the named contract type.
    pub payload: Value,
}

impl FanoutEvent {
    /// Builds a live or replay `session.output` event.
    #[must_use]
    pub fn output(envelope_sequence: u64, payload: SessionOutputEvent) -> Self {
        Self::from_typed(envelope_sequence, event_name::SESSION_OUTPUT, payload)
    }

    /// Builds a `session.replay_complete` marker.
    #[must_use]
    pub fn replay_complete(envelope_sequence: u64, payload: SessionReplayCompleteEvent) -> Self {
        Self::from_typed(
            envelope_sequence,
            event_name::SESSION_REPLAY_COMPLETE,
            payload,
        )
    }

    /// Builds a `session.output_gap` resync marker.
    #[must_use]
    pub fn gap(envelope_sequence: u64, payload: SessionOutputGapEvent) -> Self {
        Self::from_typed(envelope_sequence, event_name::SESSION_OUTPUT_GAP, payload)
    }

    /// Builds a `session.status_changed` event.
    #[must_use]
    pub fn status(envelope_sequence: u64, payload: SessionStatusChangedEvent) -> Self {
        Self::from_typed(
            envelope_sequence,
            event_name::SESSION_STATUS_CHANGED,
            payload,
        )
    }

    /// Builds a `session.exited` event.
    #[must_use]
    pub fn exited(envelope_sequence: u64, payload: SessionExitedEvent) -> Self {
        Self::from_typed(envelope_sequence, event_name::SESSION_EXITED, payload)
    }

    fn from_typed<T: serde::Serialize>(envelope_sequence: u64, name: &str, payload: T) -> Self {
        Self {
            envelope_sequence,
            name: name.to_owned(),
            payload: serde_json::to_value(payload).unwrap_or(Value::Null),
        }
    }

    /// Wraps this payload in a protocol v1 event envelope.
    #[must_use]
    pub fn into_envelope(self) -> EventEnvelope<Value> {
        EventEnvelope {
            kind: EnvelopeKind::Event,
            version: PROTOCOL_V1,
            event: self.name,
            sequence: self.envelope_sequence,
            payload: self.payload,
        }
    }

    /// Returns the per-session output sequence when this event carries one.
    #[must_use]
    pub fn output_sequence(&self) -> Option<u64> {
        self.payload.get("outputSequence")?.as_u64()
    }

    /// Returns whether this is retained output rather than a live chunk.
    #[must_use]
    pub fn is_replay_output(&self) -> bool {
        self.name == event_name::SESSION_OUTPUT
            && self.payload.get("replay").and_then(Value::as_bool) == Some(true)
    }
}
