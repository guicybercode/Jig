use std::{collections::BTreeMap, fmt, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{AgentId, AgentSource};

/// Stable public identifiers for the four built-in adapters.
///
/// These are `UUIDv7` values used on the wire and in SQLite. Adapter keys such as
/// `codex` remain internal lookup names and must never be used as [`AgentId`].
pub mod builtin_agent_ids {
    use uuid::Uuid;

    use crate::AgentId;

    const fn id(value: u128) -> AgentId {
        AgentId::from_uuid(Uuid::from_u128(value))
    }

    /// Public identifier for the Codex built-in.
    #[must_use]
    pub const fn codex() -> AgentId {
        id(0x0193_6a10_0000_7000_8000_0000_0000_0001)
    }

    /// Public identifier for the Claude Code built-in.
    #[must_use]
    pub const fn claude() -> AgentId {
        id(0x0193_6a10_0000_7000_8000_0000_0000_0002)
    }

    /// Public identifier for the Gemini CLI built-in.
    #[must_use]
    pub const fn gemini() -> AgentId {
        id(0x0193_6a10_0000_7000_8000_0000_0000_0003)
    }

    /// Public identifier for the `OpenCode` built-in.
    #[must_use]
    pub const fn opencode() -> AgentId {
        id(0x0193_6a10_0000_7000_8000_0000_0000_0004)
    }
}

/// IPC method names for the agent catalog.
pub mod agent_methods {
    /// `agent.list`
    pub const LIST: &str = "agent.list";
    /// `agent.detect`
    pub const DETECT: &str = "agent.detect";
    /// `agent.set_enabled`
    pub const SET_ENABLED: &str = "agent.set_enabled";
    /// `agent.custom.create`
    pub const CUSTOM_CREATE: &str = "agent.custom.create";
    /// `agent.custom.update`
    pub const CUSTOM_UPDATE: &str = "agent.custom.update";
    /// `agent.custom.remove`
    pub const CUSTOM_REMOVE: &str = "agent.custom.remove";
}

/// Public catalog row. Environment values are never included.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRecord {
    /// Persisted `UUIDv7` identifier.
    pub id: AgentId,
    /// Internal adapter key such as `codex`. Not a public identifier.
    pub adapter_key: String,
    /// User-facing name.
    pub display_name: String,
    /// Built-in or custom origin.
    pub source: AgentSource,
    /// Whether the agent may be selected for new sessions.
    pub enabled: bool,
    /// Whether the executable was resolved.
    pub installed: bool,
    /// Bare executable name or configured path.
    pub executable: String,
    /// Default argument array. Never a shell string.
    pub default_args: Vec<String>,
    /// Environment variable names only.
    pub env_keys: Vec<String>,
    /// Whether a PTY should be allocated.
    pub requires_pty: bool,
    /// Resolved absolute path when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_path: Option<PathBuf>,
    /// Version preview from a bounded probe.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Safe warning for the UI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    /// Optional default working directory template.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_cwd: Option<String>,
}

/// Sanitized diagnostics for one agent. Never includes environment values.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDiagnosticsReport {
    /// Public agent identifier.
    pub agent_id: AgentId,
    /// User-facing name.
    pub display_name: String,
    /// Whether an executable was resolved.
    pub installed: bool,
    /// Launch-test outcome.
    pub launch_test: LaunchTestStatusDto,
    /// Directories searched for a bare executable name.
    pub searched_paths: Vec<PathBuf>,
    /// Resolved path when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// Version preview when probed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Safe warning.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// Launch-test result suitable for IPC and copyable diagnostics.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum LaunchTestStatusDto {
    /// The executable was resolved and the optional version probe did not hang.
    Success,
    /// No candidate was found.
    NotFound,
    /// A candidate exists but cannot be executed.
    NotExecutable {
        /// Path that failed the executable check.
        candidate: PathBuf,
    },
    /// The version probe exceeded its timeout.
    Timeout,
    /// The process could not be started.
    Failed {
        /// Safe explanation without captured output or secrets.
        message: String,
    },
}

/// Empty list request.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentListRequest {}

/// List response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentListResponse {
    /// Catalog rows without secrets.
    pub agents: Vec<AgentRecord>,
}

/// Detect one agent or every registered agent.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDetectRequest {
    /// When omitted, every agent is probed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<AgentId>,
}

/// Detect response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDetectResponse {
    /// Updated catalog rows.
    pub agents: Vec<AgentRecord>,
    /// Per-agent diagnostics without environment values.
    pub diagnostics: Vec<AgentDiagnosticsReport>,
}

/// Enable or disable an agent without mutating its command.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSetEnabledRequest {
    /// Public agent identifier.
    pub agent_id: AgentId,
    /// Desired enabled flag.
    pub enabled: bool,
}

/// Create a custom agent from structured fields.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCustomCreateRequest {
    /// User-facing name.
    pub display_name: String,
    /// Absolute path, `~/` path, or bare executable name.
    pub executable: String,
    /// Ordered arguments. Never a shell string.
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment additions. Values are stored, never logged.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Optional default working directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_cwd: Option<String>,
    /// Whether a PTY is required.
    #[serde(default = "default_requires_pty")]
    pub requires_pty: bool,
}

const fn default_requires_pty() -> bool {
    true
}

impl fmt::Debug for AgentCustomCreateRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentCustomCreateRequest")
            .field("display_name", &self.display_name)
            .field("executable", &self.executable)
            .field("args_count", &self.args.len())
            .field("env_keys", &self.env.keys().collect::<Vec<_>>())
            .field("default_cwd", &self.default_cwd)
            .field("requires_pty", &self.requires_pty)
            .finish()
    }
}

/// Update a custom agent. Built-ins cannot be updated.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCustomUpdateRequest {
    /// Public agent identifier.
    pub agent_id: AgentId,
    /// User-facing name.
    pub display_name: String,
    /// Absolute path, `~/` path, or bare executable name.
    pub executable: String,
    /// Ordered arguments.
    #[serde(default)]
    pub args: Vec<String>,
    /// Replacement environment additions. Omitted keys are not inferred from secrets.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Optional default working directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_cwd: Option<String>,
    /// Whether a PTY is required.
    #[serde(default = "default_requires_pty")]
    pub requires_pty: bool,
}

impl fmt::Debug for AgentCustomUpdateRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentCustomUpdateRequest")
            .field("agent_id", &self.agent_id)
            .field("display_name", &self.display_name)
            .field("executable", &self.executable)
            .field("args_count", &self.args.len())
            .field("env_keys", &self.env.keys().collect::<Vec<_>>())
            .field("default_cwd", &self.default_cwd)
            .field("requires_pty", &self.requires_pty)
            .finish()
    }
}

/// Remove a custom agent definition. Files on disk are not deleted.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCustomRemoveRequest {
    /// Public agent identifier.
    pub agent_id: AgentId,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_ids_are_uuid_v7_and_not_adapter_keys() {
        for (key, id) in [
            ("codex", builtin_agent_ids::codex()),
            ("claude", builtin_agent_ids::claude()),
            ("gemini", builtin_agent_ids::gemini()),
            ("opencode", builtin_agent_ids::opencode()),
        ] {
            assert_eq!(id.as_uuid().get_version_num(), 7);
            assert_ne!(id.to_string(), key);
            assert!(!id.to_string().contains(key));
        }
    }

    #[test]
    fn agent_record_omits_environment_values() {
        let record = AgentRecord {
            id: builtin_agent_ids::codex(),
            adapter_key: "codex".to_owned(),
            display_name: "Codex".to_owned(),
            source: AgentSource::BuiltIn,
            enabled: true,
            installed: true,
            executable: "codex".to_owned(),
            default_args: Vec::new(),
            env_keys: vec!["OPENAI_API_KEY".to_owned()],
            requires_pty: true,
            resolved_path: Some(PathBuf::from("/home/user/.local/bin/codex")),
            version: Some("codex 1.0".to_owned()),
            warning: None,
            default_cwd: None,
        };
        let json = serde_json::to_string(&record).expect("record should serialize");
        assert!(json.contains("adapterKey"));
        assert!(json.contains("envKeys"));
        assert!(!json.contains("sk-"));
        assert!(!json.contains("envValues"));
    }

    #[test]
    fn custom_create_debug_redacts_environment_values() {
        let request = AgentCustomCreateRequest {
            display_name: "Internal".to_owned(),
            executable: "internal-agent".to_owned(),
            args: vec!["--token=argument-secret".to_owned()],
            env: BTreeMap::from([("ACCESS_TOKEN".to_owned(), "super-secret".to_owned())]),
            default_cwd: None,
            requires_pty: true,
        };
        let debug = format!("{request:?}");
        assert!(debug.contains("ACCESS_TOKEN"));
        assert!(!debug.contains("super-secret"));
        assert!(!debug.contains("argument-secret"));
    }

    #[test]
    fn detect_request_round_trips_optional_id() {
        let request = AgentDetectRequest {
            agent_id: Some(builtin_agent_ids::claude()),
        };
        let json = serde_json::to_string(&request).expect("request should serialize");
        let decoded: AgentDetectRequest =
            serde_json::from_str(&json).expect("request should deserialize");
        assert_eq!(decoded, request);
        assert!(json.contains("agentId"));
    }
}
