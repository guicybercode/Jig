//! Safe discovery and launch specifications for local coding-agent CLIs.
//!
//! This crate never invokes a shell or starts a process. It resolves an
//! adapter's executable against an explicit [`LaunchEnvironment`] and produces
//! a structured [`CommandSpec`] for the daemon's process layer.

#![warn(missing_docs)]

mod adapter;
mod builtins;
mod custom;
mod environment;
mod error;
mod registry;

pub use adapter::{AdapterDefinition, AgentAdapter, LaunchContext};
pub use builtins::{ClaudeCodeAdapter, CodexAdapter, GeminiCliAdapter, OpenCodeAdapter};
pub use cli_master_core::{AgentSource, CommandSpec};
pub use custom::{CustomAgentAdapter, CustomAgentDefinition};
pub use environment::{DetectionResult, LaunchEnvironment};
pub use error::{AgentError, CustomDefinitionError, RegistryError};
pub use registry::AgentRegistry;
