use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::error::{EntityKind, StorageError};

pub(crate) fn now_rfc3339() -> Result<String, StorageError> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|_| StorageError::serialization("timestamp", "could not format the current time"))
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn unix_ms_to_rfc3339(ms: i64) -> Result<String, StorageError> {
    let nanos = i128::from(ms).saturating_mul(1_000_000);
    OffsetDateTime::from_unix_timestamp_nanos(nanos)
        .map_err(|_| {
            StorageError::invalid_input("timestamp", EntityKind::Database, "out of range")
        })?
        .format(&Rfc3339)
        .map_err(|_| StorageError::serialization("timestamp", "could not format timestamp"))
}

pub(crate) fn rfc3339_to_unix_ms(value: &str) -> Result<i64, StorageError> {
    let parsed = OffsetDateTime::parse(value, &Rfc3339).map_err(|_| {
        StorageError::invalid_input(
            "parse timestamp",
            EntityKind::Database,
            "must be a UTC RFC 3339 timestamp",
        )
    })?;
    let millis = parsed.unix_timestamp_nanos() / 1_000_000;
    i64::try_from(millis).map_err(|_| {
        StorageError::invalid_input("parse timestamp", EntityKind::Database, "out of range")
    })
}

#[cfg(test)]
mod tests {
    use super::{now_rfc3339, rfc3339_to_unix_ms, unix_ms_to_rfc3339};

    #[test]
    fn rfc3339_round_trips_unix_millis() {
        let encoded = unix_ms_to_rfc3339(1_724_860_800_000).expect("timestamp should format");
        assert!(encoded.ends_with('Z') || encoded.contains('+'));
        assert_eq!(
            rfc3339_to_unix_ms(&encoded).expect("timestamp should parse"),
            1_724_860_800_000
        );
    }

    #[test]
    fn now_is_parseable_rfc3339() {
        let now = now_rfc3339().expect("now should format");
        rfc3339_to_unix_ms(&now).expect("now should parse");
    }
}
