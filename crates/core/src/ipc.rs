use serde::{Deserialize, Serialize};

macro_rules! ipc_names {
    ($enum:ident, $unknown:ident, { $($variant:ident => $wire:literal),+ $(,)? }) => {
        impl $enum {
            /// Every named v1 catalog entry, excluding the unknown variant.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// Returns the dotted wire name.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $wire,)+
                    Self::$unknown => "unknown",
                }
            }

            /// Parses a dotted wire name.
            #[must_use]
            pub fn parse(value: &str) -> Self {
                match value {
                    $($wire => Self::$variant,)+
                    _ => Self::$unknown,
                }
            }
        }
    };
}

/// Catalog of protocol v1 request methods.
///
/// Unknown names decode as [`IpcMethod::Unknown`]. The daemon must reject
/// those with `PROTOCOL_UNKNOWN_METHOD` rather than inventing behavior.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum IpcMethod {
    /// Negotiate protocol version and report daemon identity.
    #[serde(rename = "system.hello")]
    SystemHello,
    /// Return the current metadata snapshot.
    #[serde(rename = "state.snapshot")]
    StateSnapshot,
    /// List registered projects.
    #[serde(rename = "project.list")]
    ProjectList,
    /// Register a Git repository as a project.
    #[serde(rename = "project.add")]
    ProjectAdd,
    /// Remove project metadata. Never deletes the repository directory.
    #[serde(rename = "project.remove")]
    ProjectRemove,
    /// Rename a project's display name.
    #[serde(rename = "project.rename")]
    ProjectRename,
    /// List built-in and custom agent definitions.
    #[serde(rename = "agent.list")]
    AgentList,
    /// Detect which catalog agents are currently executable.
    #[serde(rename = "agent.detect")]
    AgentDetect,
    /// Create a structured custom agent.
    #[serde(rename = "agent.create_custom")]
    AgentCreateCustom,
    /// Update a structured custom agent.
    #[serde(rename = "agent.update_custom")]
    AgentUpdateCustom,
    /// Delete a custom agent that is not referenced by sessions.
    #[serde(rename = "agent.delete_custom")]
    AgentDeleteCustom,
    /// List sessions, optionally filtered by project.
    #[serde(rename = "session.list")]
    SessionList,
    /// Persist a session in `created` without spawning a process.
    #[serde(rename = "session.create")]
    SessionCreate,
    /// Spawn the session process and attach a PTY.
    #[serde(rename = "session.start")]
    SessionStart,
    /// Write bytes to the session PTY.
    #[serde(rename = "session.write")]
    SessionWrite,
    /// Resize the session PTY.
    #[serde(rename = "session.resize")]
    SessionResize,
    /// Request a graceful stop of the session process group.
    #[serde(rename = "session.stop")]
    SessionStop,
    /// Stop if live, then spawn again using the same metadata.
    #[serde(rename = "session.restart")]
    SessionRestart,
    /// Delete session metadata after the process has stopped.
    #[serde(rename = "session.delete")]
    SessionDelete,
    /// List managed worktrees for a project.
    #[serde(rename = "worktree.list")]
    WorktreeList,
    /// Create a managed worktree and branch.
    #[serde(rename = "worktree.create")]
    WorktreeCreate,
    /// Remove a managed worktree after the two-step safety check.
    #[serde(rename = "worktree.remove")]
    WorktreeRemove,
    /// Observe Git status for a project or worktree.
    #[serde(rename = "git.status")]
    GitStatus,
    /// Return a bounded textual diff.
    #[serde(rename = "git.diff")]
    GitDiff,
    /// Return sanitized diagnostic information.
    #[serde(rename = "diagnostics.get")]
    DiagnosticsGet,
    /// A method name this binary does not recognize.
    #[serde(other, rename = "unknown")]
    Unknown,
}

ipc_names!(IpcMethod, Unknown, {
    SystemHello => "system.hello",
    StateSnapshot => "state.snapshot",
    ProjectList => "project.list",
    ProjectAdd => "project.add",
    ProjectRemove => "project.remove",
    ProjectRename => "project.rename",
    AgentList => "agent.list",
    AgentDetect => "agent.detect",
    AgentCreateCustom => "agent.create_custom",
    AgentUpdateCustom => "agent.update_custom",
    AgentDeleteCustom => "agent.delete_custom",
    SessionList => "session.list",
    SessionCreate => "session.create",
    SessionStart => "session.start",
    SessionWrite => "session.write",
    SessionResize => "session.resize",
    SessionStop => "session.stop",
    SessionRestart => "session.restart",
    SessionDelete => "session.delete",
    WorktreeList => "worktree.list",
    WorktreeCreate => "worktree.create",
    WorktreeRemove => "worktree.remove",
    GitStatus => "git.status",
    GitDiff => "git.diff",
    DiagnosticsGet => "diagnostics.get",
});

/// Catalog of protocol v1 daemon events.
///
/// Unknown names decode as [`IpcEvent::Unknown`]. Clients must ignore them.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum IpcEvent {
    /// Batched PTY output for one session.
    #[serde(rename = "session.output")]
    SessionOutput,
    /// Session status changed.
    #[serde(rename = "session.status_changed")]
    SessionStatusChanged,
    /// Session process exited.
    #[serde(rename = "session.exited")]
    SessionExited,
    /// Project metadata was added, renamed, or removed.
    #[serde(rename = "project.changed")]
    ProjectChanged,
    /// Git status was refreshed for a project or worktree.
    #[serde(rename = "git.status_changed")]
    GitStatusChanged,
    /// Daemon lifecycle changed.
    #[serde(rename = "daemon.status_changed")]
    DaemonStatusChanged,
    /// An event name this binary does not recognize.
    #[serde(other, rename = "unknown")]
    Unknown,
}

ipc_names!(IpcEvent, Unknown, {
    SessionOutput => "session.output",
    SessionStatusChanged => "session.status_changed",
    SessionExited => "session.exited",
    ProjectChanged => "project.changed",
    GitStatusChanged => "git.status_changed",
    DaemonStatusChanged => "daemon.status_changed",
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_wire_names_round_trip() {
        for method in IpcMethod::ALL {
            let encoded = serde_json::to_string(method).expect("method should serialize");
            assert_eq!(encoded, format!("\"{}\"", method.as_str()));
            let decoded: IpcMethod =
                serde_json::from_str(&encoded).expect("method should deserialize");
            assert_eq!(decoded, *method);
            assert_eq!(IpcMethod::parse(method.as_str()), *method);
        }
        assert_eq!(IpcMethod::parse("session.explode"), IpcMethod::Unknown);
    }

    #[test]
    fn event_wire_names_round_trip() {
        for event in IpcEvent::ALL {
            let encoded = serde_json::to_string(event).expect("event should serialize");
            assert_eq!(encoded, format!("\"{}\"", event.as_str()));
            let decoded: IpcEvent =
                serde_json::from_str(&encoded).expect("event should deserialize");
            assert_eq!(decoded, *event);
        }
        assert_eq!(IpcEvent::parse("session.output_gap"), IpcEvent::Unknown);
    }

    #[test]
    fn method_catalog_is_unique_and_complete() {
        let mut names: Vec<_> = IpcMethod::ALL
            .iter()
            .map(|method| method.as_str())
            .collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), IpcMethod::ALL.len());
        assert_eq!(IpcMethod::ALL.len(), 25);
    }

    #[test]
    fn rust_catalog_matches_shared_protocol_file() {
        let catalog: serde_json::Value =
            serde_json::from_str(include_str!("../../../protocol/catalog.json"))
                .expect("catalog fixture should parse");
        assert_eq!(catalog["protocolVersion"], 1);

        let mut methods: Vec<_> = IpcMethod::ALL
            .iter()
            .map(|method| method.as_str())
            .collect();
        methods.sort_unstable();
        assert_eq!(catalog["methods"], serde_json::json!(methods));

        let mut events: Vec<_> = IpcEvent::ALL.iter().map(|event| event.as_str()).collect();
        events.sort_unstable();
        assert_eq!(catalog["events"], serde_json::json!(events));
    }
}
