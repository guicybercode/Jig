use std::{error::Error, fmt, path::PathBuf};

use cli_master_core::{ApiError, CommandSpecError};

use crate::placeholders::PlaceholderError;

/// Failure while detecting an agent or building its launch specification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentError {
    /// No session working directory or custom-agent default was provided.
    MissingWorkingDirectory,
    /// The requested working directory does not exist or is not a directory.
    InvalidWorkingDirectory(PathBuf),
    /// No executable with the requested name was found in the explicit search path.
    ExecutableNotFound,
    /// A candidate exists but is not an executable regular file.
    ExecutableNotExecutable(PathBuf),
    /// The resolved path cannot be represented by the core string command model.
    NonUtf8ExecutablePath,
    /// A placeholder in a structured field could not be expanded.
    Placeholder(PlaceholderError),
    /// `$HOME` is required to expand a leading `~` and is unset.
    HomeDirectoryUnavailable,
    /// A `~user` path was provided. Only `~/` expansion is supported.
    UnsupportedTildeExpansion,
    /// The core command contract rejected a structured command field.
    InvalidCommand(CommandSpecError),
}

impl AgentError {
    /// Converts this error into a stable IPC error without secret values.
    #[must_use]
    pub fn api_error(&self) -> ApiError {
        match self {
            Self::MissingWorkingDirectory => ApiError::new(
                "AGENT_MISSING_WORKING_DIRECTORY",
                "No working directory was provided for the agent launch.",
            )
            .with_action(
                "Choose a project or worktree directory, or configure a default working directory.",
            ),
            Self::InvalidWorkingDirectory(path) => ApiError::new(
                "AGENT_INVALID_WORKING_DIRECTORY",
                format!("Working directory is not a directory: {}", path.display()),
            )
            .with_action("Choose an existing project or worktree directory."),
            Self::ExecutableNotFound => ApiError::new(
                "AGENT_EXECUTABLE_NOT_FOUND",
                "Could not start the agent because its executable was not found.",
            )
            .with_action(
                "Install the CLI, add its directory to the executable search path, or set an absolute path.",
            ),
            Self::ExecutableNotExecutable(path) => ApiError::new(
                "AGENT_EXECUTABLE_NOT_EXECUTABLE",
                format!(
                    "The agent candidate is not an executable regular file: {}",
                    path.display()
                ),
            )
            .with_action("Fix the file permissions or point the adapter at a different executable."),
            Self::NonUtf8ExecutablePath => ApiError::new(
                "AGENT_NON_UTF8_EXECUTABLE_PATH",
                "The resolved executable path is not valid UTF-8.",
            )
            .with_action("Install the CLI at a UTF-8 path."),
            Self::Placeholder(error) => ApiError::new(
                "AGENT_PLACEHOLDER_ERROR",
                error.to_string(),
            )
            .with_action("Use only ${PROJECT_PATH}, ${WORKTREE_PATH}, ${SESSION_ID}, or ${SESSION_NAME}."),
            Self::HomeDirectoryUnavailable => ApiError::new(
                "AGENT_HOME_UNAVAILABLE",
                "Cannot expand a path starting with '~' because HOME is unset.",
            )
            .with_action("Set HOME or use an absolute executable path."),
            Self::UnsupportedTildeExpansion => ApiError::new(
                "AGENT_UNSUPPORTED_TILDE",
                "Only '~/...' home expansion is supported; ~user paths are rejected.",
            )
            .with_action("Use an absolute path or a path starting with ~/."),
            Self::InvalidCommand(error) => ApiError::new(
                "AGENT_INVALID_COMMAND",
                format!("Invalid launch command: {error}"),
            )
            .with_action("Fix the structured executable, arguments, or environment entries."),
        }
    }
}

impl fmt::Display for AgentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingWorkingDirectory => formatter.write_str(
                "working directory is required; provide a session directory or configure default_cwd",
            ),
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
            Self::Placeholder(error) => write!(formatter, "{error}"),
            Self::HomeDirectoryUnavailable => {
                formatter.write_str("HOME is unset; cannot expand a leading '~'")
            }
            Self::UnsupportedTildeExpansion => {
                formatter.write_str("only '~/...' home expansion is supported")
            }
            Self::InvalidCommand(error) => write!(formatter, "invalid launch command: {error}"),
        }
    }
}

impl Error for AgentError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidCommand(error) => Some(error),
            Self::Placeholder(error) => Some(error),
            _ => None,
        }
    }
}

impl From<CommandSpecError> for AgentError {
    fn from(value: CommandSpecError) -> Self {
        Self::InvalidCommand(value)
    }
}

impl From<PlaceholderError> for AgentError {
    fn from(value: PlaceholderError) -> Self {
        Self::Placeholder(value)
    }
}

/// Validation failure for a user-defined adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomDefinitionError {
    field: &'static str,
    reason: String,
}

impl CustomDefinitionError {
    pub(crate) fn new(field: &'static str, reason: impl Into<String>) -> Self {
        Self {
            field,
            reason: reason.into(),
        }
    }

    /// Returns the invalid field without exposing its value.
    #[must_use]
    pub const fn field(&self) -> &'static str {
        self.field
    }

    /// Returns a stable explanation that does not include the rejected value.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
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

/// Failure while adding or removing an adapter in an [`AgentRegistry`](crate::AgentRegistry).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistryError {
    /// Another adapter already owns the stable key.
    DuplicateKey(String),
    /// Another adapter already uses the display name.
    DuplicateDisplayName(String),
    /// The adapter returned an empty key.
    EmptyKey,
    /// No adapter is registered under the requested key.
    NotFound(String),
    /// Built-in adapters cannot be removed or replaced.
    BuiltInProtected(String),
}

impl fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateKey(key) => write!(formatter, "agent key is already registered: {key}"),
            Self::DuplicateDisplayName(name) => {
                write!(
                    formatter,
                    "agent display name is already registered: {name}"
                )
            }
            Self::EmptyKey => formatter.write_str("agent key must not be empty"),
            Self::NotFound(key) => write!(formatter, "agent key is not registered: {key}"),
            Self::BuiltInProtected(key) => {
                write!(formatter, "built-in agent '{key}' cannot be modified")
            }
        }
    }
}

impl Error for RegistryError {}

/// Failure while importing PATH from a login shell.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PathImportError {
    /// `SHELL` is missing or unsupported and no POSIX fallback exists.
    ShellNotFound,
    /// The configured shell executable could not be started.
    SpawnFailed,
    /// The shell did not exit before the timeout.
    Timeout,
    /// The shell exited unsuccessfully.
    Unsuccessful {
        /// Process exit code, if the shell exited rather than being signaled.
        exit_code: Option<i32>,
    },
    /// Stdout did not contain the expected PATH markers.
    MarkersMissing,
}

impl fmt::Display for PathImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ShellNotFound => formatter.write_str(
                "SHELL is missing or unsupported and no POSIX login shell was found",
            ),
            Self::SpawnFailed => {
                formatter.write_str("the login shell could not be started to read PATH")
            }
            Self::Timeout => formatter.write_str("timed out while reading PATH from the login shell"),
            Self::Unsuccessful { exit_code } => match exit_code {
                Some(code) => write!(formatter, "login shell exited with status {code}"),
                None => formatter.write_str("login shell terminated by a signal"),
            },
            Self::MarkersMissing => formatter.write_str(
                "login shell stdout did not contain PATH markers; startup files may print to stdout",
            ),
        }
    }
}

impl Error for PathImportError {}
