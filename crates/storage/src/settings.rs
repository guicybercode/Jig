//! Small non-secret JSON settings persisted with the metadata database.

use rusqlite::{OptionalExtension, params};
use serde_json::Value;

use crate::Storage;
use crate::error::{StorageError, corrupt_data, invalid_input};
use crate::models::validate_timestamp;

const MAX_SETTING_KEY_BYTES: usize = 256;
const MAX_SETTING_VALUE_BYTES: usize = 1024 * 1024;

impl Storage {
    /// Inserts or replaces one non-secret setting.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid or secret-like key, oversized JSON,
    /// invalid timestamp, or database failure.
    pub fn put_setting(
        &self,
        key: &str,
        value: &Value,
        updated_at_ms: i64,
    ) -> Result<(), StorageError> {
        validate_setting_key(key)?;
        validate_timestamp("setting updated_at_ms", updated_at_ms)?;
        let encoded = serde_json::to_string(value)
            .map_err(|error| invalid_input("setting value", error.to_string()))?;
        if encoded.len() > MAX_SETTING_VALUE_BYTES {
            return Err(invalid_input(
                "setting value",
                format!("must be at most {MAX_SETTING_VALUE_BYTES} serialized bytes"),
            ));
        }

        self.with_connection("put setting", |connection| {
            connection.execute(
                "INSERT INTO settings (key, value_json, updated_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET
                    value_json = excluded.value_json,
                    updated_at = excluded.updated_at",
                params![key, encoded, updated_at_ms],
            )?;
            Ok(())
        })
    }

    /// Loads one setting value.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid key, malformed persisted JSON, or
    /// database failure.
    pub fn get_setting(&self, key: &str) -> Result<Option<Value>, StorageError> {
        validate_setting_key(key)?;
        self.with_connection("get setting", |connection| {
            let encoded = connection
                .query_row(
                    "SELECT value_json FROM settings WHERE key = ?1",
                    [key],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            encoded.map(|encoded| decode_setting(&encoded)).transpose()
        })
    }

    /// Removes one setting when present.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid key or database failure.
    pub fn remove_setting(&self, key: &str) -> Result<(), StorageError> {
        validate_setting_key(key)?;
        self.with_connection("remove setting", |connection| {
            connection.execute("DELETE FROM settings WHERE key = ?1", [key])?;
            Ok(())
        })
    }
}

fn decode_setting(encoded: &str) -> Result<Value, StorageError> {
    if encoded.len() > MAX_SETTING_VALUE_BYTES {
        return Err(corrupt_data(
            "setting",
            "value_json",
            format!("serialized value exceeds {MAX_SETTING_VALUE_BYTES} bytes"),
        ));
    }
    serde_json::from_str(encoded)
        .map_err(|error| corrupt_data("setting", "value_json", error.to_string()))
}

fn validate_setting_key(key: &str) -> Result<(), StorageError> {
    if key.trim().is_empty() {
        return Err(invalid_input("setting key", "must not be blank"));
    }
    if key.len() > MAX_SETTING_KEY_BYTES || key.contains('\0') {
        return Err(invalid_input(
            "setting key",
            format!("must be at most {MAX_SETTING_KEY_BYTES} bytes and contain no NUL byte"),
        ));
    }
    if looks_sensitive(key) {
        return Err(invalid_input(
            "setting key",
            "secret-bearing settings are not persisted in the metadata database",
        ));
    }
    Ok(())
}

fn looks_sensitive(key: &str) -> bool {
    let compact = key
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|character| character.to_ascii_uppercase())
        .collect::<String>();
    [
        "TOKEN",
        "SECRET",
        "PASSWORD",
        "PASSWD",
        "CREDENTIAL",
        "APIKEY",
        "PRIVATEKEY",
        "ACCESSKEY",
    ]
    .iter()
    .any(|marker| compact.contains(marker))
}
