use std::{collections::BTreeMap, fmt, path::PathBuf, sync::LazyLock};

use cli_master_core::{AgentSource, CommandSpec};
use serde::Serialize;

use crate::{
    AgentError, DetectionResult, ExecutableTestReport, LaunchEnvironment, LaunchTestStatus,
    PlaceholderContext, ProbeOptions, expand_leading_tilde, placeholders, probe::test_executable,
};

static EMPTY_ENV: LazyLock<BTreeMap<String, String>> = LazyLock::new(BTreeMap::new);

/// Context required to detect and prepare one agent launch.
#[derive(Clone, Eq, PartialEq)]
pub struct LaunchContext {
    cwd: PathBuf,
    environment: LaunchEnvironment,
    extra_args: Vec<String>,
    placeholders: PlaceholderContext,
    terminal_title: Option<String>,
    executable_override: Option<PathBuf>,
    startup_input: Option<String>,
    env_additions: BTreeMap<String, String>,
    env_removals: Vec<String>,
}

impl LaunchContext {
    /// Creates a launch context. The directory is checked again immediately
    /// before a command is built to detect deletion races.
    #[must_use]
    pub fn new(cwd: impl Into<PathBuf>, environment: LaunchEnvironment) -> Self {
        Self {
            cwd: cwd.into(),
            environment,
            extra_args: Vec::new(),
            placeholders: PlaceholderContext::new(),
            terminal_title: None,
            executable_override: None,
            startup_input: None,
            env_additions: BTreeMap::new(),
            env_removals: Vec::new(),
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

    /// Returns extra arguments appended after the adapter defaults.
    #[must_use]
    pub fn extra_args(&self) -> &[String] {
        &self.extra_args
    }

    /// Returns placeholder values for this launch.
    #[must_use]
    pub const fn placeholders(&self) -> &PlaceholderContext {
        &self.placeholders
    }

    /// Appends extra arguments. Values stay separate; they are never parsed as a shell string.
    ///
    /// # Errors
    ///
    /// Returns an error if an argument contains a NUL byte.
    pub fn with_extra_args<I, S>(mut self, args: I) -> Result<Self, AgentError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.extra_args = args.into_iter().map(Into::into).collect();
        for argument in &self.extra_args {
            if argument.contains('\0') {
                return Err(AgentError::from(
                    cli_master_core::CommandSpecError::ContainsNul("argument"),
                ));
            }
        }
        Ok(self)
    }

    /// Sets placeholder values used while building the command.
    #[must_use]
    pub fn with_placeholders(mut self, placeholders: PlaceholderContext) -> Self {
        self.placeholders = placeholders;
        self
    }

    /// Sets the terminal title template. Empty titles are ignored.
    ///
    /// # Errors
    ///
    /// Returns an error if the title contains a NUL byte.
    pub fn with_terminal_title(mut self, title: impl Into<String>) -> Result<Self, AgentError> {
        let title = title.into();
        if title.contains('\0') {
            return Err(AgentError::from(
                cli_master_core::CommandSpecError::ContainsNul("terminal title"),
            ));
        }
        self.terminal_title = if title.is_empty() { None } else { Some(title) };
        Ok(self)
    }

    /// Forces an absolute executable instead of PATH lookup.
    #[must_use]
    pub fn with_executable_override(mut self, path: impl Into<PathBuf>) -> Self {
        self.executable_override = Some(path.into());
        self
    }

    /// Sets optional PTY startup input. Empty input is ignored.
    ///
    /// # Errors
    ///
    /// Returns an error if the value contains a NUL byte.
    pub fn with_startup_input(mut self, input: impl Into<String>) -> Result<Self, AgentError> {
        let input = input.into();
        if input.contains('\0') {
            return Err(AgentError::from(
                cli_master_core::CommandSpecError::ContainsNul("startup input"),
            ));
        }
        self.startup_input = if input.is_empty() { None } else { Some(input) };
        Ok(self)
    }

    /// Adds session-level environment additions. Keys override adapter defaults.
    ///
    /// # Errors
    ///
    /// Returns an error if a key or value is invalid for [`CommandSpec`].
    pub fn with_env_additions(mut self, env: BTreeMap<String, String>) -> Result<Self, AgentError> {
        CommandSpec::try_from_parts("probe", Vec::<String>::new(), "/", env.clone())?;
        self.env_additions = env;
        Ok(self)
    }

    /// Adds environment variable names to remove from the child.
    ///
    /// # Errors
    ///
    /// Returns an error if a key is invalid.
    pub fn with_env_removals<I, S>(mut self, keys: I) -> Result<Self, AgentError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let keys: Vec<String> = keys.into_iter().map(Into::into).collect();
        CommandSpec::new("probe", "/")?.with_env_removals(keys.clone())?;
        self.env_removals = keys;
        Ok(self)
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
            .field("extra_args_count", &self.extra_args.len())
            .field("terminal_title", &self.terminal_title)
            .field("executable_override", &self.executable_override)
            .field(
                "startup_input",
                &self.startup_input.as_ref().map(|_| "[redacted]"),
            )
            .field("env_keys", &self.env_additions.keys().collect::<Vec<_>>())
            .field("env_removals", &self.env_removals)
            .field("placeholders", &self.placeholders)
            .finish()
    }
}

/// Static and detected metadata for one adapter.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDefinition {
    /// Stable registry key such as `codex`.
    pub id: String,
    /// User-facing adapter name.
    pub display_name: String,
    /// Bare executable name or configured path template.
    pub executable: String,
    /// Default arguments, never a shell string.
    pub default_args: Vec<String>,
    /// Whether the adapter is bundled or user-defined.
    pub source: AgentSource,
    /// Whether detection found an executable in the supplied environment.
    pub installed: bool,
    /// Resolved absolute path when detection succeeded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_path: Option<PathBuf>,
    /// Version preview populated only by diagnostics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Launch capabilities advertised by the adapter.
    pub capabilities: AgentCapabilities,
    /// Safe warning for the UI. Never includes secrets.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// Context-independent metadata exposed by an adapter.
pub type AdapterDefinition = AgentDefinition;

/// Capabilities advertised to the session layer.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    /// Whether the agent is an interactive CLI.
    pub interactive: bool,
    /// Whether the session layer should allocate a PTY.
    pub requires_pty: bool,
    /// Whether a `--version` probe is considered safe for this adapter.
    pub supports_version_probe: bool,
    /// Whether user-configured extra arguments may be appended.
    pub extra_args_allowed: bool,
}

impl AgentCapabilities {
    /// Interactive PTY agent with optional extra args and a version probe.
    pub const INTERACTIVE_PTY: Self = Self {
        interactive: true,
        requires_pty: true,
        supports_version_probe: true,
        extra_args_allowed: true,
    };

    /// Custom interactive agent. Version probing is opt-in through [`crate::test_executable`].
    #[must_use]
    pub const fn custom(requires_pty: bool) -> Self {
        Self {
            interactive: true,
            requires_pty,
            supports_version_probe: false,
            extra_args_allowed: true,
        }
    }
}

/// UI-facing diagnostics for one adapter.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDiagnostics {
    /// Stable registry key.
    pub id: String,
    /// User-facing name.
    pub display_name: String,
    /// Whether an executable was resolved.
    pub installed: bool,
    /// Resolved path when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// Version preview when probed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Launch-test outcome.
    pub launch_test: LaunchTestStatus,
    /// Safe warning.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    /// Directories searched for a bare executable name.
    pub searched_paths: Vec<PathBuf>,
}

impl From<ExecutableTestReport> for AgentDiagnostics {
    fn from(report: ExecutableTestReport) -> Self {
        Self {
            id: String::new(),
            display_name: String::new(),
            installed: report.installed,
            path: report.resolved_path,
            version: report.version,
            launch_test: report.launch_test,
            warning: report.warning,
            searched_paths: Vec::new(),
        }
    }
}

/// Uniform contract for bundled and custom coding-agent CLIs.
pub trait AgentAdapter: Send + Sync {
    /// Returns the stable registry key.
    fn key(&self) -> &str;

    /// Returns the user-facing name.
    fn display_name(&self) -> &str;

    /// Returns whether this adapter is bundled or user-defined.
    fn source(&self) -> AgentSource;

    /// Returns the configured executable name or path template.
    fn executable_name(&self) -> &str;

    /// Returns default arguments. Built-ins use an empty list.
    fn default_args(&self) -> &[String] {
        &[]
    }

    /// Returns environment additions from the adapter definition.
    fn environment_additions(&self) -> &BTreeMap<String, String> {
        &EMPTY_ENV
    }

    /// Returns environment keys the adapter wants removed.
    fn environment_removals(&self) -> &[String] {
        &[]
    }

    /// Returns launch capabilities.
    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities::INTERACTIVE_PTY
    }

    /// Returns context-independent adapter metadata.
    fn definition(&self) -> AgentDefinition {
        AgentDefinition {
            id: self.key().to_owned(),
            display_name: self.display_name().to_owned(),
            executable: self.executable_name().to_owned(),
            default_args: self.default_args().to_vec(),
            source: self.source(),
            installed: false,
            resolved_path: None,
            version: None,
            capabilities: self.capabilities(),
            warning: None,
        }
    }

    /// Detects the executable using only the supplied launch environment.
    fn detect(&self, environment: &LaunchEnvironment) -> DetectionResult {
        if placeholders::contains_placeholder(self.executable_name()) {
            return DetectionResult::NotFound;
        }
        match expand_leading_tilde(self.executable_name()) {
            Ok(expanded) => environment.detect(expanded),
            Err(_) => DetectionResult::NotFound,
        }
    }

    /// Combines static metadata with PATH detection. Does not spawn a process.
    fn resolve_definition(&self, environment: &LaunchEnvironment) -> AgentDefinition {
        let mut definition = self.definition();
        match self.detect(environment) {
            DetectionResult::Found { executable } => {
                definition.installed = true;
                definition.resolved_path = Some(executable);
            }
            DetectionResult::NotExecutable { candidate } => {
                definition.resolved_path = Some(candidate);
                definition.warning =
                    Some("a candidate exists but is not an executable regular file".to_owned());
            }
            DetectionResult::NotFound => {
                if placeholders::contains_placeholder(self.executable_name()) {
                    definition.warning = Some(
                        "executable contains placeholders and is resolved at launch".to_owned(),
                    );
                }
            }
        }
        definition
    }

    /// Produces a structured, shell-free command in the context directory.
    ///
    /// # Errors
    ///
    /// Returns an error when the working directory is invalid, the executable
    /// is unavailable, placeholders cannot be expanded, or the core command
    /// contract rejects a field.
    fn build_command(&self, context: &LaunchContext) -> Result<CommandSpec, AgentError> {
        build_launch_command(self, context)
    }

    /// Returns sanitized install diagnostics, optionally including a version probe.
    fn diagnostics(&self, environment: &LaunchEnvironment) -> AgentDiagnostics {
        self.diagnostics_with_options(environment, ProbeOptions::default())
    }

    /// Returns diagnostics with explicit probe options.
    fn diagnostics_with_options(
        &self,
        environment: &LaunchEnvironment,
        options: ProbeOptions,
    ) -> AgentDiagnostics {
        diagnose_adapter(self, environment, options)
    }
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

fn diagnose_adapter<A: AgentAdapter + ?Sized>(
    adapter: &A,
    environment: &LaunchEnvironment,
    options: ProbeOptions,
) -> AgentDiagnostics {
    let probe_options = if adapter.capabilities().supports_version_probe {
        options
    } else {
        options.without_version_probe()
    };
    let executable = match expand_leading_tilde(adapter.executable_name()) {
        Ok(path) => path,
        Err(_) => adapter.executable_name().to_owned(),
    };
    let mut report = test_executable(executable, environment, probe_options);
    if placeholders::contains_placeholder(adapter.executable_name()) && !report.installed {
        report.warning = Some(
            "executable contains placeholders; install status is determined at launch".to_owned(),
        );
    }
    AgentDiagnostics {
        id: adapter.key().to_owned(),
        display_name: adapter.display_name().to_owned(),
        installed: report.installed,
        path: report.resolved_path,
        version: report.version,
        launch_test: report.launch_test,
        warning: report.warning,
        searched_paths: environment.search_paths(),
    }
}

fn build_launch_command<A: AgentAdapter + ?Sized>(
    adapter: &A,
    context: &LaunchContext,
) -> Result<CommandSpec, AgentError> {
    context.validate_cwd()?;

    let executable_template = match &context.executable_override {
        Some(path) => path
            .to_str()
            .ok_or(AgentError::NonUtf8ExecutablePath)?
            .to_owned(),
        None => adapter.executable_name().to_owned(),
    };
    let executable = placeholders::expand(&executable_template, context.placeholders())?;
    let executable = expand_leading_tilde(&executable)?;
    let executable = resolved_executable(context.environment().detect(&executable))?;
    let executable = executable
        .to_str()
        .ok_or(AgentError::NonUtf8ExecutablePath)?;

    let mut args = adapter.default_args().to_vec();
    if adapter.capabilities().extra_args_allowed {
        args.extend(context.extra_args().iter().cloned());
    }
    let args = placeholders::expand_args(&args, context.placeholders())?;

    let mut env = adapter.environment_additions().clone();
    env.extend(context.env_additions.clone());
    let env = placeholders::expand_env(&env, context.placeholders())?;

    let mut removals = adapter.environment_removals().to_vec();
    removals.extend(context.env_removals.iter().cloned());

    let mut spec = CommandSpec::try_from_parts(executable, args, context.cwd(), env)?;
    spec = spec.with_env_removals(removals)?;
    if let Some(title) = &context.terminal_title {
        let title = placeholders::expand(title, context.placeholders())?;
        spec = spec.with_terminal_title(title)?;
    }
    if let Some(input) = &context.startup_input {
        spec = spec.with_startup_input(input.clone())?;
    }
    Ok(spec)
}
