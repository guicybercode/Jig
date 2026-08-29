use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! typed_id {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(
            Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Generates a time-ordered UUID version 7 identifier.
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            /// Wraps an existing UUID without changing it.
            #[must_use]
            pub const fn from_uuid(value: Uuid) -> Self {
                Self(value)
            }

            /// Returns the underlying UUID.
            #[must_use]
            pub const fn as_uuid(&self) -> &Uuid {
                &self.0
            }

            /// Consumes this identifier and returns its underlying UUID.
            #[must_use]
            pub const fn into_uuid(self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(value).map(Self)
            }
        }

        impl From<Uuid> for $name {
            fn from(value: Uuid) -> Self {
                Self(value)
            }
        }

        impl From<$name> for Uuid {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

typed_id!(ProjectId, "Unique identifier for a registered project.");
typed_id!(SessionId, "Unique identifier for an agent session.");
typed_id!(WorktreeId, "Unique identifier for a managed Git worktree.");
typed_id!(RequestId, "Correlation identifier for an IPC request.");

/// Stable agent registry key.
///
/// Built-in adapters use fixed keys such as `codex`. Custom agents use a
/// caller-chosen key or a generated `UUIDv7` string. This is intentionally not a
/// UUID newtype: the storage schema and adapter registry share these keys.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct AgentId(String);

impl AgentId {
    /// Generates a `UUIDv7` string suitable for a new custom agent.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7().to_string())
    }

    /// Wraps a validated registry key such as `codex` or a custom agent key.
    ///
    /// # Errors
    ///
    /// Returns an error when the key is empty, contains a NUL byte, or uses
    /// characters other than ASCII letters, digits, `.`, `-`, or `_`.
    pub fn from_key(key: impl Into<String>) -> Result<Self, AgentIdError> {
        let key = key.into();
        validate_agent_key(&key)?;
        Ok(Self(key))
    }

    /// Returns the registry key.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for AgentId {
    type Err = AgentIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::from_key(value)
    }
}

impl AsRef<str> for AgentId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<'de> serde::Deserialize<'de> for AgentId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let key = String::deserialize(deserializer)?;
        Self::from_key(key).map_err(serde::de::Error::custom)
    }
}

/// Validation failure for an [`AgentId`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentIdError {
    /// The key was empty.
    Empty,
    /// The key contained a NUL byte.
    ContainsNul,
    /// The key used a character outside the allowed set.
    InvalidCharacter,
}

impl fmt::Display for AgentIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("agent id must not be empty"),
            Self::ContainsNul => formatter.write_str("agent id must not contain a NUL byte"),
            Self::InvalidCharacter => {
                formatter.write_str("agent id must use only ASCII letters, digits, '.', '-' or '_'")
            }
        }
    }
}

impl std::error::Error for AgentIdError {}

fn validate_agent_key(key: &str) -> Result<(), AgentIdError> {
    if key.is_empty() {
        return Err(AgentIdError::Empty);
    }
    if key.contains('\0') {
        return Err(AgentIdError::ContainsNul);
    }
    if !key
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(AgentIdError::InvalidCharacter);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_ids_are_uuid_v7() {
        let id = SessionId::new();
        assert_eq!(id.as_uuid().get_version_num(), 7);
    }

    #[test]
    fn typed_id_round_trips_as_json_string() {
        let id = ProjectId::new();
        let json = serde_json::to_string(&id).expect("project id should serialize");
        let decoded: ProjectId =
            serde_json::from_str(&json).expect("project id should deserialize");

        assert_eq!(decoded, id);
        assert_eq!(json, format!("\"{id}\""));
    }

    #[test]
    fn different_id_types_are_distinct_but_preserve_uuid() {
        let uuid = Uuid::now_v7();
        let project_id = ProjectId::from_uuid(uuid);
        let session_id = SessionId::from_uuid(uuid);

        assert_eq!(project_id.to_string(), session_id.to_string());
        assert_eq!(project_id.into_uuid(), uuid);
    }

    #[test]
    fn builtin_agent_keys_round_trip() {
        let id = AgentId::from_key("codex").expect("built-in key should parse");
        let json = serde_json::to_string(&id).expect("agent id should serialize");
        let decoded: AgentId = serde_json::from_str(&json).expect("agent id should deserialize");

        assert_eq!(decoded, id);
        assert_eq!(json, "\"codex\"");
    }

    #[test]
    fn agent_id_rejects_spaces() {
        let error = AgentId::from_key("bad key").expect_err("spaces should be rejected");
        assert_eq!(error, AgentIdError::InvalidCharacter);
    }
}
