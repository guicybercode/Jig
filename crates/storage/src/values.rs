use chrono::{DateTime, SecondsFormat, Utc};
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

pub(crate) fn rfc3339_timestamp_from_sql_value(
    value: ValueRef<'_>,
    entity: &'static str,
    field: &'static str,
) -> Result<String, StorageError> {
    match value {
        ValueRef::Integer(timestamp) => timestamp_to_sql_value(timestamp, entity, field),
        ValueRef::Text(bytes) => {
            let text = std::str::from_utf8(bytes)
                .map_err(|error| corrupt_data(entity, field, error.to_string()))?;
            if let Ok(timestamp) = text.parse::<i64>() {
                return timestamp_to_sql_value(timestamp, entity, field);
            }
            normalize_rfc3339_timestamp(text, entity, field)
        }
        _ => Err(corrupt_data(
            entity,
            field,
            "expected Unix epoch milliseconds or an RFC 3339 timestamp",
        )),
    }
}

pub(crate) fn timestamp_to_sql_value(
    timestamp: i64,
    entity: &'static str,
    field: &'static str,
) -> Result<String, StorageError> {
    DateTime::<Utc>::from_timestamp_millis(timestamp)
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Millis, true))
        .ok_or_else(|| corrupt_data(entity, field, "timestamp is outside the RFC 3339 range"))
}

pub(crate) fn normalize_rfc3339_timestamp(
    value: &str,
    entity: &'static str,
    field: &'static str,
) -> Result<String, StorageError> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| {
            timestamp
                .with_timezone(&Utc)
                .to_rfc3339_opts(SecondsFormat::AutoSi, true)
        })
        .map_err(|_| corrupt_data(entity, field, "expected an RFC 3339 timestamp"))
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

#[cfg(test)]
mod tests {
    use super::timestamp_to_sql_value;

    #[test]
    fn persisted_epoch_adapter_emits_rfc3339_utc() {
        assert_eq!(
            timestamp_to_sql_value(1_787_941_200_000, "test", "timestamp")
                .expect("valid epoch should convert"),
            "2026-08-28T18:20:00.000Z"
        );
    }
}
