//! RFC 3339 timestamp helpers used by SQLite rows.

use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::StorageError;

/// Returns the current UTC time as an RFC 3339 string.
#[must_use]
pub fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

/// Parses an RFC 3339 timestamp into Unix epoch milliseconds.
///
/// # Errors
///
/// Returns an error when the stored timestamp is not valid RFC 3339.
pub fn rfc3339_to_ms(value: &str) -> Result<i64, StorageError> {
    let parsed = OffsetDateTime::parse(value, &Rfc3339).map_err(|error| {
        StorageError::InvalidTimestamp(format!("invalid RFC 3339 timestamp {value:?}: {error}"))
    })?;
    Ok(parsed.unix_timestamp() * 1_000 + i64::from(parsed.nanosecond() / 1_000_000))
}
