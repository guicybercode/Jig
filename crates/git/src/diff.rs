use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{
    command::{GitCommand, default_diff_limit},
    error::GitError,
};

/// Whether the diff reads the index or the worktree.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffScope {
    /// `git diff` against the worktree (unstaged).
    Unstaged,
    /// `git diff --cached` against the index (staged).
    Staged,
}

/// Options for a bounded textual diff.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiffOptions {
    /// Staged or unstaged diff.
    pub scope: DiffScope,
    /// Maximum number of patch bytes to return.
    pub byte_limit: usize,
}

impl Default for DiffOptions {
    fn default() -> Self {
        Self {
            scope: DiffScope::Unstaged,
            byte_limit: default_diff_limit(),
        }
    }
}

impl DiffOptions {
    /// Unstaged diff with the default 2 MiB cap.
    #[must_use]
    pub fn unstaged() -> Self {
        Self::default()
    }

    /// Staged diff with the default 2 MiB cap.
    #[must_use]
    pub fn staged() -> Self {
        Self {
            scope: DiffScope::Staged,
            byte_limit: default_diff_limit(),
        }
    }
}

/// Bounded textual diff plus binary-file protection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitDiff {
    /// Staged or unstaged scope that was requested.
    pub scope: DiffScope,
    /// Textual patch, possibly empty.
    pub patch: String,
    /// Whether `patch` was cut off at the byte limit.
    pub truncated: bool,
    /// Paths Git reported as binary.
    pub binary_paths: Vec<String>,
    /// Whether Git produced non-UTF-8 output that was discarded.
    pub invalid_output: bool,
}

pub(crate) fn diff(
    executable: &Path,
    path: &Path,
    options: &DiffOptions,
) -> Result<GitDiff, GitError> {
    crate::command::inspect_path(path)?;
    let binary_paths = binary_paths(executable, path, options.scope)?;
    let mut command = GitCommand::new(executable)
        .read_only()
        .repo(path)
        .arg("diff")
        .arg("--no-color")
        .arg("--no-ext-diff");
    if matches!(options.scope, DiffScope::Staged) {
        command = command.arg("--cached");
    }
    let output = command.run_capped(Some(options.byte_limit))?;
    if !output.success() && !output.truncated {
        return Err(GitError::CommandFailed {
            args: vec!["diff".to_owned()],
            exit_code: output.exit_code,
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }

    let mut invalid_output = false;
    let patch_text = if let Ok(text) = std::str::from_utf8(&output.stdout) {
        filter_binary_markers(text)
    } else {
        invalid_output = true;
        String::new()
    };

    Ok(GitDiff {
        scope: options.scope,
        patch: patch_text,
        truncated: output.truncated,
        binary_paths,
        invalid_output,
    })
}

fn binary_paths(executable: &Path, path: &Path, scope: DiffScope) -> Result<Vec<String>, GitError> {
    let mut command = GitCommand::new(executable)
        .read_only()
        .repo(path)
        .arg("diff")
        .arg("--numstat")
        .arg("--no-ext-diff");
    if matches!(scope, DiffScope::Staged) {
        command = command.arg("--cached");
    }
    let output = command.run_checked()?;
    if output.stdout.is_empty() {
        return Ok(Vec::new());
    }
    let Ok(text) = output.stdout_str() else {
        return Ok(Vec::new());
    };
    Ok(text
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let added = parts.next()?;
            let deleted = parts.next()?;
            let path = parts.next()?;
            (added == "-" && deleted == "-").then(|| path.to_owned())
        })
        .collect())
}

fn filter_binary_markers(patch_text: &str) -> String {
    patch_text
        .lines()
        .filter(|line| !line.starts_with("Binary files ") && !line.starts_with("GIT binary patch"))
        .collect::<Vec<_>>()
        .join("\n")
}
