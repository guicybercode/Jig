use std::path::{Path, PathBuf};

use crate::{Git, GitError, GitErrorKind, os};

/// Filesystem and Git metadata discovered for a user-selected directory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryInspection {
    /// Canonical form of the selected directory.
    pub path: PathBuf,
    /// Canonical repository root, or `None` when the directory is not in a repo.
    pub repository_root: Option<PathBuf>,
    /// Symbolic branch name, or `None` for a non-repository or detached `HEAD`.
    pub branch: Option<String>,
}

impl RepositoryInspection {
    /// Returns whether the selected directory belongs to a Git repository.
    #[must_use]
    pub const fn is_repository(&self) -> bool {
        self.repository_root.is_some()
    }
}

pub(crate) fn inspect(git: &Git, path: &Path) -> Result<RepositoryInspection, GitError> {
    if !path.exists() {
        return Err(GitError::new(
            GitErrorKind::NotFound,
            format!("Directory does not exist: {}", path.display()),
            "Choose an existing project directory",
        )
        .with_path(path));
    }
    if !path.is_dir() {
        return Err(GitError::new(
            GitErrorKind::InvalidInput,
            format!("Project path is not a directory: {}", path.display()),
            "Choose a directory instead of a file",
        )
        .with_path(path));
    }
    let canonical = path
        .canonicalize()
        .map_err(|error| GitError::io("resolve project directory", error).with_path(path))?;
    let output = git.execute(
        Some(&canonical),
        [
            os("rev-parse"),
            os("--path-format=absolute"),
            os("--show-toplevel"),
        ],
        64 * 1024,
    )?;
    if !output.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not a git repository") {
            return Ok(RepositoryInspection {
                path: canonical,
                repository_root: None,
                branch: None,
            });
        }
        return Err(git.command_error("inspect the repository", &output));
    }
    let root = parse_single_path(&output.stdout)?;
    let root = root
        .canonicalize()
        .map_err(|error| GitError::io("resolve repository root", error).with_path(&root))?;
    let branch_output = git.execute(
        Some(&canonical),
        [os("symbolic-ref"), os("--quiet"), os("--short"), os("HEAD")],
        64 * 1024,
    )?;
    let branch = if branch_output.success() {
        Some(parse_single_line(&branch_output.stdout, "branch name")?)
    } else if branch_output.status.code() == Some(1) {
        None
    } else {
        return Err(git.command_error("identify the current branch", &branch_output));
    };
    Ok(RepositoryInspection {
        path: canonical,
        repository_root: Some(root),
        branch,
    })
}

pub(crate) fn require_root(git: &Git, path: &Path) -> Result<PathBuf, GitError> {
    git.inspect_repository(path)?
        .repository_root
        .ok_or_else(|| {
            GitError::new(
                GitErrorKind::NotRepository,
                format!("Directory is not in a Git repository: {}", path.display()),
                "Choose a Git repository or initialize one before continuing",
            )
            .with_path(path)
        })
}

fn parse_single_path(bytes: &[u8]) -> Result<PathBuf, GitError> {
    let value = parse_single_line(bytes, "repository root")?;
    Ok(PathBuf::from(value))
}

fn parse_single_line(bytes: &[u8], label: &'static str) -> Result<String, GitError> {
    let value = std::str::from_utf8(bytes).map_err(|_| {
        GitError::new(
            GitErrorKind::InvalidOutput,
            format!("Git returned a non-UTF-8 {label}"),
            "Move the repository to a UTF-8-compatible path and try again",
        )
    })?;
    let value = value.trim_end_matches(['\r', '\n']);
    if value.is_empty() || value.contains('\n') || value.contains('\r') {
        return Err(GitError::new(
            GitErrorKind::InvalidOutput,
            format!("Git returned an invalid {label}"),
            "Verify the repository with the Git command line",
        ));
    }
    Ok(value.to_owned())
}
