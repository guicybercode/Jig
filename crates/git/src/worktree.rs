use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use crate::{Git, GitError, GitErrorKind, os, repository};

/// Caller-observed runtime usage that Git cannot determine itself.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorktreeUse {
    /// An agent process is currently running with this worktree as its cwd.
    pub running: bool,
    /// Session metadata or another live operation currently claims the worktree.
    pub in_use: bool,
}

/// One entry from Git's worktree registry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorktreeInfo {
    /// Canonical or absolute worktree path reported by Git.
    pub path: PathBuf,
    /// Commit checked out in the worktree.
    pub head: Option<String>,
    /// Local branch without the `refs/heads/` prefix.
    pub branch: Option<String>,
    /// Whether the worktree has a detached `HEAD`.
    pub detached: bool,
    /// Whether Git has marked the worktree as locked.
    pub locked: bool,
    /// Whether Git considers the worktree entry prunable.
    pub prunable: bool,
}

pub(crate) fn list(git: &Git, repository: &Path) -> Result<Vec<WorktreeInfo>, GitError> {
    let root = repository::require_root(git, repository)?;
    let output = git.checked(
        Some(&root),
        [os("worktree"), os("list"), os("--porcelain"), os("-z")],
        "list worktrees",
    )?;
    parse_list(&output.stdout)
}

fn parse_list(bytes: &[u8]) -> Result<Vec<WorktreeInfo>, GitError> {
    let mut result = Vec::new();
    let mut current: Option<WorktreeInfo> = None;
    for record in bytes.split(|byte| *byte == 0) {
        if record.is_empty() {
            if let Some(worktree) = current.take() {
                result.push(worktree);
            }
            continue;
        }
        if let Some(path) = record.strip_prefix(b"worktree ") {
            if let Some(worktree) = current.take() {
                result.push(worktree);
            }
            current = Some(WorktreeInfo {
                path: bytes_to_path(path),
                head: None,
                branch: None,
                detached: false,
                locked: false,
                prunable: false,
            });
            continue;
        }
        let worktree = current
            .as_mut()
            .ok_or_else(|| invalid_worktree_output("metadata appeared before a worktree path"))?;
        if let Some(head) = record.strip_prefix(b"HEAD ") {
            worktree.head = Some(parse_utf8(head, "worktree HEAD")?.to_owned());
        } else if let Some(branch) = record.strip_prefix(b"branch refs/heads/") {
            worktree.branch = Some(parse_utf8(branch, "worktree branch")?.to_owned());
        } else if record == b"detached" {
            worktree.detached = true;
        } else if record == b"locked" || record.starts_with(b"locked ") {
            worktree.locked = true;
        } else if record == b"prunable" || record.starts_with(b"prunable ") {
            worktree.prunable = true;
        }
    }
    if let Some(worktree) = current {
        result.push(worktree);
    }
    if result.is_empty() {
        return Err(invalid_worktree_output("Git returned no worktrees"));
    }
    Ok(result)
}

pub(crate) fn invalid_worktree_output(detail: &str) -> GitError {
    GitError::new(
        GitErrorKind::InvalidOutput,
        format!("Could not parse Git worktree data: {detail}"),
        "Run `git worktree list` to inspect the repository and update Git if necessary",
    )
}

fn parse_utf8<'a>(bytes: &'a [u8], field: &'static str) -> Result<&'a str, GitError> {
    std::str::from_utf8(bytes)
        .map_err(|_| invalid_worktree_output(&format!("Git returned a non-UTF-8 {field}")))
}

fn bytes_to_path(bytes: &[u8]) -> PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        PathBuf::from(OsString::from_vec(bytes.to_vec()))
    }
    #[cfg(not(unix))]
    {
        PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_worktree_list() {
        let bytes = b"worktree /tmp/main\0HEAD abc123\0branch refs/heads/main\0\0\
            worktree /tmp/agent\0HEAD def456\0branch refs/heads/agent/task-123\0locked reason\0\0";
        let worktrees = parse_list(bytes).expect("fixture should parse");
        assert_eq!(worktrees.len(), 2);
        assert_eq!(worktrees[1].branch.as_deref(), Some("agent/task-123"));
        assert!(worktrees[1].locked);
    }
}
