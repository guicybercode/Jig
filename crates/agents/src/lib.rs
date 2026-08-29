//! Discovery, validation, and launch specifications for local coding-agent CLIs.
//!
//! Adapters resolve an executable against an explicit [`LaunchEnvironment`] and
//! produce a structured [`CommandSpec`]. Version probes and executable tests may
//! spawn a child with stdin closed and a timeout. They never invoke a shell and
//! never write a prompt.

#![warn(missing_docs)]

mod adapter;
mod builtins;
mod custom;
mod environment;
mod error;
mod placeholders;
mod probe;
mod process;
mod registry;

pub use adapter::{
    AdapterDefinition, AgentAdapter, AgentCapabilities, AgentDefinition, AgentDiagnostics,
    LaunchContext,
};
pub use builtins::{ClaudeCodeAdapter, CodexAdapter, GeminiCliAdapter, OpenCodeAdapter};
pub use cli_master_core::{AgentSource, CommandSpec};
pub use custom::{CustomAgentAdapter, CustomAgentDefinition};
pub use environment::{
    DetectionResult, LaunchEnvironment, PathDiagnostics, expand_leading_tilde,
    read_login_shell_path, standard_search_directories,
};
pub use error::{AgentError, CustomDefinitionError, PathImportError, RegistryError};
pub use placeholders::{
    PROJECT_PATH, PlaceholderContext, PlaceholderError, SESSION_ID, SESSION_NAME, WORKTREE_PATH,
    contains_placeholder, expand, expand_args, expand_env,
};
pub use probe::{ExecutableTestReport, LaunchTestStatus, ProbeOptions, test_executable};
pub use registry::AgentRegistry;
