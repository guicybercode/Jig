use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{command::GitCommand, error::GitError, paths::normalize_absolute};

/// How a path relates to a Git repository.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryKind {
    /// The path is the worktree root.
    Root,
    /// The path is a subdirectory of a worktree.
    Subdirectory,
    /// The path is a linked Git worktree.
    Worktree,
}

/// Current `HEAD` naming state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum BranchState {
    /// `HEAD` points at a named branch.
    Branch {
        /// Branch name without `refs/heads/`.
        name: String,
    },
    /// `HEAD` is detached at a commit.
    Detached {
        /// Commit object name.
        head: String,
    },
    /// A branch is checked out but has no commits.
    Unborn {
        /// Name of the unborn branch.
        name: String,
    },
}

/// Detected repository metadata for a filesystem path.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryInfo {
    /// Path that was requested.
    pub path: PathBuf,
    /// Worktree root (`rev-parse --show-toplevel`).
    pub root: PathBuf,
    /// Absolute Git directory for this worktree.
    pub git_dir: PathBuf,
    /// Common Git directory shared by linked worktrees.
    pub git_common_dir: PathBuf,
    /// Relationship of `path` to `root`.
    pub kind: RepositoryKind,
    /// Current branch or detached/unborn state.
    pub branch: BranchState,
    /// Whether the repository has no commits.
    pub unborn: bool,
    /// Whether Git reports this as a bare repository.
    pub bare: bool,
}

pub(crate) fn detect_repository(
    executable: &Path,
    path: &Path,
) -> Result<RepositoryInfo, GitError> {
    crate::command::inspect_path(path)?;
    let path = normalize_absolute(path)?;

    let inside = git(executable, &path)
        .arg("rev-parse")
        .arg("--is-inside-work-tree")
        .run()?;
    if !inside.success() || inside.stdout_trimmed()? != "true" {
        let bare = git(executable, &path)
            .arg("rev-parse")
            .arg("--is-bare-repository")
            .run()?;
        if bare.success() && bare.stdout_trimmed().unwrap_or("") == "true" {
            return Err(GitError::BareRepository { path });
        }
        return Err(GitError::NotARepository { path });
    }

    let bare_output = git(executable, &path)
        .arg("rev-parse")
        .arg("--is-bare-repository")
        .run_checked()?;
    if bare_output.stdout_trimmed()? == "true" {
        return Err(GitError::BareRepository { path });
    }

    let root = read_git(executable, &path, &["rev-parse", "--show-toplevel"])?;
    let git_dir = read_git(executable, &path, &["rev-parse", "--absolute-git-dir"])?;
    let git_common_dir = read_git(executable, &path, &["rev-parse", "--git-common-dir"])?;

    let root = normalize_absolute(Path::new(&root))?;
    let git_dir = normalize_absolute(Path::new(&git_dir))?;
    let git_common_dir = if Path::new(&git_common_dir).is_absolute() {
        normalize_absolute(Path::new(&git_common_dir))?
    } else {
        normalize_absolute(&path.join(git_common_dir))?
    };

    let kind = if git_dir != git_common_dir {
        RepositoryKind::Worktree
    } else if path == root {
        RepositoryKind::Root
    } else {
        RepositoryKind::Subdirectory
    };

    let branch = current_branch(executable, &root)?;
    let unborn = matches!(branch, BranchState::Unborn { .. });

    Ok(RepositoryInfo {
        path,
        root,
        git_dir,
        git_common_dir,
        kind,
        branch,
        unborn,
        bare: false,
    })
}

pub(crate) fn current_branch(executable: &Path, path: &Path) -> Result<BranchState, GitError> {
    crate::command::inspect_path(path)?;
    let head_commit = git(executable, path)
        .arg("rev-parse")
        .arg("--verify")
        .arg("HEAD")
        .run()?;
    let symbolic = git(executable, path)
        .arg("symbolic-ref")
        .arg("--short")
        .arg("HEAD")
        .run()?;

    if symbolic.success() {
        let name = symbolic.stdout_trimmed()?.to_owned();
        if head_commit.success() {
            Ok(BranchState::Branch { name })
        } else {
            Ok(BranchState::Unborn { name })
        }
    } else if head_commit.success() {
        Ok(BranchState::Detached {
            head: head_commit.stdout_trimmed()?.to_owned(),
        })
    } else {
        Err(GitError::InvalidOutput {
            reason: "HEAD is neither a symbolic ref nor a commit".to_owned(),
        })
    }
}

pub(crate) fn resolve_commit(
    executable: &Path,
    repository: &Path,
    reference: &str,
) -> Result<String, GitError> {
    if reference.is_empty() || reference.starts_with('-') || reference.contains('\0') {
        return Err(GitError::InvalidRef {
            reference: reference.to_owned(),
        });
    }

    let spec = format!("{reference}^{{commit}}");
    let output = git(executable, repository)
        .arg("rev-parse")
        .arg("--verify")
        .arg("--end-of-options")
        .arg(&spec)
        .run()?;
    if !output.success() {
        return Err(GitError::InvalidRef {
            reference: reference.to_owned(),
        });
    }
    let commit = output.stdout_trimmed()?.to_owned();
    if !is_object_id(&commit) {
        return Err(GitError::InvalidOutput {
            reason: format!("rev-parse returned a non-object id for {reference}"),
        });
    }
    Ok(commit)
}

pub(crate) fn list_local_branches(
    executable: &Path,
    repository: &Path,
) -> Result<Vec<String>, GitError> {
    let output = git(executable, repository)
        .arg("for-each-ref")
        .arg("--format=%(refname:short)")
        .arg("refs/heads")
        .run_checked()?;
    if output.stdout.is_empty() {
        return Ok(Vec::new());
    }
    Ok(output
        .stdout_str()?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

fn is_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn git<'a>(executable: &'a Path, path: &Path) -> GitCommand<'a> {
    GitCommand::new(executable).read_only().repo(path)
}

fn read_git(executable: &Path, path: &Path, args: &[&str]) -> Result<String, GitError> {
    let output = git(executable, path)
        .args(args.iter().copied())
        .run_checked()?;
    Ok(output.stdout_trimmed()?.to_owned())
}
