use std::{error::Error, fmt, path::PathBuf};

use cli_master_core::CommandSpecError;

/// Failure while detecting an agent or building its launch specification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentError {
    /// The requested working directory does not exist or is not a directory.
    InvalidWorkingDirectory(PathBuf),
    /// No executable with the requested name was found in the explicit search path.
    ExecutableNotFound,
    /// A candidate exists but is not an executable regular file.
    ExecutableNotExecutable(PathBuf),
    /// The resolved path cannot be represented by the core string command model.
    NonUtf8ExecutablePath,
    /// The core command contract rejected a structured command field.
    InvalidCommand(CommandSpecError),
}

impl fmt::Display for AgentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidWorkingDirectory(path) => {
                write!(
                    formatter,
                    "working directory is not a directory: {}",
                    path.display()
                )
            }
            Self::ExecutableNotFound => {
                formatter.write_str("agent executable was not found in the configured PATH")
            }
            Self::ExecutableNotExecutable(path) => write!(
                formatter,
                "agent executable is not an executable regular file: {}",
                path.display()
            ),
            Self::NonUtf8ExecutablePath => formatter.write_str(
                "resolved executable path cannot be represented by the core command model",
            ),
            Self::InvalidCommand(error) => write!(formatter, "invalid launch command: {error}"),
        }
    }
}

impl Error for AgentError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidCommand(error) => Some(error),
            _ => None,
        }
    }
}

impl From<CommandSpecError> for AgentError {
    fn from(value: CommandSpecError) -> Self {
        Self::InvalidCommand(value)
    }
}

/// Validation failure for a user-defined adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomDefinitionError {
    field: &'static str,
    reason: &'static str,
}

impl CustomDefinitionError {
    pub(crate) const fn new(field: &'static str, reason: &'static str) -> Self {
        Self { field, reason }
    }

    /// Returns the invalid field without exposing its value.
    #[must_use]
    pub const fn field(&self) -> &'static str {
        self.field
    }

    /// Returns a stable explanation that does not include the rejected value.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        self.reason
    }
}

impl fmt::Display for CustomDefinitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid custom agent {}: {}",
            self.field, self.reason
        )
    }
}

impl Error for CustomDefinitionError {}

/// Failure while adding an adapter to an [`AgentRegistry`](crate::AgentRegistry).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistryError {
    /// Another adapter already owns the stable key.
    DuplicateKey(String),
    /// The adapter returned an empty key.
    EmptyKey,
}

impl fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateKey(key) => write!(formatter, "agent key is already registered: {key}"),
            Self::EmptyKey => formatter.write_str("agent key must not be empty"),
        }
    }
}

impl Error for RegistryError {}
