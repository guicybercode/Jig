use std::{
    collections::BTreeMap,
    fmt,
    path::{Path, PathBuf},
};

use cli_master_core::{AgentSource, validate_structured_invocation};
use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{
    AgentAdapter, AgentCapabilities, AgentError, CustomDefinitionError, PlaceholderContext,
    placeholders,
};

const MAX_ICON_CHARS: usize = 64;
const MAX_COLOR_CHARS: usize = 32;

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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    env_removals: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default_cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<String>,
    #[serde(default = "default_requires_pty")]
    requires_pty: bool,
}

const fn default_requires_pty() -> bool {
    true
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
            key: key.into().trim().to_owned(),
            display_name: display_name.into().trim().to_owned(),
            executable: executable.into().trim().to_owned(),
            args: args.into_iter().map(Into::into).collect(),
            env,
            env_removals: Vec::new(),
            default_cwd: None,
            icon: None,
            color: None,
            requires_pty: true,
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

    /// Returns the absolute path, `~/` path, placeholder template, or bare name.
    #[must_use]
    pub fn executable(&self) -> &str {
        &self.executable
    }

    /// Returns the ordered argument array.
    #[must_use]
    pub fn args(&self) -> &[String] {
        &self.args
    }

    /// Returns only the explicitly configured environment additions.
    #[must_use]
    pub const fn env(&self) -> &BTreeMap<String, String> {
        &self.env
    }

    /// Returns environment keys to remove from the child.
    #[must_use]
    pub fn env_removals(&self) -> &[String] {
        &self.env_removals
    }

    /// Returns the optional default working directory template.
    #[must_use]
    pub fn default_cwd(&self) -> Option<&str> {
        self.default_cwd.as_deref()
    }

    /// Returns optional icon metadata.
    #[must_use]
    pub fn icon(&self) -> Option<&str> {
        self.icon.as_deref()
    }

    /// Returns optional color metadata.
    #[must_use]
    pub fn color(&self) -> Option<&str> {
        self.color.as_deref()
    }

    /// Returns whether the session layer should allocate a PTY.
    #[must_use]
    pub const fn requires_pty(&self) -> bool {
        self.requires_pty
    }

    /// Replaces default arguments.
    ///
    /// # Errors
    ///
    /// Returns an error if an argument contains a NUL byte.
    pub fn with_args<I, S>(mut self, args: I) -> Result<Self, CustomDefinitionError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args = args.into_iter().map(Into::into).collect();
        self.validate()?;
        Ok(self)
    }

    /// Replaces environment additions.
    ///
    /// # Errors
    ///
    /// Returns an error if a key or value is invalid.
    pub fn with_env(
        mut self,
        env: BTreeMap<String, String>,
    ) -> Result<Self, CustomDefinitionError> {
        self.env = env;
        self.validate()?;
        Ok(self)
    }

    /// Replaces environment-removal keys.
    ///
    /// # Errors
    ///
    /// Returns an error if a key is invalid.
    pub fn with_env_removals<I, S>(mut self, keys: I) -> Result<Self, CustomDefinitionError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.env_removals = keys.into_iter().map(Into::into).collect();
        self.validate()?;
        Ok(self)
    }

    /// Sets the default working directory template.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory is a relative path without placeholders.
    pub fn with_default_cwd(
        mut self,
        cwd: impl Into<String>,
    ) -> Result<Self, CustomDefinitionError> {
        let cwd = cwd.into();
        self.default_cwd = if cwd.is_empty() { None } else { Some(cwd) };
        self.validate()?;
        Ok(self)
    }

    /// Sets optional icon metadata.
    ///
    /// # Errors
    ///
    /// Returns an error if the value is too long or contains a NUL byte.
    pub fn with_icon(mut self, icon: impl Into<String>) -> Result<Self, CustomDefinitionError> {
        let icon = icon.into();
        self.icon = if icon.is_empty() { None } else { Some(icon) };
        self.validate()?;
        Ok(self)
    }

    /// Sets optional color metadata.
    ///
    /// # Errors
    ///
    /// Returns an error if the value is not a short token or `#RGB`/`#RRGGBB`.
    pub fn with_color(mut self, color: impl Into<String>) -> Result<Self, CustomDefinitionError> {
        let color = color.into();
        self.color = if color.is_empty() { None } else { Some(color) };
        self.validate()?;
        Ok(self)
    }

    /// Sets whether a PTY is required.
    #[must_use]
    pub const fn with_requires_pty(mut self, requires_pty: bool) -> Self {
        self.requires_pty = requires_pty;
        self
    }

    /// Resolves the working directory for a session.
    ///
    /// `session_cwd` wins when provided. Otherwise `default_cwd` is expanded.
    ///
    /// # Errors
    ///
    /// Returns an error if neither directory was provided or placeholders fail.
    pub fn resolve_cwd(
        &self,
        session_cwd: Option<&Path>,
        placeholders: &PlaceholderContext,
    ) -> Result<PathBuf, AgentError> {
        if let Some(path) = session_cwd {
            return Ok(path.to_path_buf());
        }
        let Some(template) = &self.default_cwd else {
            return Err(AgentError::MissingWorkingDirectory);
        };
        let expanded = placeholders::expand(template, placeholders)?;
        let expanded = crate::expand_leading_tilde(&expanded)?;
        Ok(PathBuf::from(expanded))
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
        validate_executable_shape("executable", &self.executable)?;

        for argument in &self.args {
            validate_no_nul("args", argument)?;
        }
        validate_structured_invocation(&self.executable, &self.args).map_err(|_| {
            CustomDefinitionError::new(
                "args",
                "shell command strings are refused; use a direct executable and argument array",
            )
        })?;
        for (key, value) in &self.env {
            validate_environment_key(key)?;
            validate_no_nul("env", value)?;
        }
        for key in &self.env_removals {
            validate_environment_key(key)?;
        }
        if let Some(cwd) = &self.default_cwd {
            validate_required("default_cwd", cwd)?;
            validate_directory_shape("default_cwd", cwd)?;
        }
        if let Some(icon) = &self.icon {
            validate_no_nul("icon", icon)?;
            if icon.chars().count() > MAX_ICON_CHARS {
                return Err(CustomDefinitionError::new(
                    "icon",
                    "must be at most 64 characters",
                ));
            }
        }
        if let Some(color) = &self.color {
            validate_color(color)?;
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
            .field("env_removals", &self.env_removals)
            .field("default_cwd", &self.default_cwd)
            .field("icon", &self.icon)
            .field("color", &self.color)
            .field("requires_pty", &self.requires_pty)
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
            #[serde(default)]
            env_removals: Vec<String>,
            #[serde(default)]
            default_cwd: Option<String>,
            #[serde(default)]
            icon: Option<String>,
            #[serde(default)]
            color: Option<String>,
            #[serde(default = "default_requires_pty")]
            requires_pty: bool,
        }

        let wire = WireDefinition::deserialize(deserializer)?;
        let mut definition = Self::try_from_parts(
            wire.key,
            wire.display_name,
            wire.executable,
            wire.args,
            wire.env,
        )
        .map_err(de::Error::custom)?;
        definition = definition
            .with_env_removals(wire.env_removals)
            .map_err(de::Error::custom)?;
        if let Some(cwd) = wire.default_cwd {
            definition = definition
                .with_default_cwd(cwd)
                .map_err(de::Error::custom)?;
        }
        if let Some(icon) = wire.icon {
            definition = definition.with_icon(icon).map_err(de::Error::custom)?;
        }
        if let Some(color) = wire.color {
            definition = definition.with_color(color).map_err(de::Error::custom)?;
        }
        Ok(definition.with_requires_pty(wire.requires_pty))
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

    fn executable_name(&self) -> &str {
        self.definition.executable()
    }

    fn default_args(&self) -> &[String] {
        self.definition.args()
    }

    fn environment_additions(&self) -> &BTreeMap<String, String> {
        self.definition.env()
    }

    fn environment_removals(&self) -> &[String] {
        self.definition.env_removals()
    }

    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities::custom(self.definition.requires_pty())
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

fn validate_environment_key(key: &str) -> Result<(), CustomDefinitionError> {
    if key.is_empty() || key.contains('=') {
        return Err(CustomDefinitionError::new(
            "env",
            "variable names must be non-empty and must not contain '='",
        ));
    }
    validate_no_nul("env", key)
}

fn validate_executable_shape(
    field: &'static str,
    value: &str,
) -> Result<(), CustomDefinitionError> {
    if placeholders::contains_placeholder(value) {
        return Ok(());
    }
    if value == "~" || value.starts_with("~/") {
        return Ok(());
    }
    if value.starts_with('~') {
        return Err(CustomDefinitionError::new(
            field,
            "must not use ~user forms; use ~/ or an absolute path",
        ));
    }
    let path = Path::new(value);
    if path.is_absolute() || path.components().count() == 1 {
        Ok(())
    } else {
        Err(CustomDefinitionError::new(
            field,
            "must be an absolute path, a ~/ path, a placeholder, or a bare executable name",
        ))
    }
}

fn validate_directory_shape(field: &'static str, value: &str) -> Result<(), CustomDefinitionError> {
    if placeholders::contains_placeholder(value) {
        return Ok(());
    }
    if value == "~" || value.starts_with("~/") {
        return Ok(());
    }
    if value.starts_with('~') {
        return Err(CustomDefinitionError::new(
            field,
            "must not use ~user forms; use ~/ or an absolute path",
        ));
    }
    if Path::new(value).is_absolute() {
        Ok(())
    } else {
        Err(CustomDefinitionError::new(
            field,
            "must be an absolute path, a ~/ path, or a placeholder",
        ))
    }
}

fn validate_color(color: &str) -> Result<(), CustomDefinitionError> {
    validate_no_nul("color", color)?;
    if color.chars().count() > MAX_COLOR_CHARS {
        return Err(CustomDefinitionError::new(
            "color",
            "must be at most 32 characters",
        ));
    }
    if let Some(hex) = color.strip_prefix('#') {
        let valid_len = hex.len() == 3 || hex.len() == 6;
        let valid_hex = hex.bytes().all(|byte| byte.is_ascii_hexdigit());
        if !(valid_len && valid_hex) {
            return Err(CustomDefinitionError::new(
                "color",
                "must be #RGB, #RRGGBB, or a short token",
            ));
        }
    } else if !color
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(CustomDefinitionError::new(
            "color",
            "must be #RGB, #RRGGBB, or a short token",
        ));
    }
    Ok(())
}
