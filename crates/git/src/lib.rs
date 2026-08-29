//! Structured Git operations that never interpolate a shell command string.
//!
//! Every invocation uses an absolute `git` executable plus an argument array.
//! Read operations disable pagers and interactive prompts.

#![warn(missing_docs)]

mod error;
mod service;
mod status;
mod worktree;

pub use error::GitError;
pub use service::GitService;
pub use status::{GitDiff, GitStatus};
pub use worktree::{RemovalPlan, WorktreeCreate, WorktreeInfo};
