use std::time::Duration;

/// Upper bound for Git subprocesses that inspect or mutate a repository.
pub const GIT_COMMAND_TIMEOUT: Duration = Duration::from_secs(15);

/// Upper bound for `git --version` and similar version probes.
pub const VERSION_DETECT_TIMEOUT: Duration = Duration::from_secs(3);

/// Upper bound for assembling a diagnostics snapshot.
pub const DIAGNOSTICS_TIMEOUT: Duration = Duration::from_secs(5);

/// Grace period after SIGTERM before a forced stop is offered.
pub const PROCESS_STOP_GRACE: Duration = Duration::from_secs(3);

/// Upper bound for a confirmed SIGKILL.
pub const PROCESS_FORCE_KILL_TIMEOUT: Duration = Duration::from_secs(2);

/// Default in-memory PTY replay buffer.
pub const OUTPUT_BUFFER_MAX_BYTES: usize = 8 * 1024 * 1024;

/// Maximum textual diff returned to the UI.
pub const DIFF_MAX_BYTES: usize = 2 * 1024 * 1024;

/// Maximum captured stdout/stderr for a bounded Git or diagnostics command.
pub const COMMAND_OUTPUT_MAX_BYTES: usize = 64 * 1024;

/// Maximum bytes retained per rotated log file.
pub const LOG_FILE_MAX_BYTES: usize = 5 * 1024 * 1024;

/// Number of rotated log files retained on disk.
pub const LOG_FILE_RETENTION: usize = 3;

/// Number of sanitized log records kept for diagnostics export.
pub const RECENT_LOGS_MAX: usize = 200;

/// Number of sanitized recent errors kept for diagnostics export.
pub const RECENT_ERRORS_MAX: usize = 50;

/// Confirmation tokens expire if unused.
pub const CONFIRMATION_TTL: Duration = Duration::from_secs(300);

/// Login-shell PATH import must not hang on a prompt.
pub const LOGIN_SHELL_PATH_TIMEOUT: Duration = Duration::from_secs(3);
