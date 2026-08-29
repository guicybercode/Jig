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
typed_id!(AgentId, "Unique identifier for an agent definition.");
typed_id!(SessionId, "Unique identifier for an agent session.");
typed_id!(WorktreeId, "Unique identifier for a managed Git worktree.");
typed_id!(RequestId, "Correlation identifier for an IPC request.");
typed_id!(
    DaemonInstanceId,
    "Unique identifier for one daemon process lifetime."
);

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
}
