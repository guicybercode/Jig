use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;

use crate::error::{EntityKind, StorageError};
use crate::serialize::required_text;
use crate::time::now_rfc3339;

pub(crate) struct SettingsRepository<'a> {
    connection: &'a Connection,
}

impl<'a> SettingsRepository<'a> {
    pub(crate) const fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub(crate) fn put(&self, key: &str, value: &Value) -> Result<(), StorageError> {
        required_text(key, "key", EntityKind::Setting)?;
        if crate::secret::setting_key_is_forbidden(key) {
            return Err(StorageError::new(
                "put",
                EntityKind::Setting,
                crate::error::StorageErrorKind::SecretRejected,
                "Do not store tokens in settings. Authentication stays with each CLI.",
            ));
        }
        let encoded = serde_json::to_string(value)
            .map_err(|_| StorageError::serialization("put", "could not encode setting value"))?;
        let timestamp = now_rfc3339()?;
        self.connection
            .execute(
                "INSERT INTO settings (key, value_json, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
                params![key.trim(), encoded, timestamp],
            )
            .map_err(|error| StorageError::from_sqlite("put", EntityKind::Setting, &error))?;
        Ok(())
    }

    pub(crate) fn get(&self, key: &str) -> Result<Option<Value>, StorageError> {
        let json: Option<String> = self
            .connection
            .query_row(
                "SELECT value_json FROM settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| StorageError::from_sqlite("get", EntityKind::Setting, &error))?;
        json.map(|encoded| {
            serde_json::from_str(&encoded)
                .map_err(|_| StorageError::serialization("get", "stored setting is not valid JSON"))
        })
        .transpose()
    }

    pub(crate) fn remove(&self, key: &str) -> Result<(), StorageError> {
        self.connection
            .execute("DELETE FROM settings WHERE key = ?1", params![key])
            .map_err(|error| StorageError::from_sqlite("remove", EntityKind::Setting, &error))?;
        Ok(())
    }
}
