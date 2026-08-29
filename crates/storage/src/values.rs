use chrono::DateTime;
use rusqlite::types::ValueRef;

use crate::StorageError;
use crate::error::corrupt_data;

pub(crate) fn timestamp_from_sql_value(
    value: ValueRef<'_>,
    entity: &'static str,
    field: &'static str,
) -> Result<i64, StorageError> {
    match value {
        ValueRef::Integer(timestamp) => Ok(timestamp),
        ValueRef::Text(bytes) => parse_timestamp_text(bytes, entity, field),
        _ => Err(corrupt_data(
            entity,
            field,
            "expected Unix epoch milliseconds or an RFC 3339 timestamp",
        )),
    }
}

pub(crate) fn optional_timestamp_from_sql_value(
    value: ValueRef<'_>,
    entity: &'static str,
    field: &'static str,
) -> Result<Option<i64>, StorageError> {
    if value == ValueRef::Null {
        Ok(None)
    } else {
        timestamp_from_sql_value(value, entity, field).map(Some)
    }
}

fn parse_timestamp_text(
    bytes: &[u8],
    entity: &'static str,
    field: &'static str,
) -> Result<i64, StorageError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|error| corrupt_data(entity, field, error.to_string()))?;
    if let Ok(timestamp) = text.parse::<i64>() {
        return Ok(timestamp);
    }

    let parsed = DateTime::parse_from_rfc3339(text).map_err(|_| {
        corrupt_data(
            entity,
            field,
            "expected decimal Unix epoch milliseconds or RFC 3339",
        )
    })?;
    Ok(parsed.timestamp_millis())
}
