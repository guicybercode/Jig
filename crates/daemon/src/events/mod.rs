//! Session event bus: sequenced output, bounded replay, and per-client queues.

mod buffer;
mod bus;
mod encode;
mod fanout;
mod queue;
mod types;

pub use bus::EventBus;
pub use fanout::FanoutEvent;
pub use types::{ClientHandle, ClientId, EventBusLimits, SubscribeError, SubscribeOutcome};
