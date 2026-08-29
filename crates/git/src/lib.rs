//! System Git operations with structured arguments and worktree safety.

#![warn(missing_docs)]

use std::env;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use cli_master_core::GitStatus;

const GIT_TIMEOUT: Duration = Duration::from_secs(30);
const DIFF_LIMIT: usize = 2 * 1024 * 1024;
const OUTPUT_LIMIT: usize = 4 * 1024 * 1024;

/// Error raised by [`GitService`].
#[derive(Debug)]
pub enum GitError {
    /// No `git` executable was found.
    NotFound,
    /// The selected path is not a Git repository.
    NotARepository {
        /// Inspected path.
        path: PathBuf,
    },
    /// The path cannot be read or is not a directory.
    Unreadable {
        /// Inspected path.
        path: PathBuf,
        /// Underlying I/O error.
        source: io::Error,
    },
    /// A Git command failed, timed out, or exceeded its output budget.
    CommandFailed {
        /// Safe explanation without secrets.
        message: String,
        /// Optional Git stderr, truncated.
        stderr: Option<String>,
    },
    /// A generated worktree path escaped the managed root.
    PathEscaped {
        /// Requested path.
        path: PathBuf,
    },
    /// The branch or worktree directory already exists.
    AlreadyExists {
        /// Conflicting branch or path.
        target: String,
    },
    /// Removal is blocked because the worktree has uncommitted changes.
    Dirty {
        /// Worktree path.
        path: PathBuf,
    },
}

impl std::fmt::Display for GitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => formatter.write_str("git executable was not found"),
            Self::NotARepository { path } => {
                write!(formatter, "not a git repository: {}", path.display())
            }
            Self::Unreadable { path, source } => {
                write!(formatter, "cannot read {}: {source}", path.display())
            }
            Self::CommandFailed { message, .. } => formatter.write_str(message),
            Self::PathEscaped { path } => {
                write!(
                    formatter,
                    "worktree path is outside the managed root: {}",
                    path.display()
                )
            }
            Self::AlreadyExists { target } => {
                write!(formatter, "branch or worktree already exists: {target}")
            }
            Self::Dirty { path } => write!(
                formatter,
                "worktree has uncommitted changes: {}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for GitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Unreadable { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Size-capped textual diff.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitDiffOutput {
    /// Diff text, possibly truncated.
    pub text: String,
    /// Whether the cap was hit.
    pub truncated: bool,
}

/// System Git adapter. Commands always use an executable plus an argument array.
#[derive(Clone, Debug)]
pub struct GitService {
    executable: PathBuf,
}

impl GitService {
    /// Resolves `git` from the current process PATH and common Unix locations.
    ///
    /// # Errors
    ///
    /// Returns [`GitError::NotFound`] when no executable Git binary exists.
    pub fn from_environment() -> Result<Self, GitError> {
        if let Some(path) = env::var_os("CLI_MASTER_GIT") {
            let candidate = PathBuf::from(path);
            if is_executable(&candidate) {
                return Ok(Self {
                    executable: candidate,
                });
            }
        }
        if let Some(path) = env::var_os("PATH") {
            for directory in env::split_paths(&path) {
                let candidate = directory.join("git");
                if is_executable(&candidate) {
                    return Ok(Self {
                        executable: candidate,
                    });
                }
            }
        }
        for candidate in [
            "/usr/bin/git",
            "/usr/local/bin/git",
            "/opt/homebrew/bin/git",
        ] {
            let candidate = PathBuf::from(candidate);
            if is_executable(&candidate) {
                return Ok(Self {
                    executable: candidate,
                });
            }
        }
        Err(GitError::NotFound)
    }

    /// Returns the resolved Git executable.
    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// Canonicalizes `path` and returns `git rev-parse --show-toplevel`.
    ///
    /// # Errors
    ///
    /// Returns an error when the path cannot be read or is not a repository.
    pub fn repository_root(&self, path: &Path) -> Result<PathBuf, GitError> {
        let canonical = canonicalize_existing(path)?;
        let output = self.run(&canonical, &["rev-parse", "--show-toplevel"], None)?;
        let root = output.trim();
        if root.is_empty() {
            return Err(GitError::NotARepository { path: canonical });
        }
        canonicalize_existing(Path::new(root))
    }

    /// Returns the current branch name, or `HEAD` when detached.
    ///
    /// # Errors
    ///
    /// Returns an error when Git cannot inspect the repository.
    pub fn current_branch(&self, worktree: &Path) -> Result<String, GitError> {
        let output = self.run(worktree, &["branch", "--show-current"], None)?;
        let branch = output.trim();
        if branch.is_empty() {
            Ok("HEAD".to_owned())
        } else {
            Ok(branch.to_owned())
        }
    }

    /// Parses porcelain v2 status, including untracked files.
    ///
    /// # Errors
    ///
    /// Returns an error when Git cannot inspect the working tree.
    pub fn status(&self, worktree: &Path) -> Result<GitStatus, GitError> {
        let repository_root = self.repository_root(worktree)?;
        let branch = self.current_branch(worktree)?;
        let output = self.run(
            worktree,
            &[
                "status",
                "--porcelain=v2",
                "-z",
                "--untracked-files=all",
                "--branch",
            ],
            None,
        )?;
        let changed_files = parse_porcelain_v2_paths(&output);
        Ok(GitStatus {
            repository_root,
            worktree_path: canonicalize_existing(worktree)?,
            branch,
            is_dirty: !changed_files.is_empty(),
            changed_file_count: u32::try_from(changed_files.len()).unwrap_or(u32::MAX),
            changed_files,
        })
    }

    /// Returns a size-capped `git diff HEAD` including untracked files via status.
    ///
    /// # Errors
    ///
    /// Returns an error when Git cannot produce the diff.
    pub fn diff(&self, worktree: &Path) -> Result<GitDiffOutput, GitError> {
        let (text, truncated) = self.run_capped(
            worktree,
            &["diff", "--no-ext-diff", "--binary", "HEAD"],
            DIFF_LIMIT,
        )?;
        Ok(GitDiffOutput { text, truncated })
    }

    /// Creates a new branch and worktree under `managed_root`.
    ///
    /// # Errors
    ///
    /// Returns an error when the path escapes the root, the branch exists, or
    /// Git rejects the worktree.
    pub fn create_worktree(
        &self,
        repository: &Path,
        managed_root: &Path,
        worktree_path: &Path,
        branch: &str,
    ) -> Result<(), GitError> {
        ensure_managed_path(managed_root, worktree_path)?;
        if worktree_path.exists() {
            return Err(GitError::AlreadyExists {
                target: worktree_path.display().to_string(),
            });
        }
        if self.branch_exists(repository, branch)? {
            return Err(GitError::AlreadyExists {
                target: branch.to_owned(),
            });
        }
        if let Some(parent) = worktree_path.parent() {
            fs::create_dir_all(parent).map_err(|source| GitError::Unreadable {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        match self.run(
            repository,
            &[
                "worktree",
                "add",
                "-b",
                branch,
                &worktree_path.to_string_lossy(),
            ],
            None,
        ) {
            Ok(_) => Ok(()),
            Err(error) => {
                let _ = fs::remove_dir_all(worktree_path);
                Err(error)
            }
        }
    }

    /// Removes a worktree. Dirty trees require `allow_dirty`.
    ///
    /// # Errors
    ///
    /// Returns [`GitError::Dirty`] when the tree has changes and `allow_dirty`
    /// is false.
    pub fn remove_worktree(
        &self,
        repository: &Path,
        worktree_path: &Path,
        allow_dirty: bool,
    ) -> Result<(), GitError> {
        let status = self.status(worktree_path)?;
        if status.is_dirty && !allow_dirty {
            return Err(GitError::Dirty {
                path: worktree_path.to_path_buf(),
            });
        }
        let path_text = worktree_path.to_string_lossy();
        if allow_dirty && status.is_dirty {
            self.run(
                repository,
                &["worktree", "remove", "--force", path_text.as_ref()],
                None,
            )?;
        } else {
            self.run(
                repository,
                &["worktree", "remove", path_text.as_ref()],
                None,
            )?;
        }
        Ok(())
    }

    /// Returns whether `branch` already exists in the repository.
    ///
    /// # Errors
    ///
    /// Returns an error when Git cannot list refs.
    pub fn branch_exists(&self, repository: &Path, branch: &str) -> Result<bool, GitError> {
        match self.run(
            repository,
            &[
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ],
            None,
        ) {
            Ok(_) => Ok(true),
            Err(GitError::CommandFailed { .. }) => Ok(false),
            Err(error) => Err(error),
        }
    }

    fn run(&self, cwd: &Path, args: &[&str], limit: Option<usize>) -> Result<String, GitError> {
        let (stdout, truncated) = self.run_capped(cwd, args, limit.unwrap_or(OUTPUT_LIMIT))?;
        if truncated {
            return Err(GitError::CommandFailed {
                message: "git command exceeded the output limit".to_owned(),
                stderr: None,
            });
        }
        Ok(stdout)
    }

    fn run_capped(
        &self,
        cwd: &Path,
        args: &[&str],
        limit: usize,
    ) -> Result<(String, bool), GitError> {
        let mut command = Command::new(&self.executable);
        command
            .args(["--no-pager"])
            .args(args)
            .current_dir(cwd)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_PAGER", "cat")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let child = command.spawn().map_err(|source| {
            if source.kind() == io::ErrorKind::NotFound {
                GitError::NotFound
            } else {
                GitError::CommandFailed {
                    message: format!("failed to start git: {source}"),
                    stderr: None,
                }
            }
        })?;
        let pid = child.id();
        let handle = thread::spawn(move || child.wait_with_output());
        let output = wait_for_output(handle, pid)?;
        let truncated = output.stdout.len() >= limit;
        let stdout = if truncated {
            output.stdout[..limit].to_vec()
        } else {
            output.stdout
        };
        if !output.status.success() {
            return Err(GitError::CommandFailed {
                message: format!(
                    "git {} failed with status {}",
                    args.first().copied().unwrap_or("command"),
                    output.status
                ),
                stderr: Some(String::from_utf8_lossy(&output.stderr).into_owned()),
            });
        }
        Ok((String::from_utf8_lossy(&stdout).into_owned(), truncated))
    }
}

/// Builds a collision-resistant branch name.
#[must_use]
pub fn branch_name(slug: &str, suffix: &str) -> String {
    format!("agent/{}-{suffix}", sanitize_slug(slug))
}

/// Builds a worktree directory name using the same slug and suffix.
#[must_use]
pub fn worktree_dir_name(slug: &str, suffix: &str) -> String {
    format!("{}-{suffix}", sanitize_slug(slug))
}

fn sanitize_slug(value: &str) -> String {
    let mut slug = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if matches!(ch, '-' | '_' | '.') {
            slug.push(ch);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-').to_owned();
    if slug.is_empty() {
        "session".to_owned()
    } else {
        slug
    }
}

fn canonicalize_existing(path: &Path) -> Result<PathBuf, GitError> {
    fs::canonicalize(path).map_err(|source| GitError::Unreadable {
        path: path.to_path_buf(),
        source,
    })
}

fn ensure_managed_path(root: &Path, candidate: &Path) -> Result<(), GitError> {
    let root = fs::canonicalize(root).or_else(|_| {
        fs::create_dir_all(root).map_err(|source| GitError::Unreadable {
            path: root.to_path_buf(),
            source,
        })?;
        fs::canonicalize(root).map_err(|source| GitError::Unreadable {
            path: root.to_path_buf(),
            source,
        })
    })?;
    let parent = candidate.parent().ok_or_else(|| GitError::PathEscaped {
        path: candidate.to_path_buf(),
    })?;
    fs::create_dir_all(parent).map_err(|source| GitError::Unreadable {
        path: parent.to_path_buf(),
        source,
    })?;
    let parent = fs::canonicalize(parent).map_err(|source| GitError::Unreadable {
        path: parent.to_path_buf(),
        source,
    })?;
    if !parent.starts_with(&root) {
        return Err(GitError::PathEscaped {
            path: candidate.to_path_buf(),
        });
    }
    Ok(())
}

fn parse_porcelain_v2_paths(output: &str) -> Vec<String> {
    let mut files = Vec::new();
    for record in output.split('\0') {
        if record.is_empty() || record.starts_with('#') {
            continue;
        }
        if let Some(path) = porcelain_path(record) {
            files.push(path);
        }
    }
    files
}

fn porcelain_path(record: &str) -> Option<String> {
    let kind = record.chars().next()?;
    match kind {
        '1' | '2' | 'u' => record.split_whitespace().next_back().map(ToOwned::to_owned),
        '?' | '!' => Some(record[1..].trim().to_owned()),
        _ => None,
    }
}

fn is_executable(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|metadata| {
        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
    })
}

fn wait_for_output(
    handle: thread::JoinHandle<io::Result<std::process::Output>>,
    pid: u32,
) -> Result<std::process::Output, GitError> {
    let deadline = Instant::now() + GIT_TIMEOUT;
    loop {
        if handle.is_finished() {
            return handle
                .join()
                .map_err(|_| GitError::CommandFailed {
                    message: "git worker thread panicked".to_owned(),
                    stderr: None,
                })?
                .map_err(|source| GitError::CommandFailed {
                    message: format!("failed to wait for git: {source}"),
                    stderr: None,
                });
        }
        if Instant::now() >= deadline {
            let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
            let _ = handle.join();
            return Err(GitError::CommandFailed {
                message: "git command exceeded the time limit".to_owned(),
                stderr: None,
            });
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCommand;
    use tempfile::TempDir;

    fn git_available() -> bool {
        GitService::from_environment().is_ok()
    }

    fn init_repo() -> (TempDir, GitService, PathBuf) {
        let temp = TempDir::new().expect("tempdir");
        let service = GitService::from_environment().expect("git should exist in CI");
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).expect("repo dir");
        run_git(&service, &repo, &["init", "-b", "main"]);
        run_git(
            &service,
            &repo,
            &["config", "user.email", "dev@example.com"],
        );
        run_git(&service, &repo, &["config", "user.name", "Dev"]);
        fs::write(repo.join("README.md"), "hello\n").expect("write");
        run_git(&service, &repo, &["add", "README.md"]);
        run_git(&service, &repo, &["commit", "-m", "init"]);
        (temp, service, repo)
    }

    fn run_git(service: &GitService, cwd: &Path, args: &[&str]) {
        let status = StdCommand::new(service.executable())
            .args(args)
            .current_dir(cwd)
            .status()
            .expect("git spawn");
        assert!(status.success(), "git {args:?} failed");
    }

    #[test]
    fn repository_root_and_branch_are_canonical() {
        if !git_available() {
            return;
        }
        let (_temp, service, repo) = init_repo();
        let nested = repo.join("src");
        fs::create_dir_all(&nested).expect("nested");
        let root = service.repository_root(&nested).expect("root");
        assert_eq!(root, fs::canonicalize(&repo).expect("canon"));
        assert_eq!(service.current_branch(&repo).expect("branch"), "main");
    }

    #[test]
    fn dirty_worktree_removal_is_blocked() {
        if !git_available() {
            return;
        }
        let (temp, service, repo) = init_repo();
        let managed = temp.path().join("worktrees");
        let worktree = managed.join("feature-abc123");
        service
            .create_worktree(&repo, &managed, &worktree, "agent/feature-abc123")
            .expect("create");
        fs::write(worktree.join("dirty.txt"), "nope\n").expect("dirty");
        let status = service.status(&worktree).expect("status");
        assert!(status.is_dirty);
        let error = service
            .remove_worktree(&repo, &worktree, false)
            .expect_err("dirty remove should fail");
        assert!(matches!(error, GitError::Dirty { .. }));
        assert!(worktree.exists());
        service
            .remove_worktree(&repo, &worktree, true)
            .expect("forced dirty remove after explicit allow");
        assert!(!worktree.exists());
    }

    #[test]
    fn existing_branch_is_rejected() {
        if !git_available() {
            return;
        }
        let (temp, service, repo) = init_repo();
        let managed = temp.path().join("worktrees");
        let first = managed.join("one-abc123");
        service
            .create_worktree(&repo, &managed, &first, "agent/one-abc123")
            .expect("first");
        let second = managed.join("two-abc123");
        let error = service
            .create_worktree(&repo, &managed, &second, "agent/one-abc123")
            .expect_err("duplicate branch");
        assert!(matches!(error, GitError::AlreadyExists { .. }));
    }

    #[test]
    fn slug_is_lowercase_ascii() {
        assert_eq!(
            branch_name("Implement Authentication!", "7k3m"),
            "agent/implement-authentication-7k3m"
        );
        assert_eq!(worktree_dir_name("  ", "7k3m"), "session-7k3m");
    }
}
