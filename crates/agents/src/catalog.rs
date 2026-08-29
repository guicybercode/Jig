use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use cli_master_core::{
    AgentCustomCreateRequest, AgentCustomUpdateRequest, AgentDetectResponse,
    AgentDiagnosticsReport, AgentId, AgentListResponse, AgentRecord, AgentSource,
    LaunchTestStatusDto, builtin_agent_ids,
};

use crate::{
    AgentRegistry, CustomAgentDefinition, CustomDefinitionError, LaunchEnvironment,
    LaunchTestStatus, ProbeOptions, RegistryError,
};

struct CatalogEntry {
    id: AgentId,
    adapter_key: String,
    enabled: bool,
    installed: bool,
    resolved_path: Option<PathBuf>,
    version: Option<String>,
    warning: Option<String>,
    env: BTreeMap<String, String>,
    custom: Option<CustomAgentDefinition>,
}

/// UUID-addressed agent catalog used by `agent.list`, `detect`, `set_enabled`,
/// and custom CRUD. Adapter keys stay internal.
pub struct AgentCatalog {
    registry: AgentRegistry,
    entries: BTreeMap<AgentId, CatalogEntry>,
    environment: LaunchEnvironment,
}

impl AgentCatalog {
    /// Seeds the four built-ins with stable `UUIDv7` identifiers.
    #[must_use]
    pub fn new(environment: LaunchEnvironment) -> Self {
        let registry = AgentRegistry::new();
        let mut catalog = Self {
            registry,
            entries: BTreeMap::new(),
            environment,
        };
        catalog.seed_builtin(builtin_agent_ids::codex(), "codex");
        catalog.seed_builtin(builtin_agent_ids::claude(), "claude");
        catalog.seed_builtin(builtin_agent_ids::gemini(), "gemini");
        catalog.seed_builtin(builtin_agent_ids::opencode(), "opencode");
        catalog
    }

    /// Returns catalog rows without environment values.
    #[must_use]
    pub fn list(&self) -> AgentListResponse {
        let mut agents: Vec<AgentRecord> = self
            .entries
            .values()
            .map(|entry| self.record(entry))
            .collect();
        agents.sort_by(|left, right| match (left.source, right.source) {
            (AgentSource::BuiltIn, AgentSource::Custom) => std::cmp::Ordering::Less,
            (AgentSource::Custom, AgentSource::BuiltIn) => std::cmp::Ordering::Greater,
            _ => left.display_name.cmp(&right.display_name),
        });
        AgentListResponse { agents }
    }

    /// Probes one agent or every agent using the configured launch environment.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogError::NotFound`] when `agent_id` is unknown.
    pub fn detect(
        &mut self,
        agent_id: Option<AgentId>,
    ) -> Result<AgentDetectResponse, CatalogError> {
        let ids: Vec<AgentId> = match agent_id {
            Some(id) => {
                if !self.entries.contains_key(&id) {
                    return Err(CatalogError::NotFound(id));
                }
                vec![id]
            }
            None => self.entries.keys().copied().collect(),
        };

        let mut diagnostics = Vec::new();
        for id in ids {
            diagnostics.push(self.detect_one(id)?);
        }
        Ok(AgentDetectResponse {
            agents: self.list().agents,
            diagnostics,
        })
    }

    /// Enables or disables an agent. Built-in command defaults are not changed.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogError::NotFound`] when the id is unknown.
    pub fn set_enabled(
        &mut self,
        agent_id: AgentId,
        enabled: bool,
    ) -> Result<AgentRecord, CatalogError> {
        let entry = self
            .entries
            .get_mut(&agent_id)
            .ok_or(CatalogError::NotFound(agent_id))?;
        entry.enabled = enabled;
        let snapshot = self
            .entries
            .get(&agent_id)
            .ok_or(CatalogError::NotFound(agent_id))?;
        Ok(self.record(snapshot))
    }

    /// Creates a custom agent from structured fields.
    ///
    /// # Errors
    ///
    /// Returns a validation or duplicate-name error. Environment values are not
    /// included in the error.
    pub fn create_custom(
        &mut self,
        request: AgentCustomCreateRequest,
    ) -> Result<AgentRecord, CatalogError> {
        let id = AgentId::new();
        let adapter_key = custom_adapter_key(&request.display_name, id);
        let mut definition = CustomAgentDefinition::try_from_parts(
            adapter_key.clone(),
            request.display_name,
            request.executable,
            request.args,
            request.env.clone(),
        )?;
        if let Some(cwd) = request.default_cwd {
            definition = definition.with_default_cwd(cwd)?;
        }
        definition = definition.with_requires_pty(request.requires_pty);
        self.registry
            .register_custom(definition.clone())
            .map_err(CatalogError::from)?;
        let entry = CatalogEntry {
            id,
            adapter_key,
            enabled: true,
            installed: false,
            resolved_path: None,
            version: None,
            warning: None,
            env: request.env,
            custom: Some(definition),
        };
        self.entries.insert(id, entry);
        self.detect_one(id)?;
        self.entries
            .get(&id)
            .map(|entry| self.record(entry))
            .ok_or(CatalogError::NotFound(id))
    }

    /// Replaces a custom agent definition.
    ///
    /// # Errors
    ///
    /// Built-ins cannot be updated. Unknown ids fail.
    pub fn update_custom(
        &mut self,
        request: AgentCustomUpdateRequest,
    ) -> Result<AgentRecord, CatalogError> {
        let current = self
            .entries
            .get(&request.agent_id)
            .ok_or(CatalogError::NotFound(request.agent_id))?;
        if current.custom.is_none() {
            return Err(CatalogError::BuiltInProtected(request.agent_id));
        }
        let adapter_key = current.adapter_key.clone();
        let enabled = current.enabled;
        let mut definition = CustomAgentDefinition::try_from_parts(
            adapter_key.clone(),
            request.display_name,
            request.executable,
            request.args,
            request.env.clone(),
        )?;
        if let Some(cwd) = request.default_cwd {
            definition = definition.with_default_cwd(cwd)?;
        }
        definition = definition.with_requires_pty(request.requires_pty);
        self.registry.replace_custom(definition.clone())?;
        let entry = CatalogEntry {
            id: request.agent_id,
            adapter_key,
            enabled,
            installed: false,
            resolved_path: None,
            version: None,
            warning: None,
            env: request.env,
            custom: Some(definition),
        };
        self.entries.insert(request.agent_id, entry);
        self.detect_one(request.agent_id)?;
        self.entries
            .get(&request.agent_id)
            .map(|entry| self.record(entry))
            .ok_or(CatalogError::NotFound(request.agent_id))
    }

    /// Removes a custom agent. Built-ins cannot be removed. Disk files remain.
    ///
    /// # Errors
    ///
    /// Returns an error for unknown or built-in agents.
    pub fn remove_custom(&mut self, agent_id: AgentId) -> Result<(), CatalogError> {
        let entry = self
            .entries
            .get(&agent_id)
            .ok_or(CatalogError::NotFound(agent_id))?;
        if entry.custom.is_none() {
            return Err(CatalogError::BuiltInProtected(agent_id));
        }
        let adapter_key = entry.adapter_key.clone();
        self.registry.unregister(&adapter_key)?;
        self.entries.remove(&agent_id);
        Ok(())
    }

    fn seed_builtin(&mut self, id: AgentId, adapter_key: &str) {
        self.entries.insert(
            id,
            CatalogEntry {
                id,
                adapter_key: adapter_key.to_owned(),
                enabled: true,
                installed: false,
                resolved_path: None,
                version: None,
                warning: None,
                env: BTreeMap::new(),
                custom: None,
            },
        );
    }

    fn detect_one(&mut self, id: AgentId) -> Result<AgentDiagnosticsReport, CatalogError> {
        let adapter_key = self
            .entries
            .get(&id)
            .ok_or(CatalogError::NotFound(id))?
            .adapter_key
            .clone();
        let adapter = self
            .registry
            .get(&adapter_key)
            .ok_or(CatalogError::NotFound(id))?;
        let report = adapter.diagnostics_with_options(&self.environment, ProbeOptions::default());
        let entry = self
            .entries
            .get_mut(&id)
            .ok_or(CatalogError::NotFound(id))?;
        entry.installed = report.installed;
        entry.resolved_path.clone_from(&report.path);
        entry.version.clone_from(&report.version);
        entry.warning.clone_from(&report.warning);
        Ok(AgentDiagnosticsReport {
            agent_id: id,
            display_name: report.display_name,
            installed: report.installed,
            launch_test: launch_test_dto(report.launch_test),
            searched_paths: report.searched_paths,
            path: report.path,
            version: report.version,
            warning: report.warning,
        })
    }

    fn record(&self, entry: &CatalogEntry) -> AgentRecord {
        let adapter = self
            .registry
            .get(&entry.adapter_key)
            .expect("catalog entries are registered");
        AgentRecord {
            id: entry.id,
            adapter_key: entry.adapter_key.clone(),
            display_name: adapter.display_name().to_owned(),
            source: adapter.source(),
            enabled: entry.enabled,
            installed: entry.installed,
            executable: adapter.executable_name().to_owned(),
            default_args: adapter.default_args().to_vec(),
            env_keys: entry.env.keys().cloned().collect(),
            requires_pty: adapter.capabilities().requires_pty,
            resolved_path: entry.resolved_path.clone(),
            version: entry.version.clone(),
            warning: entry.warning.clone(),
            default_cwd: entry
                .custom
                .as_ref()
                .and_then(CustomAgentDefinition::default_cwd)
                .map(ToOwned::to_owned),
        }
    }
}

fn custom_adapter_key(display_name: &str, id: AgentId) -> String {
    let mut slug = String::new();
    for character in display_name.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    let suffix = &id.to_string()[..8];
    if slug.is_empty() {
        format!("custom-{suffix}")
    } else {
        format!("custom-{slug}-{suffix}")
    }
}

fn launch_test_dto(status: LaunchTestStatus) -> LaunchTestStatusDto {
    match status {
        LaunchTestStatus::Success => LaunchTestStatusDto::Success,
        LaunchTestStatus::NotFound => LaunchTestStatusDto::NotFound,
        LaunchTestStatus::NotExecutable { candidate } => {
            LaunchTestStatusDto::NotExecutable { candidate }
        }
        LaunchTestStatus::Timeout => LaunchTestStatusDto::Timeout,
        LaunchTestStatus::Failed { message } => LaunchTestStatusDto::Failed { message },
    }
}

/// Failure for catalog IPC methods.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CatalogError {
    /// No agent exists for the public identifier.
    NotFound(AgentId),
    /// Built-in agents cannot be edited or removed.
    BuiltInProtected(AgentId),
    /// Custom definition validation failed.
    InvalidDefinition(CustomDefinitionError),
    /// Registry rejected a key or display name.
    Registry(RegistryError),
}

impl CatalogError {
    /// Converts this error into a stable IPC error without secret values.
    #[must_use]
    pub fn api_error(&self) -> cli_master_core::ApiError {
        match self {
            Self::NotFound(id) => cli_master_core::ApiError::new(
                "AGENT_NOT_FOUND",
                format!("No agent is registered for id {id}"),
            )
            .with_action("Refresh the agent list and try again."),
            Self::BuiltInProtected(_) => cli_master_core::ApiError::new(
                "AGENT_BUILTIN_PROTECTED",
                "Built-in agents can be disabled but not edited or removed.",
            )
            .with_action("Create a custom agent if you need different arguments."),
            Self::InvalidDefinition(error) => {
                cli_master_core::ApiError::new("AGENT_INVALID_DEFINITION", error.to_string())
                    .with_action("Fix the highlighted field. Arguments must stay an array.")
            }
            Self::Registry(error) => {
                cli_master_core::ApiError::new("AGENT_REGISTRY_ERROR", error.to_string())
                    .with_action("Choose a unique name.")
            }
        }
    }
}

impl fmt::Display for CatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(id) => write!(formatter, "agent not found: {id}"),
            Self::BuiltInProtected(id) => {
                write!(formatter, "built-in agent cannot be modified: {id}")
            }
            Self::InvalidDefinition(error) => write!(formatter, "{error}"),
            Self::Registry(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for CatalogError {}

impl From<CustomDefinitionError> for CatalogError {
    fn from(value: CustomDefinitionError) -> Self {
        Self::InvalidDefinition(value)
    }
}

impl From<RegistryError> for CatalogError {
    fn from(value: RegistryError) -> Self {
        Self::Registry(value)
    }
}
