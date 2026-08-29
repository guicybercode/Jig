use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{AgentId, ProjectId, SessionId, WorktreeId};

use super::{
    ConfirmationToken, DisplayName, ExecutableName, GitRelativePath, OutputCursor, PtyInputBase64,
    RelativeDirectory, SelectedProjectPath, TerminalDimension, WireValidationError,
};

const MAX_AGENT_ARGUMENTS: usize = 256;
const MAX_AGENT_ARGUMENT_BYTES: usize = 8 * 1_024;
const MAX_AGENT_ENVIRONMENT_ENTRIES: usize = 128;
const MAX_AGENT_ENVIRONMENT_BYTES: usize = 64 * 1_024;
const MAX_DETECTION_IDS: usize = 256;

/// Empty object used by read-only methods that accept no parameters.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EmptyRequest {}

/// Request to register a user-selected project directory.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectAddRequest {
    /// Absolute path selected through the native folder picker.
    pub path: SelectedProjectPath,
    /// Optional user-facing name; the daemon derives one when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<DisplayName>,
}

/// Request to rename registered project metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectRenameRequest {
    /// Registered project to update.
    pub project_id: ProjectId,
    /// New canonical user-facing name.
    pub name: DisplayName,
}

/// Request to remove only registered project metadata.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectRemoveRequest {
    /// Registered project to remove.
    pub project_id: ProjectId,
}

/// Structured local agent configuration passed directly to a process API.
///
/// Arguments and environment overrides are intentionally serialized across the
/// local authenticated IPC boundary so custom definitions can be edited. Its
/// `Debug` representation never includes argument or environment values.
#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCommand {
    executable: ExecutableName,
    args: Vec<String>,
    env: BTreeMap<String, String>,
}

impl AgentCommand {
    /// Creates and validates a structured custom-agent command.
    ///
    /// # Errors
    ///
    /// Returns an error when argument or environment limits are exceeded, values
    /// contain NUL bytes, the executable is unsafe, or an environment key is not portable.
    pub fn try_new(
        executable: impl Into<String>,
        args: Vec<String>,
        env: BTreeMap<String, String>,
    ) -> Result<Self, WireValidationError> {
        let executable = executable.into();
        validate_agent_command(&executable, &args, &env)?;
        let command = Self {
            executable: ExecutableName::from_validated(executable),
            args,
            env,
        };
        Ok(command)
    }

    /// Returns the bare executable name or absolute executable path.
    #[must_use]
    pub fn executable(&self) -> &str {
        self.executable.as_str()
    }

    /// Returns the ordered arguments passed without shell parsing.
    #[must_use]
    pub fn args(&self) -> &[String] {
        &self.args
    }

    /// Returns explicit non-secret environment overrides.
    #[must_use]
    pub const fn env(&self) -> &BTreeMap<String, String> {
        &self.env
    }
}

impl fmt::Debug for AgentCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentCommand")
            .field("executable", &self.executable)
            .field("args_count", &self.args.len())
            .field("env_keys", &self.env.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl<'de> Deserialize<'de> for AgentCommand {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        struct WireCommand {
            executable: String,
            #[serde(default)]
            args: Vec<String>,
            #[serde(default)]
            env: BTreeMap<String, String>,
        }

        let wire = WireCommand::deserialize(deserializer)?;
        Self::try_new(wire.executable, wire.args, wire.env).map_err(de::Error::custom)
    }
}

/// Validates executable, argument, and environment configuration shared by wire and storage.
///
/// Environment values are local configuration serialized over IPC, but keys
/// that conservatively indicate secrets are rejected so their values are never
/// persisted. Callers must inherit secrets or obtain them from a secret manager.
///
/// # Errors
///
/// Returns an error when the command violates executable, size, NUL, portable-key,
/// or secret-bearing environment constraints.
pub fn validate_agent_command(
    executable: &str,
    args: &[String],
    env: &BTreeMap<String, String>,
) -> Result<(), WireValidationError> {
    ExecutableName::try_new(executable.to_owned())?;
    if args.len() > MAX_AGENT_ARGUMENTS {
        return Err(WireValidationError::new(
            "args",
            "must contain at most 256 entries",
        ));
    }
    let mut total_bytes = 0usize;
    for argument in args {
        if argument.len() > MAX_AGENT_ARGUMENT_BYTES {
            return Err(WireValidationError::new(
                "args",
                "each argument must be at most 8192 UTF-8 bytes",
            ));
        }
        if argument.contains('\0') {
            return Err(WireValidationError::new(
                "args",
                "must not contain a NUL byte",
            ));
        }
        total_bytes = total_bytes.saturating_add(argument.len());
    }
    if env.len() > MAX_AGENT_ENVIRONMENT_ENTRIES {
        return Err(WireValidationError::new(
            "env",
            "must contain at most 128 entries",
        ));
    }
    for (key, value) in env {
        if !is_portable_environment_key(key) {
            return Err(WireValidationError::new(
                "env",
                "keys must use portable environment-variable syntax",
            ));
        }
        if is_sensitive_environment_key(key) {
            return Err(WireValidationError::new(
                "env",
                "secret-bearing variables must be inherited or supplied by a secret manager",
            ));
        }
        if value.contains('\0') {
            return Err(WireValidationError::new(
                "env",
                "values must not contain a NUL byte",
            ));
        }
        total_bytes = total_bytes
            .saturating_add(key.len())
            .saturating_add(value.len());
    }
    if total_bytes > MAX_AGENT_ENVIRONMENT_BYTES {
        return Err(WireValidationError::new(
            "command",
            "arguments and environment must total at most 65536 UTF-8 bytes",
        ));
    }
    Ok(())
}

/// Request to create a custom agent definition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentCustomCreateRequest {
    /// User-facing custom-agent name.
    pub display_name: DisplayName,
    /// Shell-free command metadata.
    pub command: AgentCommand,
}

/// Request to replace the editable fields of a custom agent definition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentCustomUpdateRequest {
    /// Custom agent to update.
    pub agent_id: AgentId,
    /// Replacement user-facing name.
    pub display_name: DisplayName,
    /// Replacement shell-free command metadata.
    pub command: AgentCommand,
}

/// Request to enable or disable an agent definition.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentSetEnabledRequest {
    /// Agent definition to update.
    pub agent_id: AgentId,
    /// Whether new sessions may select the agent.
    pub enabled: bool,
}

/// Request to remove an unreferenced custom agent definition.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentCustomRemoveRequest {
    /// Custom agent to remove.
    pub agent_id: AgentId,
}

/// Request to detect some or all configured agent executables.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDetectRequest {
    agent_ids: Vec<AgentId>,
}

impl AgentDetectRequest {
    /// Creates a bounded detection request.
    ///
    /// # Errors
    ///
    /// Returns an error when more than 256 agent IDs are supplied.
    pub fn try_new(agent_ids: Vec<AgentId>) -> Result<Self, WireValidationError> {
        if agent_ids.len() > MAX_DETECTION_IDS {
            return Err(WireValidationError::new(
                "agent_ids",
                "must contain at most 256 entries",
            ));
        }
        Ok(Self { agent_ids })
    }

    /// Creates a request that detects every enabled agent.
    #[must_use]
    pub const fn all() -> Self {
        Self {
            agent_ids: Vec::new(),
        }
    }

    /// Returns requested IDs; an empty slice means every enabled agent.
    #[must_use]
    pub fn agent_ids(&self) -> &[AgentId] {
        &self.agent_ids
    }
}

impl<'de> Deserialize<'de> for AgentDetectRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        struct WireRequest {
            #[serde(default)]
            agent_ids: Vec<AgentId>,
        }

        Self::try_new(WireRequest::deserialize(deserializer)?.agent_ids).map_err(de::Error::custom)
    }
}

/// Daemon-derived repository isolation used by a new session.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionIsolation {
    /// Run in the registered project's current working tree.
    Current,
    /// Generate a branch and managed worktree from daemon-owned state.
    NewWorktree,
}

/// Request to create session metadata with a daemon-derived working directory.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SessionCreateRequest {
    /// Registered project that owns the session.
    pub project_id: ProjectId,
    /// User-facing session name, also used only as input to safe generated names.
    pub name: DisplayName,
    /// Enabled agent definition selected for this session.
    pub agent_id: AgentId,
    /// Whether the daemon uses the current tree or generates a managed worktree.
    pub isolation: SessionIsolation,
    /// Optional child directory below the daemon-selected project or worktree root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_directory: Option<RelativeDirectory>,
}

macro_rules! session_id_request {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        pub struct $name {
            /// Target session.
            pub session_id: SessionId,
        }
    };
}

session_id_request!(SessionStartRequest, "Request to start a stopped session.");
session_id_request!(
    SessionRestartRequest,
    "Request to restart a session with a fresh PTY."
);
session_id_request!(
    SessionStopRequest,
    "Request to gracefully stop a live session."
);
session_id_request!(
    SessionDeleteRequest,
    "Request to delete stopped-session metadata."
);
session_id_request!(
    SessionUnsubscribeRequest,
    "Request to stop terminal event delivery for one session."
);

/// Request to rename session metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SessionRenameRequest {
    /// Target session.
    pub session_id: SessionId,
    /// Replacement user-facing name.
    pub name: DisplayName,
}

/// Request to list sessions, optionally for one project.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SessionListRequest {
    /// Optional project filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
}

/// Request to write arbitrary bytes to a live session PTY.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SessionWriteRequest {
    /// Target live session.
    pub session_id: SessionId,
    /// Canonical base64 bytes, including control bytes such as Ctrl+C (`0x03`).
    pub base64: PtyInputBase64,
}

/// Request to resize a live session PTY.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SessionResizeRequest {
    /// Target live session.
    pub session_id: SessionId,
    /// Terminal column count.
    pub columns: TerminalDimension,
    /// Terminal row count.
    pub rows: TerminalDimension,
}

/// Request to replay and then follow terminal output for one session.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SessionSubscribeRequest {
    /// Target session.
    pub session_id: SessionId,
    /// Last output sequence already applied by the client; absent means replay from retained start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<OutputCursor>,
}

/// Registered entity whose daemon-derived repository path Git may inspect.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    deny_unknown_fields,
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum GitTarget {
    /// Inspect the registered repository root for a project.
    Project {
        /// Registered project identifier.
        project_id: ProjectId,
    },
    /// Inspect the daemon-recorded working directory for a session.
    Session {
        /// Registered session identifier.
        session_id: SessionId,
    },
    /// Inspect a daemon-recorded managed worktree.
    Worktree {
        /// Registered worktree identifier.
        worktree_id: WorktreeId,
    },
}

/// Request to inspect structured Git status.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GitStatusRequest {
    /// Typed registered entity; arbitrary filesystem paths are never accepted.
    pub target: GitTarget,
}

/// Request to read a daemon-bounded textual Git diff.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GitDiffRequest {
    /// Typed registered entity; arbitrary filesystem paths are never accepted.
    pub target: GitTarget,
    /// Optional repository-relative file. Absent means the combined target diff.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<GitRelativePath>,
}

/// Request to prepare safe managed-worktree removal.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct WorktreePrepareRemoveRequest {
    /// Managed worktree to re-inspect.
    pub worktree_id: WorktreeId,
}

/// Request to remove a worktree whose exact clean state was previously confirmed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct WorktreeRemoveRequest {
    /// Managed worktree to remove.
    pub worktree_id: WorktreeId,
    /// Short-lived token bound to the most recent safety inspection.
    pub confirmation_token: ConfirmationToken,
}

fn is_portable_environment_key(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn is_sensitive_environment_key(key: &str) -> bool {
    const SENSITIVE_SEGMENTS: &[&str] = &[
        "TOKEN",
        "SECRET",
        "PASSWORD",
        "PASSWD",
        "CREDENTIAL",
        "CREDENTIALS",
    ];
    const SENSITIVE_COMPOUNDS: &[&str] = &[
        "APIKEY",
        "PRIVATEKEY",
        "ACCESSKEY",
        "CLIENTSECRET",
        "AUTHTOKEN",
        "ACCESSTOKEN",
        "REFRESHTOKEN",
    ];
    let compact = key
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|character| character.to_ascii_uppercase())
        .collect::<String>();
    let segments = key
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|segment| !segment.is_empty())
        .map(str::to_ascii_uppercase)
        .collect::<Vec<_>>();
    segments.iter().any(|segment| {
        SENSITIVE_SEGMENTS.contains(&segment.as_str())
            || SENSITIVE_COMPOUNDS.contains(&segment.as_str())
    }) || SENSITIVE_SEGMENTS
        .iter()
        .chain(SENSITIVE_COMPOUNDS)
        .any(|suffix| compact.ends_with(suffix))
        || segments.windows(2).any(|pair| {
            matches!(
                (pair[0].as_str(), pair[1].as_str()),
                ("API" | "PRIVATE" | "ACCESS", "KEY")
                    | ("CLIENT", "SECRET")
                    | ("AUTH" | "ACCESS" | "REFRESH", "TOKEN")
            )
        })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn session_create_has_a_minimal_daemon_authoritative_shape() {
        let request = SessionCreateRequest {
            project_id: ProjectId::new(),
            name: DisplayName::try_new("Review auth").unwrap(),
            agent_id: AgentId::new(),
            isolation: SessionIsolation::NewWorktree,
            relative_directory: Some(RelativeDirectory::try_new("apps/api").unwrap()),
        };

        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({
                "projectId": request.project_id,
                "name": "Review auth",
                "agentId": request.agent_id,
                "isolation": "new_worktree",
                "relativeDirectory": "apps/api"
            })
        );
    }

    #[test]
    fn session_create_rejects_daemon_authoritative_fields() {
        let project_id = ProjectId::new();
        let agent_id = AgentId::new();
        for forbidden in [
            ("cwd", json!("/tmp/attacker")),
            ("branch", json!("main")),
            ("baseBranch", json!("main")),
            ("worktreePath", json!("/tmp/worktree")),
            ("additionalArgs", json!(["--unsafe"])),
        ] {
            let mut value = json!({
                "projectId": project_id,
                "name": "Safe",
                "agentId": agent_id,
                "isolation": "current"
            });
            value[forbidden.0] = forbidden.1;
            assert!(
                serde_json::from_value::<SessionCreateRequest>(value).is_err(),
                "accepted {}",
                forbidden.0
            );
        }
    }

    #[test]
    fn git_targets_use_snake_case_variants_and_camel_case_ids() {
        let project_id = ProjectId::new();
        let session_id = SessionId::new();
        let worktree_id = WorktreeId::new();
        let project = json!({ "kind": "project", "projectId": project_id });
        let session = json!({ "kind": "session", "sessionId": session_id });
        let worktree = json!({ "kind": "worktree", "worktreeId": worktree_id });

        assert_eq!(
            serde_json::from_value::<GitTarget>(project.clone()).unwrap(),
            GitTarget::Project { project_id }
        );
        assert_eq!(
            serde_json::from_value::<GitTarget>(session.clone()).unwrap(),
            GitTarget::Session { session_id }
        );
        assert_eq!(
            serde_json::from_value::<GitTarget>(worktree.clone()).unwrap(),
            GitTarget::Worktree { worktree_id }
        );
        assert_eq!(
            serde_json::to_value(GitTarget::Project { project_id }).unwrap(),
            project
        );
        assert_eq!(
            serde_json::to_value(GitTarget::Session { session_id }).unwrap(),
            session
        );
        assert_eq!(
            serde_json::to_value(GitTarget::Worktree { worktree_id }).unwrap(),
            worktree
        );
    }

    #[test]
    fn otherwise_valid_git_requests_reject_arbitrary_paths() {
        let project_id = ProjectId::new();
        let valid = json!({
            "target": { "kind": "project", "projectId": project_id }
        });
        assert!(serde_json::from_value::<GitStatusRequest>(valid.clone()).is_ok());

        let mut with_path = valid;
        with_path["path"] = json!("/tmp/not-registered");
        assert!(serde_json::from_value::<GitStatusRequest>(with_path).is_err());
        assert!(serde_json::from_value::<GitDiffRequest>(valid_git_diff(project_id)).is_ok());
        let mut unsupported_scope = valid_git_diff(project_id);
        unsupported_scope["scope"] = json!("staged");
        assert!(serde_json::from_value::<GitDiffRequest>(unsupported_scope).is_err());
        assert!(
            serde_json::from_value::<GitStatusRequest>(json!({
                "path": "/tmp/not-registered"
            }))
            .is_err()
        );
        let mut file_diff = valid_git_diff(project_id);
        file_diff["path"] = json!("src/lib.rs");
        assert!(serde_json::from_value::<GitDiffRequest>(file_diff).is_ok());
        for unsafe_path in ["/tmp/not-registered", "../secret", "-u", "--"] {
            let mut value = valid_git_diff(project_id);
            value["path"] = json!(unsafe_path);
            assert!(
                serde_json::from_value::<GitDiffRequest>(value).is_err(),
                "{unsafe_path}"
            );
        }
    }

    #[test]
    fn terminal_requests_use_bounded_base64_dimensions_and_cursor_fields() {
        let session_id = SessionId::new();
        let write: SessionWriteRequest = serde_json::from_value(json!({
            "sessionId": session_id,
            "base64": "Aw=="
        }))
        .unwrap();
        assert_eq!(write.base64.as_str(), "Aw==");

        assert!(
            serde_json::from_value::<SessionResizeRequest>(json!({
                "sessionId": session_id,
                "columns": 0,
                "rows": 24
            }))
            .is_err()
        );

        let subscribe: SessionSubscribeRequest = serde_json::from_value(json!({
            "sessionId": session_id,
            "cursor": 41
        }))
        .unwrap();
        assert_eq!(subscribe.cursor.map(OutputCursor::get), Some(41));
    }

    #[test]
    fn worktree_remove_rejects_dirty_and_force_bypasses() {
        let base = json!({
            "worktreeId": WorktreeId::new(),
            "confirmationToken": "abcdefghijklmnop"
        });
        assert!(serde_json::from_value::<WorktreeRemoveRequest>(base.clone()).is_ok());
        for field in ["allowDirty", "force"] {
            let mut value = base.clone();
            value[field] = json!(true);
            assert!(serde_json::from_value::<WorktreeRemoveRequest>(value).is_err());
        }
    }

    #[test]
    fn command_debug_redacts_argument_and_environment_values() {
        let command = AgentCommand::try_new(
            "codex",
            vec!["--token=argument-secret".to_owned()],
            BTreeMap::from([(
                "CLI_MASTER_PROFILE".to_owned(),
                "environment-secret".to_owned(),
            )]),
        )
        .unwrap();
        let debug = format!("{command:?}");
        assert!(!debug.contains("argument-secret"));
        assert!(!debug.contains("environment-secret"));
        assert!(debug.contains("CLI_MASTER_PROFILE"));
        assert_eq!(command.executable(), "codex");
        assert_eq!(command.args(), ["--token=argument-secret"]);
        assert_eq!(
            serde_json::to_value(&command).unwrap(),
            json!({
                "executable": "codex",
                "args": ["--token=argument-secret"],
                "env": { "CLI_MASTER_PROFILE": "environment-secret" }
            })
        );

        let mut with_cwd = serde_json::to_value(&command).unwrap();
        with_cwd["cwd"] = json!("/tmp/ui-selected");
        assert!(serde_json::from_value::<AgentCommand>(with_cwd).is_err());
    }

    #[test]
    fn custom_agent_commands_reject_persisted_secret_variables() {
        for key in ["OPENAI_API_KEY", "DB_PASSWORD_BACKUP"] {
            let mut env = serde_json::Map::new();
            env.insert(key.to_owned(), json!("must-not-be-persisted"));
            let value = json!({
                "executable": "codex",
                "args": [],
                "env": env
            });
            assert!(
                serde_json::from_value::<AgentCommand>(value).is_err(),
                "accepted {key}"
            );
        }
    }

    #[test]
    fn agent_detection_construction_preserves_its_bound() {
        let ids = vec![AgentId::new(), AgentId::new()];
        let request = AgentDetectRequest::try_new(ids.clone()).unwrap();
        assert_eq!(request.agent_ids(), ids);
        assert!(AgentDetectRequest::all().agent_ids().is_empty());
        assert!(AgentDetectRequest::try_new(vec![AgentId::new(); MAX_DETECTION_IDS + 1]).is_err());
    }

    #[test]
    fn mutable_requests_reject_unknown_fields() {
        assert!(
            serde_json::from_value::<ProjectRemoveRequest>(json!({
                "projectId": ProjectId::new(),
                "path": "/tmp/project"
            }))
            .is_err()
        );
        assert!(serde_json::from_value::<EmptyRequest>(json!({ "unexpected": true })).is_err());
    }

    fn valid_git_diff(project_id: ProjectId) -> serde_json::Value {
        json!({
            "target": { "kind": "project", "projectId": project_id }
        })
    }
}
