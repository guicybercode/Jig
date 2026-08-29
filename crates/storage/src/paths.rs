use std::path::{Path, PathBuf};

use rusqlite::types::{Value, ValueRef};

use crate::StorageError;
use crate::error::{corrupt_data, invalid_input, persisted_validation};

pub(crate) fn validate_absolute_path(path: &Path, field: &'static str) -> Result<(), StorageError> {
    if path.as_os_str().is_empty() {
        return Err(invalid_input(field, "path must not be empty"));
    }
    if path.as_os_str().as_encoded_bytes().contains(&0) {
        return Err(invalid_input(field, "path must not contain a NUL byte"));
    }
    if !path.is_absolute() {
        return Err(invalid_input(field, "path must be absolute"));
    }
    Ok(())
}

pub(crate) fn path_to_sql_value(path: &Path, field: &'static str) -> Result<Value, StorageError> {
    validate_absolute_path(path, field)?;

    if let Some(text) = path.to_str() {
        return Ok(Value::Text(text.to_owned()));
    }

    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;

        Ok(Value::Blob(path.as_os_str().as_bytes().to_vec()))
    }

    #[cfg(not(unix))]
    {
        Err(invalid_input(
            field,
            "path must be valid UTF-8 on this platform",
        ))
    }
}

pub(crate) fn path_from_sql_value(
    value: ValueRef<'_>,
    entity: &'static str,
    field: &'static str,
) -> Result<PathBuf, StorageError> {
    let path = match value {
        ValueRef::Text(bytes) | ValueRef::Blob(bytes) => {
            #[cfg(unix)]
            {
                use std::ffi::OsString;
                use std::os::unix::ffi::OsStringExt;

                PathBuf::from(OsString::from_vec(bytes.to_vec()))
            }

            #[cfg(not(unix))]
            {
                let text = std::str::from_utf8(bytes)
                    .map_err(|error| corrupt_data(entity, field, error.to_string()))?;
                PathBuf::from(text)
            }
        }
        _ => {
            return Err(corrupt_data(
                entity,
                field,
                "expected a UTF-8 text path or native path byte sequence",
            ));
        }
    };
    persisted_validation(entity, validate_absolute_path(&path, field))?;
    Ok(path)
}
