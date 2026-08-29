//! Agent metadata persistence.

use std::str::FromStr;

use cli_master_core::{AgentId, AgentSource};
use rusqlite::{Row, params};

use crate::Storage;
use crate::error::{
    StorageError, corrupt_data, map_delete_error, map_write_error, persisted_validation,
};
use crate::models::{
    MAX_COMMAND_JSON_BYTES, StoredAgent, agent_source_from_database, agent_source_to_database,
    validate_timestamp,
};
use crate::values::{timestamp_from_sql_value, timestamp_to_sql_value};

const AGENT_COLUMNS: &str =
    "id, source, name, executable, args_json, env_json, enabled, created_at, updated_at";

impl Storage {
    /// Inserts a validated built-in or custom agent definition.
    ///
    /// Arguments and environment overrides are encoded as separate JSON values;
    /// they are never flattened into a shell command. Environment overrides
    /// are restricted to non-secret configuration; obvious secret-bearing key
    /// names are rejected before serialization.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid command metadata, duplicate IDs, or database failures.
    pub fn insert_agent(&self, agent: &StoredAgent) -> Result<(), StorageError> {
        agent.validate()?;
        let args_json = encode_json(&agent.args, "agent args")?;
        let env_json = encode_json(&agent.env, "agent env")?;
        let created_at = timestamp_to_sql_value(agent.created_at_ms, "agent", "created_at")?;
        let updated_at = timestamp_to_sql_value(agent.updated_at_ms, "agent", "updated_at")?;
        self.connection
            .execute(
                "INSERT INTO agents (
                    id, source, name, executable, args_json, env_json,
                    enabled, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    agent.id.to_string(),
                    agent_source_to_database(agent.source),
                    agent.display_name,
                    agent.executable,
                    args_json,
                    env_json,
                    agent.enabled,
                    created_at,
                    updated_at,
                ],
            )
            .map_err(|error| map_write_error(error, "agent"))?;
        Ok(())
    }

    /// Lists built-in and custom agent definitions in display order.
    ///
    /// # Errors
    ///
    /// Returns an error if rows cannot be loaded or decoded.
    pub fn list_agents(&self) -> Result<Vec<StoredAgent>, StorageError> {
        let sql = format!(
            "SELECT {AGENT_COLUMNS} FROM agents
             ORDER BY source, name COLLATE NOCASE, id"
        );
        let mut statement = self.connection.prepare(&sql)?;
        let mut rows = statement.query([])?;
        let mut agents = Vec::new();
        while let Some(row) = rows.next()? {
            agents.push(decode_agent(row)?);
        }
        Ok(agents)
    }

    /// Loads one agent definition by ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the row cannot be loaded or decoded.
    pub fn get_agent(&self, id: &AgentId) -> Result<Option<StoredAgent>, StorageError> {
        let sql = format!("SELECT {AGENT_COLUMNS} FROM agents WHERE id = ?1");
        let mut statement = self.connection.prepare(&sql)?;
        let mut rows = statement.query([id.to_string()])?;
        rows.next()?.map(decode_agent).transpose()
    }

    /// Replaces mutable metadata for an existing custom agent definition.
    ///
    /// The stable ID, source, and original creation timestamp are not changed.
    /// Both the stored row and the supplied definition must have custom source.
    /// Built-in command definitions are immutable and can only be enabled or
    /// disabled through [`Self::set_agent_enabled`].
    ///
    /// # Errors
    ///
    /// Returns an error for invalid metadata, a missing agent, or database failures.
    pub fn update_custom_agent(&self, agent: &StoredAgent) -> Result<(), StorageError> {
        if agent.source != AgentSource::Custom {
            return Err(custom_agent_required());
        }
        agent.validate()?;
        let args_json = encode_json(&agent.args, "agent args")?;
        let env_json = encode_json(&agent.env, "agent env")?;
        let updated_at = timestamp_to_sql_value(agent.updated_at_ms, "agent", "updated_at")?;
        let changed = self.connection.execute(
            "UPDATE agents
             SET name = ?1, executable = ?2, args_json = ?3,
                 env_json = ?4, enabled = ?5, updated_at = ?6
             WHERE id = ?7 AND source = 'custom'",
            params![
                agent.display_name,
                agent.executable,
                args_json,
                env_json,
                agent.enabled,
                updated_at,
                agent.id.to_string(),
            ],
        )?;
        require_custom_agent_changed(self, changed, &agent.id)
    }

    /// Enables or disables any built-in or custom agent definition.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid timestamp, a missing agent, or database failures.
    pub fn set_agent_enabled(
        &self,
        id: &AgentId,
        enabled: bool,
        updated_at_ms: i64,
    ) -> Result<(), StorageError> {
        validate_timestamp("agent updated_at_ms", updated_at_ms)?;
        let updated_at = timestamp_to_sql_value(updated_at_ms, "agent", "updated_at")?;
        let changed = self.connection.execute(
            "UPDATE agents SET enabled = ?1, updated_at = ?2 WHERE id = ?3",
            params![enabled, updated_at, id.to_string()],
        )?;
        require_changed(changed, id)
    }

    /// Removes only an agent metadata row.
    ///
    /// Sessions referencing the agent protect it through a foreign key.
    ///
    /// # Errors
    ///
    /// Returns an error if the agent is missing, still referenced, or deletion fails.
    pub fn remove_agent_metadata(&self, id: &AgentId) -> Result<(), StorageError> {
        let changed = self
            .connection
            .execute("DELETE FROM agents WHERE id = ?1", [id.to_string()])
            .map_err(|error| map_delete_error(error, "agent", "its sessions"))?;
        require_changed(changed, id)
    }
}

fn decode_agent(row: &Row<'_>) -> Result<StoredAgent, StorageError> {
    let id_text: String = row.get(0)?;
    let source_text: String = row.get(1)?;
    let args_json: String = row.get(4)?;
    let env_json: String = row.get(5)?;
    let agent = StoredAgent {
        id: AgentId::from_str(&id_text)
            .map_err(|error| corrupt_data("agent", "id", error.to_string()))?,
        source: agent_source_from_database(&source_text)?,
        display_name: row.get(2)?,
        executable: row.get(3)?,
        args: decode_json(&args_json, "args_json")?,
        env: decode_json(&env_json, "env_json")?,
        enabled: row.get(6)?,
        created_at_ms: timestamp_from_sql_value(row.get_ref(7)?, "agent", "created_at")?,
        updated_at_ms: timestamp_from_sql_value(row.get_ref(8)?, "agent", "updated_at")?,
    };
    persisted_validation("agent", agent.validate())?;
    Ok(agent)
}

fn encode_json<T: serde::Serialize>(
    value: &T,
    field: &'static str,
) -> Result<String, StorageError> {
    let encoded = serde_json::to_string(value)
        .map_err(|error| crate::error::invalid_input(field, error.to_string()))?;
    if encoded.len() > MAX_COMMAND_JSON_BYTES {
        return Err(crate::error::invalid_input(
            field,
            format!("serialized value must be at most {MAX_COMMAND_JSON_BYTES} bytes"),
        ));
    }
    Ok(encoded)
}

fn decode_json<T: serde::de::DeserializeOwned>(
    encoded: &str,
    field: &'static str,
) -> Result<T, StorageError> {
    if encoded.len() > MAX_COMMAND_JSON_BYTES {
        return Err(corrupt_data(
            "agent",
            field,
            format!("serialized value exceeds {MAX_COMMAND_JSON_BYTES} bytes"),
        ));
    }
    serde_json::from_str(encoded).map_err(|error| corrupt_data("agent", field, error.to_string()))
}

fn require_changed(changed: usize, id: &AgentId) -> Result<(), StorageError> {
    if changed == 0 {
        Err(StorageError::NotFound {
            entity: "agent",
            id: id.to_string(),
        })
    } else {
        Ok(())
    }
}

fn require_custom_agent_changed(
    storage: &Storage,
    changed: usize,
    id: &AgentId,
) -> Result<(), StorageError> {
    if changed > 0 {
        return Ok(());
    }
    match storage.get_agent(id)? {
        Some(_) => Err(custom_agent_required()),
        None => Err(StorageError::NotFound {
            entity: "agent",
            id: id.to_string(),
        }),
    }
}

fn custom_agent_required() -> StorageError {
    crate::error::invalid_input(
        "agent source",
        "only custom agent definitions may change command metadata",
    )
}
