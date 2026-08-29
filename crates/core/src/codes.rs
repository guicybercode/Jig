//! Stable machine-readable error codes returned across IPC.

/// The requested IPC method is not implemented by this daemon.
pub const METHOD_NOT_FOUND: &str = "METHOD_NOT_FOUND";
/// The protocol version is not supported.
pub const PROTOCOL_UNSUPPORTED: &str = "PROTOCOL_UNSUPPORTED";
/// The request payload could not be parsed.
pub const INVALID_REQUEST: &str = "INVALID_REQUEST";
/// The daemon is not ready to accept requests.
pub const DAEMON_UNAVAILABLE: &str = "DAEMON_UNAVAILABLE";
/// SQLite could not be opened or queried.
pub const DATABASE_UNAVAILABLE: &str = "DATABASE_UNAVAILABLE";

/// Git is missing from PATH or is not executable.
pub const GIT_NOT_FOUND: &str = "GIT_NOT_FOUND";
/// A Git command exceeded its time or output limit.
pub const GIT_COMMAND_FAILED: &str = "GIT_COMMAND_FAILED";
/// The selected path is not a Git repository.
pub const GIT_NOT_A_REPOSITORY: &str = "GIT_NOT_A_REPOSITORY";
/// The selected path cannot be read.
pub const PATH_UNREADABLE: &str = "PATH_UNREADABLE";
/// The project directory no longer exists at the stored path.
pub const PROJECT_MOVED: &str = "PROJECT_MOVED";
/// The project is still referenced by sessions or worktrees.
pub const PROJECT_IN_USE: &str = "PROJECT_IN_USE";
/// A project with the same canonical path already exists.
pub const PROJECT_DUPLICATE: &str = "PROJECT_DUPLICATE";

/// The agent executable was not found.
pub const AGENT_EXECUTABLE_NOT_FOUND: &str = "AGENT_EXECUTABLE_NOT_FOUND";
/// The custom executable exists but is not executable.
pub const AGENT_NOT_EXECUTABLE: &str = "AGENT_NOT_EXECUTABLE";
/// The custom agent definition is invalid.
pub const AGENT_INVALID: &str = "AGENT_INVALID";
/// The agent key is already registered.
pub const AGENT_DUPLICATE: &str = "AGENT_DUPLICATE";
/// The agent is still referenced by a session.
pub const AGENT_IN_USE: &str = "AGENT_IN_USE";
/// A built-in agent cannot be mutated or removed.
pub const AGENT_BUILTIN_READONLY: &str = "AGENT_BUILTIN_READONLY";

/// The session does not exist.
pub const SESSION_NOT_FOUND: &str = "SESSION_NOT_FOUND";
/// The session is already starting or running.
pub const SESSION_ALREADY_RUNNING: &str = "SESSION_ALREADY_RUNNING";
/// The session is already stopping or stopped.
pub const SESSION_NOT_RUNNING: &str = "SESSION_NOT_RUNNING";
/// Metadata cannot be deleted while the process is live.
pub const SESSION_STILL_RUNNING: &str = "SESSION_STILL_RUNNING";

/// A worktree or branch name already exists.
pub const WORKTREE_EXISTS: &str = "WORKTREE_EXISTS";
/// Removal is blocked because the worktree has uncommitted changes.
pub const WORKTREE_DIRTY: &str = "WORKTREE_DIRTY";
/// Removal is blocked because a session still uses the worktree.
pub const WORKTREE_IN_USE: &str = "WORKTREE_IN_USE";
/// The confirmation token is missing, expired, or does not match current state.
pub const WORKTREE_CONFIRMATION_REQUIRED: &str = "WORKTREE_CONFIRMATION_REQUIRED";
/// The worktree path escaped the managed root.
pub const WORKTREE_PATH_INVALID: &str = "WORKTREE_PATH_INVALID";

/// PTY output exceeded the in-memory replay budget and older bytes were dropped.
pub const OUTPUT_TRUNCATED: &str = "OUTPUT_TRUNCATED";
