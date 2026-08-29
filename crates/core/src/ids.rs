use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, de};
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
typed_id!(
    DaemonInstanceId,
    "Unique identifier for one `cli-masterd` process lifetime."
);

/// Stable identifier for a built-in or custom agent definition.
///
/// Built-in IDs are catalog keys (`codex`, `claude`, `gemini`, `opencode`).
/// Custom agents use a `UUIDv7` string generated at creation time. This is not
/// a UUID-only newtype because the built-in catalog must survive reinstalls.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct AgentId(String);

impl<'de> Deserialize<'de> for AgentId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse_str(value).map_err(de::Error::custom)
    }
}

impl AgentId {
    /// Built-in Codex adapter key.
    pub const CODEX: &str = "codex";
    /// Built-in Claude Code adapter key.
    pub const CLAUDE: &str = "claude";
    /// Built-in Gemini CLI adapter key.
    pub const GEMINI: &str = "gemini";
    /// Built-in `OpenCode` adapter key.
    pub const OPENCODE: &str = "opencode";

    /// Creates a custom-agent identifier from a new `UUIDv7`.
    #[must_use]
    pub fn new_custom() -> Self {
        Self(Uuid::now_v7().to_string())
    }

    /// Parses a built-in key or previously issued custom identifier.
    ///
    /// # Errors
    ///
    /// Returns an error when the value is empty, contains a NUL byte, or uses
    /// characters outside ASCII letters, digits, `.`, `-`, and `_`.
    pub fn parse_str(value: impl AsRef<str>) -> Result<Self, AgentIdError> {
        let value = value.as_ref();
        validate_agent_id(value)?;
        Ok(Self(value.to_owned()))
    }

    /// Returns the wire identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns whether this ID is one of the four built-in catalog keys.
    #[must_use]
    pub fn is_builtin(&self) -> bool {
        matches!(
            self.0.as_str(),
            Self::CODEX | Self::CLAUDE | Self::GEMINI | Self::OPENCODE
        )
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
        Self::parse_str(value)
    }
}

impl TryFrom<String> for AgentId {
    type Error = AgentIdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse_str(value)
    }
}

/// Rejection for an [`AgentId`] that cannot be stored or sent on the wire.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentIdError {
    /// The identifier was empty.
    Empty,
    /// The identifier contained a NUL byte or a disallowed character.
    InvalidCharset,
}

impl fmt::Display for AgentIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("agent id must not be empty"),
            Self::InvalidCharset => {
                formatter.write_str("agent id must use only ASCII letters, digits, '.', '-' or '_'")
            }
        }
    }
}

impl std::error::Error for AgentIdError {}

fn validate_agent_id(value: &str) -> Result<(), AgentIdError> {
    if value.is_empty() {
        return Err(AgentIdError::Empty);
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(AgentIdError::InvalidCharset);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_session_ids_are_uuid_v7() {
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
    fn builtin_agent_ids_are_stable_keys() {
        let id = AgentId::parse_str(AgentId::CODEX).expect("codex key should parse");
        assert!(id.is_builtin());
        assert_eq!(id.as_str(), "codex");
        assert_eq!(serde_json::to_string(&id).expect("serialize"), "\"codex\"");
    }

    #[test]
    fn custom_agent_ids_are_uuid_v7_strings() {
        let id = AgentId::new_custom();
        let uuid = Uuid::parse_str(id.as_str()).expect("custom id should be a UUID");
        assert_eq!(uuid.get_version_num(), 7);
        assert!(!id.is_builtin());
    }

    #[test]
    fn agent_id_rejects_empty_and_whitespace_keys() {
        assert_eq!(AgentId::parse_str(""), Err(AgentIdError::Empty));
        assert_eq!(
            AgentId::parse_str("codex cli"),
            Err(AgentIdError::InvalidCharset)
        );
    }
}
