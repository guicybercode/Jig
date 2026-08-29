//! Stable dotted method names supported by protocol version 1.

/// Negotiate protocol compatibility and identify the daemon lifetime.
pub const SYSTEM_HELLO: &str = "system.hello";
/// Read the durable state required to bootstrap a client.
pub const STATE_SNAPSHOT: &str = "state.snapshot";

/// Register a user-selected local Git project.
pub const PROJECT_ADD: &str = "project.add";
/// List registered projects.
pub const PROJECT_LIST: &str = "project.list";
/// Change a registered project's user-facing name.
pub const PROJECT_RENAME: &str = "project.rename";
/// Remove only a project's application metadata.
pub const PROJECT_REMOVE: &str = "project.remove";

/// List built-in and custom agent definitions.
pub const AGENT_LIST: &str = "agent.list";
/// Detect whether configured agent executables are available.
pub const AGENT_DETECT: &str = "agent.detect";
/// Enable or disable an agent definition without deleting it.
pub const AGENT_SET_ENABLED: &str = "agent.set_enabled";
/// Create a structured custom agent definition.
pub const AGENT_CUSTOM_CREATE: &str = "agent.custom.create";
/// Replace the editable fields of a custom agent definition.
pub const AGENT_CUSTOM_UPDATE: &str = "agent.custom.update";
/// Remove an unreferenced custom agent definition.
pub const AGENT_CUSTOM_REMOVE: &str = "agent.custom.remove";

/// Create session metadata and derive its authoritative working directory.
pub const SESSION_CREATE: &str = "session.create";
/// List sessions, optionally constrained to a project.
pub const SESSION_LIST: &str = "session.list";
/// Rename a session without changing process state.
pub const SESSION_RENAME: &str = "session.rename";
/// Start an existing stopped session.
pub const SESSION_START: &str = "session.start";
/// Restart an existing session with a fresh PTY.
pub const SESSION_RESTART: &str = "session.restart";
/// Stop a running session using the daemon's bounded graceful policy.
pub const SESSION_STOP: &str = "session.stop";
/// Delete stopped-session metadata without deleting repository files.
pub const SESSION_DELETE: &str = "session.delete";
/// Write bounded base64 terminal bytes to a live session PTY.
pub const SESSION_WRITE: &str = "session.write";
/// Resize a live session PTY.
pub const SESSION_RESIZE: &str = "session.resize";
/// Subscribe to replay and live output starting after a cursor.
pub const SESSION_SUBSCRIBE: &str = "session.subscribe";
/// Stop delivering terminal events to one client.
pub const SESSION_UNSUBSCRIBE: &str = "session.unsubscribe";

/// Read structured repository status for a registered project, session, or worktree.
pub const GIT_STATUS: &str = "git.status";
/// Read a bounded textual diff for a registered project, session, or worktree.
pub const GIT_DIFF: &str = "git.diff";

/// Inspect whether a managed worktree can be safely removed.
pub const WORKTREE_PREPARE_REMOVE: &str = "worktree.prepare_remove";
/// Remove a managed worktree after token-bound state confirmation.
pub const WORKTREE_REMOVE: &str = "worktree.remove";

/// Read a sanitized local diagnostic snapshot.
pub const DIAGNOSTICS_GET: &str = "diagnostics.get";

/// Every method implemented by the Beta v1 contract.
pub const ALL: &[&str] = &[
    SYSTEM_HELLO,
    STATE_SNAPSHOT,
    PROJECT_ADD,
    PROJECT_LIST,
    PROJECT_RENAME,
    PROJECT_REMOVE,
    AGENT_LIST,
    AGENT_DETECT,
    AGENT_SET_ENABLED,
    AGENT_CUSTOM_CREATE,
    AGENT_CUSTOM_UPDATE,
    AGENT_CUSTOM_REMOVE,
    SESSION_CREATE,
    SESSION_LIST,
    SESSION_RENAME,
    SESSION_START,
    SESSION_RESTART,
    SESSION_STOP,
    SESSION_DELETE,
    SESSION_WRITE,
    SESSION_RESIZE,
    SESSION_SUBSCRIBE,
    SESSION_UNSUBSCRIBE,
    GIT_STATUS,
    GIT_DIFF,
    WORKTREE_PREPARE_REMOVE,
    WORKTREE_REMOVE,
    DIAGNOSTICS_GET,
];

/// Returns whether a dotted method belongs to the Beta v1 contract.
#[must_use]
pub fn is_supported(value: &str) -> bool {
    ALL.contains(&value)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn names_are_unique_and_use_stable_dotted_form() {
        let unique: BTreeSet<_> = ALL.iter().copied().collect();
        assert_eq!(unique.len(), ALL.len());
        assert!(ALL.iter().all(|name| {
            let (namespace, operation) = name.split_once('.').unwrap_or_default();
            !namespace.is_empty()
                && !operation.is_empty()
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || matches!(byte, b'.' | b'_'))
        }));
    }
}
