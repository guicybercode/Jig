use std::collections::BTreeMap;

use cli_master_core::{AgentId, AgentSource};
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::error::{EntityKind, StorageError};
use crate::records::{NewCustomAgent, StoredAgent};
use crate::serialize::{decode_args, decode_env, encode_args, encode_env, required_text};
use crate::time::{now_rfc3339, rfc3339_to_unix_ms};

pub(crate) struct AgentRepository<'a> {
    connection: &'a Connection,
}

impl<'a> AgentRepository<'a> {
    pub(crate) const fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub(crate) fn upsert_builtin(
        &self,
        id: AgentId,
        name: &str,
        executable: &str,
        args: &[String],
    ) -> Result<StoredAgent, StorageError> {
        required_text(name, "name", EntityKind::Agent)?;
        required_text(executable, "executable", EntityKind::Agent)?;
        let timestamp = now_rfc3339()?;
        let args_json = encode_args(args)?;
        self.connection
            .execute(
                "INSERT INTO agents (
                    id, source, name, executable, args_json, env_json, enabled, created_at, updated_at
                ) VALUES (?1, 'built_in', ?2, ?3, ?4, '{}', 1, ?5, ?5)
                ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    executable = excluded.executable,
                    args_json = excluded.args_json,
                    updated_at = excluded.updated_at
                WHERE agents.source = 'built_in'",
                params![id.to_string(), name.trim(), executable.trim(), args_json, timestamp],
            )
            .map_err(|error| StorageError::from_sqlite("upsert", EntityKind::Agent, &error))?;

        let stored = self.get(id)?;
        if stored.source != AgentSource::BuiltIn {
            return Err(StorageError::conflict(
                "upsert",
                EntityKind::Agent,
                "cannot overwrite a custom agent with a built-in definition",
                "Choose a different agent id.",
            ));
        }
        Ok(stored)
    }

    pub(crate) fn insert_custom(&self, new: &NewCustomAgent) -> Result<StoredAgent, StorageError> {
        required_text(&new.name, "name", EntityKind::Agent)?;
        required_text(&new.executable, "executable", EntityKind::Agent)?;
        crate::secret::validate_allowed_env(&new.env)?;
        let timestamp = now_rfc3339()?;
        self.connection
            .execute(
                "INSERT INTO agents (
                    id, source, name, executable, args_json, env_json, enabled, created_at, updated_at
                ) VALUES (?1, 'custom', ?2, ?3, ?4, ?5, 1, ?6, ?6)",
                params![
                    new.id.to_string(),
                    new.name.trim(),
                    new.executable.trim(),
                    encode_args(&new.args)?,
                    encode_env(&new.env)?,
                    timestamp
                ],
            )
            .map_err(|error| {
                crate::repo::remap_constraint(
                    StorageError::from_sqlite("insert", EntityKind::Agent, &error),
                    StorageError::conflict(
                        "insert",
                        EntityKind::Agent,
                        "an agent with this id already exists",
                        "Use a new id or update the existing custom agent.",
                    ),
                )
            })?;
        self.get(new.id)
    }

    pub(crate) fn update_custom(
        &self,
        id: AgentId,
        name: &str,
        executable: &str,
        args: &[String],
        env: &BTreeMap<String, String>,
    ) -> Result<StoredAgent, StorageError> {
        required_text(name, "name", EntityKind::Agent)?;
        required_text(executable, "executable", EntityKind::Agent)?;
        crate::secret::validate_allowed_env(env)?;
        let current = self.get(id)?;
        if current.source != AgentSource::Custom {
            return Err(StorageError::conflict(
                "update",
                EntityKind::Agent,
                "built-in agent defaults cannot be mutated",
                "Disable the built-in agent or create a custom agent instead.",
            ));
        }
        let timestamp = now_rfc3339()?;
        self.connection
            .execute(
                "UPDATE agents
                 SET name = ?1, executable = ?2, args_json = ?3, env_json = ?4, updated_at = ?5
                 WHERE id = ?6 AND source = 'custom'",
                params![
                    name.trim(),
                    executable.trim(),
                    encode_args(args)?,
                    encode_env(env)?,
                    timestamp,
                    id.to_string()
                ],
            )
            .map_err(|error| StorageError::from_sqlite("update", EntityKind::Agent, &error))?;
        self.get(id)
    }

    pub(crate) fn set_enabled(
        &self,
        id: AgentId,
        enabled: bool,
    ) -> Result<StoredAgent, StorageError> {
        let timestamp = now_rfc3339()?;
        let updated = self
            .connection
            .execute(
                "UPDATE agents SET enabled = ?1, updated_at = ?2 WHERE id = ?3",
                params![i64::from(enabled), timestamp, id.to_string()],
            )
            .map_err(|error| StorageError::from_sqlite("update", EntityKind::Agent, &error))?;
        if updated == 0 {
            return Err(StorageError::not_found("update", EntityKind::Agent, id));
        }
        self.get(id)
    }

    pub(crate) fn remove_custom(&self, id: AgentId) -> Result<(), StorageError> {
        let current = self.get(id)?;
        if current.source != AgentSource::Custom {
            return Err(StorageError::conflict(
                "remove",
                EntityKind::Agent,
                "built-in agents cannot be deleted",
                "Disable the built-in agent instead.",
            ));
        }
        let session_count: i64 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE agent_id = ?1",
                params![id.to_string()],
                |row| row.get(0),
            )
            .map_err(|error| StorageError::from_sqlite("remove", EntityKind::Agent, &error))?;
        if session_count > 0 {
            return Err(StorageError::conflict(
                "remove",
                EntityKind::Agent,
                "agent is still referenced by session history",
                "Disable the agent to preserve history. Deleting it is blocked.",
            ));
        }
        self.connection
            .execute(
                "DELETE FROM agents WHERE id = ?1 AND source = 'custom'",
                params![id.to_string()],
            )
            .map_err(|error| StorageError::from_sqlite("remove", EntityKind::Agent, &error))?;
        Ok(())
    }

    pub(crate) fn get(&self, id: AgentId) -> Result<StoredAgent, StorageError> {
        self.connection
            .query_row(
                "SELECT id, source, name, executable, args_json, env_json, enabled, created_at, updated_at
                 FROM agents WHERE id = ?1",
                params![id.to_string()],
                map_agent_row,
            )
            .optional()
            .map_err(|error| StorageError::from_sqlite("get", EntityKind::Agent, &error))?
            .ok_or_else(|| StorageError::not_found("get", EntityKind::Agent, id))
    }

    pub(crate) fn list(&self) -> Result<Vec<StoredAgent>, StorageError> {
        self.list_filtered(None)
    }

    pub(crate) fn list_custom(&self) -> Result<Vec<StoredAgent>, StorageError> {
        self.list_filtered(Some(AgentSource::Custom))
    }

    fn list_filtered(&self, source: Option<AgentSource>) -> Result<Vec<StoredAgent>, StorageError> {
        let sql = if source.is_some() {
            "SELECT id, source, name, executable, args_json, env_json, enabled, created_at, updated_at
             FROM agents WHERE source = ?1 ORDER BY name COLLATE NOCASE"
        } else {
            "SELECT id, source, name, executable, args_json, env_json, enabled, created_at, updated_at
             FROM agents ORDER BY source, name COLLATE NOCASE"
        };
        let mut statement = self
            .connection
            .prepare(sql)
            .map_err(|error| StorageError::from_sqlite("list", EntityKind::Agent, &error))?;
        let mapped = if let Some(source) = source {
            statement
                .query_map(params![source_to_db(source)], map_agent_row)
                .map_err(|error| StorageError::from_sqlite("list", EntityKind::Agent, &error))?
                .collect::<Result<Vec<_>, _>>()
        } else {
            statement
                .query_map([], map_agent_row)
                .map_err(|error| StorageError::from_sqlite("list", EntityKind::Agent, &error))?
                .collect::<Result<Vec<_>, _>>()
        };
        mapped.map_err(|error| StorageError::from_sqlite("list", EntityKind::Agent, &error))
    }
}

fn source_to_db(source: AgentSource) -> &'static str {
    match source {
        AgentSource::BuiltIn => "built_in",
        AgentSource::Custom => "custom",
    }
}

fn source_from_db(value: &str) -> AgentSource {
    match value {
        "built_in" => AgentSource::BuiltIn,
        _ => AgentSource::Custom,
    }
}

fn map_agent_row(row: &Row<'_>) -> rusqlite::Result<StoredAgent> {
    let id: String = row.get("id")?;
    let source: String = row.get("source")?;
    let args_json: String = row.get("args_json")?;
    let env_json: String = row.get("env_json")?;
    let created_at: String = row.get("created_at")?;
    let updated_at: String = row.get("updated_at")?;
    let enabled: i64 = row.get("enabled")?;

    Ok(StoredAgent {
        id: id.parse().map_err(conversion_error)?,
        source: source_from_db(&source),
        name: row.get("name")?,
        executable: row.get("executable")?,
        args: decode_args(&args_json).map_err(conversion_error)?,
        env: decode_env(&env_json).map_err(conversion_error)?,
        enabled: enabled != 0,
        created_at_ms: rfc3339_to_unix_ms(&created_at).map_err(conversion_error)?,
        updated_at_ms: rfc3339_to_unix_ms(&updated_at).map_err(conversion_error)?,
    })
}

fn conversion_error<E>(error: E) -> rusqlite::Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}
