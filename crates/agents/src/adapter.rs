use std::{fmt, path::PathBuf};

use cli_master_core::{AgentSource, CommandSpec};

use crate::{AgentError, DetectionResult, LaunchEnvironment};

/// Context required to detect and prepare one agent launch.
#[derive(Clone, Eq, PartialEq)]
pub struct LaunchContext {
    cwd: PathBuf,
    environment: LaunchEnvironment,
}

impl LaunchContext {
    /// Creates a launch context. The directory is checked again immediately
    /// before a command is built to detect deletion races.
    #[must_use]
    pub fn new(cwd: impl Into<PathBuf>, environment: LaunchEnvironment) -> Self {
        Self {
            cwd: cwd.into(),
            environment,
        }
    }

    /// Returns the requested process working directory.
    #[must_use]
    pub fn cwd(&self) -> &PathBuf {
        &self.cwd
    }

    /// Returns the explicit executable search environment.
    #[must_use]
    pub const fn environment(&self) -> &LaunchEnvironment {
        &self.environment
    }

    pub(crate) fn validate_cwd(&self) -> Result<(), AgentError> {
        if self.cwd.is_dir() {
            Ok(())
        } else {
            Err(AgentError::InvalidWorkingDirectory(self.cwd.clone()))
        }
    }
}

impl fmt::Debug for LaunchContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LaunchContext")
            .field("cwd", &self.cwd)
            .field("environment", &self.environment)
            .finish()
    }
}

/// Context-independent metadata exposed by an adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterDefinition {
    /// Stable registry key such as `codex`.
    pub key: String,
    /// User-facing adapter name.
    pub display_name: String,
    /// Whether the adapter is bundled or user-defined.
    pub source: AgentSource,
}

/// Uniform contract for bundled and custom coding-agent CLIs.
pub trait AgentAdapter: Send + Sync {
    /// Returns the stable registry key.
    fn key(&self) -> &str;

    /// Returns the user-facing name.
    fn display_name(&self) -> &str;

    /// Returns whether this adapter is bundled or user-defined.
    fn source(&self) -> AgentSource;

    /// Returns context-independent adapter metadata.
    fn definition(&self) -> AdapterDefinition {
        AdapterDefinition {
            key: self.key().to_owned(),
            display_name: self.display_name().to_owned(),
            source: self.source(),
        }
    }

    /// Detects the executable using only the supplied launch environment.
    fn detect(&self, environment: &LaunchEnvironment) -> DetectionResult;

    /// Produces a structured, shell-free command in the context directory.
    ///
    /// # Errors
    ///
    /// Returns an error when the working directory is invalid, the executable
    /// is unavailable, or the core command contract rejects a field.
    fn build_command(&self, context: &LaunchContext) -> Result<CommandSpec, AgentError>;
}

pub(crate) fn resolved_executable(detection: DetectionResult) -> Result<PathBuf, AgentError> {
    match detection {
        DetectionResult::Found { executable } => Ok(executable),
        DetectionResult::NotFound => Err(AgentError::ExecutableNotFound),
        DetectionResult::NotExecutable { candidate } => {
            Err(AgentError::ExecutableNotExecutable(candidate))
        }
    }
}
