//! Local safety primitives for process execution, paths, logs, and diagnostics.
//!
//! This crate does not sandbox user-installed CLIs. It prevents accidental
//! data loss, secret leakage, and ambiguous destructive operations.

#![warn(missing_docs)]

mod destructive;
mod diagnostics;
mod identity;
mod ipc;
mod log;
mod paths;
mod platform;
mod process;
mod shell;

pub use destructive::{
    ConfirmationStore, DestructiveKind, DestructiveRequest, RemovalPlan, WorktreeRemovalState,
};
pub use diagnostics::collect_diagnostics;
pub use identity::{ProcessIdentity, ProcessStopPlan, record_identity, stop_process};
pub use ipc::validate_method_payload;
pub use log::{LogLevel, Logger, StructuredLog};
pub use paths::{
    ManagedRoots, ResolvedPath, assert_managed_worktree, assert_not_critical,
    canonicalize_existing, is_within, normalize_lexical, resolve_path,
};
pub use platform::PlatformPaths;
pub use process::{
    ProcessOutput, SpawnRequest, assert_structured_command, run_command, run_command_spec,
    run_command_unchecked,
};
pub use shell::{LOGIN_SHELL_PATH_COMMAND, import_login_path};
