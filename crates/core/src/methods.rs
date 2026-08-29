//! Stable IPC method names for protocol version 1.

/// Negotiates protocol support and reports daemon identity.
pub const SYSTEM_HELLO: &str = "system.hello";
/// Returns the current metadata snapshot.
pub const STATE_SNAPSHOT: &str = "state.snapshot";

/// Validates and registers a local Git repository.
pub const PROJECT_ADD: &str = "project.add";
/// Lists registered projects.
pub const PROJECT_LIST: &str = "project.list";
/// Renames a registered project without touching disk.
pub const PROJECT_RENAME: &str = "project.rename";
/// Removes project metadata from the application only.
pub const PROJECT_REMOVE: &str = "project.remove";

/// Lists built-in and custom agent definitions.
pub const AGENT_LIST: &str = "agent.list";
/// Detects which registered agents are installed.
pub const AGENT_DETECT: &str = "agent.detect";
/// Creates a structured custom agent.
pub const AGENT_CUSTOM_CREATE: &str = "agent.custom.create";
/// Updates a structured custom agent.
pub const AGENT_CUSTOM_UPDATE: &str = "agent.custom.update";
/// Removes a custom agent that is not referenced by a session.
pub const AGENT_CUSTOM_REMOVE: &str = "agent.custom.remove";

/// Creates a session in the project tree or a new worktree.
pub const SESSION_CREATE: &str = "session.create";
/// Lists sessions, optionally filtered by project.
pub const SESSION_LIST: &str = "session.list";
/// Returns one session.
pub const SESSION_GET: &str = "session.get";
/// Subscribes to replay and live output for a session.
pub const SESSION_SUBSCRIBE: &str = "session.subscribe";
/// Stops output delivery without stopping the process.
pub const SESSION_UNSUBSCRIBE: &str = "session.unsubscribe";
/// Writes bytes to a session PTY.
pub const SESSION_WRITE: &str = "session.write";
/// Resizes a session PTY.
pub const SESSION_RESIZE: &str = "session.resize";
/// Requests a graceful stop.
pub const SESSION_STOP: &str = "session.stop";
/// Force-kills a session process group.
pub const SESSION_KILL: &str = "session.kill";
/// Restarts a stopped or unknown session.
pub const SESSION_RESTART: &str = "session.restart";
/// Renames a session.
pub const SESSION_RENAME: &str = "session.rename";
/// Deletes session metadata after the process has stopped.
pub const SESSION_DELETE: &str = "session.delete";

/// Returns Git status for a project or worktree path.
pub const GIT_STATUS: &str = "git.status";
/// Returns a size-capped textual diff.
pub const GIT_DIFF: &str = "git.diff";

/// Creates a managed worktree.
pub const WORKTREE_CREATE: &str = "worktree.create";
/// Lists managed worktrees for a project.
pub const WORKTREE_LIST: &str = "worktree.list";
/// Inspects whether a worktree can be removed.
pub const WORKTREE_PREPARE_REMOVE: &str = "worktree.prepare_remove";
/// Removes a worktree after confirmation.
pub const WORKTREE_REMOVE: &str = "worktree.remove";

/// Returns sanitized diagnostic information.
pub const DIAGNOSTICS_GET: &str = "diagnostics.get";
/// Exports a sanitized diagnostic bundle.
pub const DIAGNOSTICS_EXPORT: &str = "diagnostics.export";

/// Event emitted when a session is created.
pub const EVENT_SESSION_CREATED: &str = "session.created";
/// Event emitted when session metadata changes.
pub const EVENT_SESSION_UPDATED: &str = "session.updated";
/// Event emitted when session metadata is deleted.
pub const EVENT_SESSION_DELETED: &str = "session.deleted";
/// Event emitted for PTY output chunks.
pub const EVENT_SESSION_OUTPUT: &str = "session.output";
/// Event emitted when a subscriber missed output.
pub const EVENT_SESSION_OUTPUT_GAP: &str = "session.output_gap";
/// Event emitted when session status changes.
pub const EVENT_SESSION_STATUS_CHANGED: &str = "session.status_changed";
/// Event emitted when a session process exits.
pub const EVENT_SESSION_EXITED: &str = "session.exited";
/// Event emitted when project metadata changes.
pub const EVENT_PROJECT_UPDATED: &str = "project.updated";
/// Event emitted when worktree metadata changes.
pub const EVENT_WORKTREE_UPDATED: &str = "worktree.updated";
/// Event emitted after an explicit Git refresh.
pub const EVENT_GIT_STATUS_CHANGED: &str = "git.status_changed";
/// Event emitted before the daemon exits.
pub const EVENT_DAEMON_SHUTTING_DOWN: &str = "daemon.shutting_down";
