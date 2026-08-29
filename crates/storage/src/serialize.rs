use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cli_master_core::SessionStatus;

use crate::error::{EntityKind, StorageError};

const MAX_ENV_OVERRIDES: usize = 32;

pub(crate) fn required_text(
    value: &str,
    field: &'static str,
    entity: EntityKind,
) -> Result<(), StorageError> {
    if value.trim().is_empty() {
        Err(StorageError::invalid_input(
            "validate",
            entity,
            nonempty_reason(field),
        ))
    } else {
        Ok(())
    }
}

fn nonempty_reason(field: &'static str) -> &'static str {
    match field {
        "name" => "name must not be empty",
        "path" => "path must not be empty",
        "executable" => "executable must not be empty",
        "cwd" => "working directory must not be empty",
        "branch" => "branch must not be empty",
        "key" => "setting key must not be empty",
        _ => "value must not be empty",
    }
}

pub(crate) fn absolute_path(path: &Path, entity: EntityKind) -> Result<PathBuf, StorageError> {
    if path.as_os_str().is_empty() {
        return Err(StorageError::invalid_input(
            "validate path",
            entity,
            "path must not be empty",
        ));
    }
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Err(StorageError::invalid_input(
            "validate path",
            entity,
            "path must be absolute",
        ))
    }
}

pub(crate) fn encode_args(args: &[String]) -> Result<String, StorageError> {
    serde_json::to_string(args).map_err(|_| {
        StorageError::serialization("serialize args", "could not encode argument list")
    })
}

pub(crate) fn decode_args(json: &str) -> Result<Vec<String>, StorageError> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|_| {
        StorageError::serialization("decode args", "stored argument list is not valid JSON")
    })?;
    match value {
        serde_json::Value::Array(entries) => entries
            .into_iter()
            .map(|entry| {
                entry.as_str().map(ToOwned::to_owned).ok_or_else(|| {
                    StorageError::serialization(
                        "decode args",
                        "stored argument list must contain only strings",
                    )
                })
            })
            .collect(),
        _ => Err(StorageError::serialization(
            "decode args",
            "stored argument list must be a JSON array",
        )),
    }
}

pub(crate) fn encode_env(env: &BTreeMap<String, String>) -> Result<String, StorageError> {
    crate::secret::validate_allowed_env(env)?;
    serde_json::to_string(env).map_err(|_| {
        StorageError::serialization("serialize env", "could not encode environment overrides")
    })
}

pub(crate) fn decode_env(json: &str) -> Result<BTreeMap<String, String>, StorageError> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|_| {
        StorageError::serialization(
            "decode env",
            "stored environment overrides are not valid JSON",
        )
    })?;
    let serde_json::Value::Object(entries) = value else {
        return Err(StorageError::serialization(
            "decode env",
            "stored environment overrides must be a JSON object",
        ));
    };
    if entries.len() > MAX_ENV_OVERRIDES {
        return Err(StorageError::new(
            "decode env",
            EntityKind::Agent,
            crate::error::StorageErrorKind::FullEnvironmentRejected,
            "Remove inherited environment values. Only explicit non-secret overrides are stored.",
        ));
    }
    let mut env = BTreeMap::new();
    for (key, value) in entries {
        let Some(text) = value.as_str() else {
            return Err(StorageError::serialization(
                "decode env",
                "stored environment values must be strings",
            ));
        };
        env.insert(key, text.to_owned());
    }
    crate::secret::validate_allowed_env(&env)?;
    Ok(env)
}

pub(crate) fn session_status_to_db(status: SessionStatus) -> &'static str {
    match status {
        SessionStatus::Starting => "starting",
        SessionStatus::Running => "running",
        SessionStatus::Idle => "idle",
        SessionStatus::Exited => "exited",
        SessionStatus::Failed => "failed",
        SessionStatus::Unknown => "unknown",
    }
}

pub(crate) fn session_status_from_db(value: &str) -> SessionStatus {
    match value {
        "starting" => SessionStatus::Starting,
        "running" => SessionStatus::Running,
        "idle" => SessionStatus::Idle,
        "exited" => SessionStatus::Exited,
        "failed" => SessionStatus::Failed,
        _ => SessionStatus::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_args, decode_env, encode_args, encode_env, session_status_from_db};
    use cli_master_core::SessionStatus;
    use std::collections::BTreeMap;

    #[test]
    fn args_round_trip_as_json_array() {
        let args = vec!["--interactive".to_owned(), "value with space".to_owned()];
        let encoded = encode_args(&args).expect("args should encode");
        assert_eq!(encoded, r#"["--interactive","value with space"]"#);
        assert_eq!(decode_args(&encoded).expect("args should decode"), args);
    }

    #[test]
    fn env_round_trip_rejects_non_object() {
        decode_env("[]").expect_err("array is not an env object");
    }

    #[test]
    fn allowed_env_round_trips() {
        let env = BTreeMap::from([("PATH".to_owned(), "/usr/bin".to_owned())]);
        let encoded = encode_env(&env).expect("env should encode");
        assert_eq!(decode_env(&encoded).expect("env should decode"), env);
    }

    #[test]
    fn unknown_session_status_maps_to_unknown() {
        assert_eq!(session_status_from_db("paused"), SessionStatus::Unknown);
    }
}
