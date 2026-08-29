use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

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

pub(crate) fn require_common_dir(git: &Git, root: &Path) -> Result<PathBuf, GitError> {
    let output = git.checked(
        Some(root),
        [
            os("rev-parse"),
            os("--path-format=absolute"),
            os("--git-common-dir"),
        ],
        "resolve the Git common directory",
    )?;
    let common_dir = parse_single_path(&output.stdout)?;
    common_dir.canonicalize().map_err(|error| {
        GitError::io("resolve the Git common directory", error).with_path(common_dir)
    })
}

pub(crate) fn resolve_head_commit(git: &Git, root: &Path) -> Result<String, GitError> {
    let output = git.checked(
        Some(root),
        [os("rev-parse"), os("--verify"), os("HEAD^{commit}")],
        "resolve the initial worktree commit",
    )?;
    parse_object_id(&output.stdout)
}

pub(crate) fn verify_commit(git: &Git, root: &Path, object_id: &str) -> Result<(), GitError> {
    let commit = format!("{object_id}^{{commit}}");
    let result = git.checked(
        Some(root),
        [os("cat-file"), os("-e"), os(commit)],
        "verify the planned worktree commit",
    );
    match result {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == GitErrorKind::CommandFailed => Err(GitError::new(
            GitErrorKind::InvalidInput,
            format!("Planned initial commit is no longer available: {object_id}"),
            "Discard the stale plan and generate a new worktree plan",
        )
        .with_path(root)
        .with_exit_status(error.exit_status())),
        Err(error) => Err(error),
    }
}

fn parse_single_path(bytes: &[u8]) -> Result<PathBuf, GitError> {
    let value = trim_single_record(bytes, "repository path")?;
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Ok(PathBuf::from(OsString::from_vec(value.to_vec())))
    }
    #[cfg(not(unix))]
    {
        Ok(PathBuf::from(String::from_utf8_lossy(value).into_owned()))
    }
}

fn parse_single_line(bytes: &[u8], label: &'static str) -> Result<String, GitError> {
    let value = trim_single_record(bytes, label)?;
    let value = std::str::from_utf8(value).map_err(|_| {
        GitError::new(
            GitErrorKind::InvalidOutput,
            format!("Git returned a non-UTF-8 {label}"),
            "Verify the repository with the Git command line",
        )
    })?;
    Ok(value.to_owned())
}

fn parse_object_id(bytes: &[u8]) -> Result<String, GitError> {
    let value = parse_single_line(bytes, "commit object ID")?;
    if !matches!(value.len(), 40 | 64) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(GitError::new(
            GitErrorKind::InvalidOutput,
            "Git returned an invalid commit object ID",
            "Verify the repository with the Git command line",
        ));
    }
    Ok(value)
}

fn trim_single_record<'a>(bytes: &'a [u8], label: &'static str) -> Result<&'a [u8], GitError> {
    let value = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    let value = value.strip_suffix(b"\r").unwrap_or(value);
    if value.is_empty() || value.contains(&b'\n') || value.contains(&b'\r') {
        return Err(GitError::new(
            GitErrorKind::InvalidOutput,
            format!("Git returned an invalid {label}"),
            "Verify the repository with the Git command line",
        ));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn repository_paths_preserve_non_utf8_bytes() {
        use std::os::unix::ffi::OsStrExt;

        let parsed = parse_single_path(b"/tmp/repository-\xff\n").expect("path should parse");

        assert_eq!(parsed.as_os_str().as_bytes(), b"/tmp/repository-\xff");
    }

    #[test]
    fn object_ids_must_be_full_hex_values() {
        assert!(parse_object_id(format!("{}\n", "a".repeat(40)).as_bytes()).is_ok());
        assert!(parse_object_id(b"abc123\n").is_err());
        assert!(parse_object_id(format!("{}z\n", "a".repeat(39)).as_bytes()).is_err());
    }
}
