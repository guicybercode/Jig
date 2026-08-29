use std::{collections::BTreeMap, fmt, sync::Arc};

use crate::{
    AgentAdapter, ClaudeCodeAdapter, CodexAdapter, DetectionResult, GeminiCliAdapter,
    LaunchEnvironment, OpenCodeAdapter, RegistryError,
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
    /// Returns an error if the key is empty or already registered.
    pub fn register<A>(&mut self, adapter: A) -> Result<(), RegistryError>
    where
        A: AgentAdapter + 'static,
    {
        self.register_shared(Arc::new(adapter))
    }

    /// Registers a shared adapter under its stable key.
    ///
    /// # Errors
    ///
    /// Returns an error if the key is empty or already registered.
    pub fn register_shared(&mut self, adapter: Arc<dyn AgentAdapter>) -> Result<(), RegistryError> {
        let key = adapter.key();
        if key.is_empty() {
            return Err(RegistryError::EmptyKey);
        }
        if self.adapters.contains_key(key) {
            return Err(RegistryError::DuplicateKey(key.to_owned()));
        }
        self.adapters.insert(key.to_owned(), adapter);
        Ok(())
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
