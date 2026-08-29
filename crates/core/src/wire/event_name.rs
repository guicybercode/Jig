//! Stable dotted event names emitted by protocol version 1.

/// A project was created or its public metadata changed.
pub const PROJECT_UPDATED: &str = "project.updated";
/// A project metadata record was removed.
pub const PROJECT_REMOVED: &str = "project.removed";
/// An agent definition or detection result changed.
pub const AGENT_UPDATED: &str = "agent.updated";
/// A custom agent definition was removed.
pub const AGENT_REMOVED: &str = "agent.removed";
/// A session was created.
pub const SESSION_CREATED: &str = "session.created";
/// A session's public metadata changed.
pub const SESSION_UPDATED: &str = "session.updated";
/// A stopped session metadata record was removed.
pub const SESSION_DELETED: &str = "session.deleted";
/// One bounded terminal-output chunk is available.
pub const SESSION_OUTPUT: &str = "session.output";
/// Replay for a subscription cursor reached the live stream boundary.
pub const SESSION_REPLAY_COMPLETE: &str = "session.replay_complete";
/// A requested or live output range is no longer available.
pub const SESSION_OUTPUT_GAP: &str = "session.output_gap";
/// A session entered a different lifecycle state.
pub const SESSION_STATUS_CHANGED: &str = "session.status_changed";
/// A session process exited.
pub const SESSION_EXITED: &str = "session.exited";
/// A worktree was created or its public metadata changed.
pub const WORKTREE_UPDATED: &str = "worktree.updated";
/// A managed worktree metadata record was removed.
pub const WORKTREE_REMOVED: &str = "worktree.removed";
/// Explicit repository refresh observed a different Git status.
pub const GIT_STATUS_CHANGED: &str = "git.status_changed";
/// The daemon began a clean shutdown.
pub const DAEMON_SHUTTING_DOWN: &str = "daemon.shutting_down";

/// Every event emitted by the Beta v1 contract.
pub const ALL: &[&str] = &[
    PROJECT_UPDATED,
    PROJECT_REMOVED,
    AGENT_UPDATED,
    AGENT_REMOVED,
    SESSION_CREATED,
    SESSION_UPDATED,
    SESSION_DELETED,
    SESSION_OUTPUT,
    SESSION_REPLAY_COMPLETE,
    SESSION_OUTPUT_GAP,
    SESSION_STATUS_CHANGED,
    SESSION_EXITED,
    WORKTREE_UPDATED,
    WORKTREE_REMOVED,
    GIT_STATUS_CHANGED,
    DAEMON_SHUTTING_DOWN,
];

/// Returns whether a dotted event belongs to the Beta v1 contract.
#[must_use]
pub fn is_supported(value: &str) -> bool {
    ALL.contains(&value)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn event_names_are_unique() {
        let unique: BTreeSet<_> = ALL.iter().copied().collect();
        assert_eq!(unique.len(), ALL.len());
    }
}
