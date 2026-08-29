use std::{collections::BTreeMap, fmt, sync::Arc};

use crate::{
    AgentAdapter, AgentDefinition, AgentDiagnostics, ClaudeCodeAdapter, CodexAdapter,
    CustomAgentAdapter, CustomAgentDefinition, DetectionResult, GeminiCliAdapter,
    LaunchEnvironment, OpenCodeAdapter, ProbeOptions, RegistryError,
};

/// Registry that treats built-in and custom adapters uniformly.
pub struct AgentRegistry {
    adapters: BTreeMap<String, Arc<dyn AgentAdapter>>,
}

impl AgentRegistry {
    /// Creates a registry containing the four Beta v0.1 built-ins.
    #[must_use]
    pub fn new() -> Self {
        let mut registry = Self::empty();
        registry.insert_builtin(Arc::new(CodexAdapter));
        registry.insert_builtin(Arc::new(ClaudeCodeAdapter));
        registry.insert_builtin(Arc::new(GeminiCliAdapter));
        registry.insert_builtin(Arc::new(OpenCodeAdapter));
        registry
    }

    /// Creates an empty registry for explicit composition or tests.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            adapters: BTreeMap::new(),
        }
    }

    /// Registers an adapter under its stable key.
    ///
    /// # Errors
    ///
    /// Returns an error if the key is empty, already registered, or the display
    /// name collides with another adapter.
    pub fn register<A>(&mut self, adapter: A) -> Result<(), RegistryError>
    where
        A: AgentAdapter + 'static,
    {
        self.register_shared(Arc::new(adapter))
    }

    /// Registers a validated custom agent definition.
    ///
    /// # Errors
    ///
    /// Returns an error if the key or display name is already registered.
    pub fn register_custom(
        &mut self,
        definition: CustomAgentDefinition,
    ) -> Result<(), RegistryError> {
        self.register(CustomAgentAdapter::new(definition))
    }

    /// Replaces an existing custom definition. Built-ins cannot be replaced.
    ///
    /// # Errors
    ///
    /// Returns an error if the key is missing, names a built-in, or the new
    /// display name collides with a different adapter.
    pub fn replace_custom(
        &mut self,
        definition: CustomAgentDefinition,
    ) -> Result<(), RegistryError> {
        let key = definition.key().to_owned();
        let previous = self.unregister(&key)?;
        if let Err(error) = self.register_custom(definition) {
            self.adapters.insert(previous.key().to_owned(), previous);
            return Err(error);
        }
        Ok(())
    }

    /// Registers a shared adapter under its stable key.
    ///
    /// # Errors
    ///
    /// Returns an error if the key is empty, already registered, or the display
    /// name collides with another adapter.
    pub fn register_shared(&mut self, adapter: Arc<dyn AgentAdapter>) -> Result<(), RegistryError> {
        let key = adapter.key();
        if key.is_empty() {
            return Err(RegistryError::EmptyKey);
        }
        if self.adapters.contains_key(key) {
            return Err(RegistryError::DuplicateKey(key.to_owned()));
        }
        let display_name = adapter.display_name();
        if self
            .adapters
            .values()
            .any(|existing| existing.display_name().eq_ignore_ascii_case(display_name))
        {
            return Err(RegistryError::DuplicateDisplayName(display_name.to_owned()));
        }
        self.adapters.insert(key.to_owned(), adapter);
        Ok(())
    }

    /// Removes a custom adapter. Built-ins are protected.
    ///
    /// # Errors
    ///
    /// Returns an error if the key is unknown or names a built-in.
    pub fn unregister(&mut self, key: &str) -> Result<Arc<dyn AgentAdapter>, RegistryError> {
        let source = self
            .adapters
            .get(key)
            .ok_or_else(|| RegistryError::NotFound(key.to_owned()))?
            .source();
        if source == cli_master_core::AgentSource::BuiltIn {
            return Err(RegistryError::BuiltInProtected(key.to_owned()));
        }
        self.adapters
            .remove(key)
            .ok_or_else(|| RegistryError::NotFound(key.to_owned()))
    }

    /// Returns an adapter by stable key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&dyn AgentAdapter> {
        self.adapters.get(key).map(AsRef::as_ref)
    }

    /// Returns registered keys in stable lexical order.
    pub fn keys(&self) -> impl ExactSizeIterator<Item = &str> {
        self.adapters.keys().map(String::as_str)
    }

    /// Detects every registered executable using one explicit environment.
    #[must_use]
    pub fn detect_all(&self, environment: &LaunchEnvironment) -> BTreeMap<String, DetectionResult> {
        self.adapters
            .iter()
            .map(|(key, adapter)| (key.clone(), adapter.detect(environment)))
            .collect()
    }

    /// Returns catalog snapshots without spawning version probes.
    #[must_use]
    pub fn snapshots(&self, environment: &LaunchEnvironment) -> Vec<AgentDefinition> {
        self.adapters
            .values()
            .map(|adapter| adapter.resolve_definition(environment))
            .collect()
    }

    /// Returns diagnostics for one adapter.
    ///
    /// # Errors
    ///
    /// Returns an error if the key is not registered.
    pub fn diagnostics(
        &self,
        key: &str,
        environment: &LaunchEnvironment,
        options: ProbeOptions,
    ) -> Result<AgentDiagnostics, RegistryError> {
        let adapter = self
            .get(key)
            .ok_or_else(|| RegistryError::NotFound(key.to_owned()))?;
        Ok(adapter.diagnostics_with_options(environment, options))
    }

    /// Returns the number of registered adapters.
    #[must_use]
    pub fn len(&self) -> usize {
        self.adapters.len()
    }

    /// Returns whether the registry contains no adapters.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.adapters.is_empty()
    }

    fn insert_builtin(&mut self, adapter: Arc<dyn AgentAdapter>) {
        let key = adapter.key().to_owned();
        let previous = self.adapters.insert(key, adapter);
        debug_assert!(previous.is_none(), "built-in adapter keys must be unique");
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for AgentRegistry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentRegistry")
            .field("keys", &self.adapters.keys().collect::<Vec<_>>())
            .finish()
    }
}
