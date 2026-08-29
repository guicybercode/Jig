use std::{collections::BTreeMap, fmt, path::Path};

use cli_master_core::{AgentSource, CommandSpec};
use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{
    AgentAdapter, AgentError, CustomDefinitionError, DetectionResult, LaunchContext,
    LaunchEnvironment, adapter::resolved_executable,
};

/// Validated, serializable definition for a user-provided CLI adapter.
///
/// `Debug` intentionally exposes only argument counts and environment keys.
#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomAgentDefinition {
    key: String,
    display_name: String,
    executable: String,
    args: Vec<String>,
    env: BTreeMap<String, String>,
}

impl CustomAgentDefinition {
    /// Creates a definition without arguments or environment overrides.
    ///
    /// # Errors
    ///
    /// Returns an error for empty or NUL-containing fields, unsafe relative
    /// executable paths, or invalid environment variable names.
    pub fn new(
        key: impl Into<String>,
        display_name: impl Into<String>,
        executable: impl Into<String>,
    ) -> Result<Self, CustomDefinitionError> {
        Self::try_from_parts(
            key,
            display_name,
            executable,
            Vec::<String>::new(),
            BTreeMap::new(),
        )
    }

    /// Creates a definition from structured executable, arguments and overrides.
    ///
    /// Arguments remain individual values; they are never parsed or joined as
    /// a shell command.
    ///
    /// # Errors
    ///
    /// Returns an error for empty or NUL-containing fields, unsafe relative
    /// executable paths, or invalid environment variable names.
    pub fn try_from_parts<K, N, E, A, S>(
        key: K,
        display_name: N,
        executable: E,
        args: A,
        env: BTreeMap<String, String>,
    ) -> Result<Self, CustomDefinitionError>
    where
        K: Into<String>,
        N: Into<String>,
        E: Into<String>,
        A: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let definition = Self {
            key: key.into(),
            display_name: display_name.into(),
            executable: executable.into(),
            args: args.into_iter().map(Into::into).collect(),
            env,
        };
        definition.validate()?;
        Ok(definition)
    }

    /// Returns the stable registry key.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the user-facing name.
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// Returns the absolute path or bare executable name.
    #[must_use]
    pub fn executable(&self) -> &str {
        &self.executable
    }

    /// Returns the ordered argument array.
    #[must_use]
    pub fn args(&self) -> &[String] {
        &self.args
    }

    /// Returns only the explicitly configured environment overrides.
    #[must_use]
    pub const fn env(&self) -> &BTreeMap<String, String> {
        &self.env
    }

    fn validate(&self) -> Result<(), CustomDefinitionError> {
        validate_required("key", &self.key)?;
        if !self
            .key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err(CustomDefinitionError::new(
                "key",
                "must use only ASCII letters, digits, '.', '-' or '_'",
            ));
        }
        validate_required("display_name", &self.display_name)?;
        validate_required("executable", &self.executable)?;

        let executable_path = Path::new(&self.executable);
        if !executable_path.is_absolute() && executable_path.components().count() != 1 {
            return Err(CustomDefinitionError::new(
                "executable",
                "must be an absolute path or a bare executable name",
            ));
        }

        for argument in &self.args {
            validate_no_nul("args", argument)?;
        }
        for (key, value) in &self.env {
            if key.is_empty() || key.contains('=') {
                return Err(CustomDefinitionError::new(
                    "env",
                    "variable names must be non-empty and must not contain '='",
                ));
            }
            validate_no_nul("env", key)?;
            validate_no_nul("env", value)?;
        }
        Ok(())
    }
}

impl fmt::Debug for CustomAgentDefinition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CustomAgentDefinition")
            .field("key", &self.key)
            .field("display_name", &self.display_name)
            .field("executable", &self.executable)
            .field("args_count", &self.args.len())
            .field("env_keys", &self.env.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl<'de> Deserialize<'de> for CustomAgentDefinition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct WireDefinition {
            key: String,
            display_name: String,
            executable: String,
            #[serde(default)]
            args: Vec<String>,
            #[serde(default)]
            env: BTreeMap<String, String>,
        }

        let wire = WireDefinition::deserialize(deserializer)?;
        Self::try_from_parts(
            wire.key,
            wire.display_name,
            wire.executable,
            wire.args,
            wire.env,
        )
        .map_err(de::Error::custom)
    }
}

/// Runtime adapter backed by a validated [`CustomAgentDefinition`].
#[derive(Clone)]
pub struct CustomAgentAdapter {
    definition: CustomAgentDefinition,
}

impl CustomAgentAdapter {
    /// Wraps a validated custom definition.
    #[must_use]
    pub const fn new(definition: CustomAgentDefinition) -> Self {
        Self { definition }
    }

    /// Returns the persisted custom definition.
    #[must_use]
    pub const fn custom_definition(&self) -> &CustomAgentDefinition {
        &self.definition
    }
}

impl fmt::Debug for CustomAgentAdapter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CustomAgentAdapter")
            .field("definition", &self.definition)
            .finish()
    }
}

impl AgentAdapter for CustomAgentAdapter {
    fn key(&self) -> &str {
        self.definition.key()
    }

    fn display_name(&self) -> &str {
        self.definition.display_name()
    }

    fn source(&self) -> AgentSource {
        AgentSource::Custom
    }

    fn detect(&self, environment: &LaunchEnvironment) -> DetectionResult {
        environment.detect(self.definition.executable())
    }

    fn build_command(&self, context: &LaunchContext) -> Result<CommandSpec, AgentError> {
        context.validate_cwd()?;
        let executable = resolved_executable(self.detect(context.environment()))?;
        let executable = executable
            .to_str()
            .ok_or(AgentError::NonUtf8ExecutablePath)?;

        CommandSpec::try_from_parts(
            executable,
            self.definition.args().iter().cloned(),
            context.cwd(),
            self.definition.env().clone(),
        )
        .map_err(AgentError::from)
    }
}

fn validate_required(field: &'static str, value: &str) -> Result<(), CustomDefinitionError> {
    if value.is_empty() {
        return Err(CustomDefinitionError::new(field, "must not be empty"));
    }
    validate_no_nul(field, value)
}

fn validate_no_nul(field: &'static str, value: &str) -> Result<(), CustomDefinitionError> {
    if value.contains('\0') {
        Err(CustomDefinitionError::new(
            field,
            "must not contain a NUL byte",
        ))
    } else {
        Ok(())
    }
}
