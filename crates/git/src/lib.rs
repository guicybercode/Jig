//! Git operations that always use a structured executable and argument array.
//!
//! The backend never runs `git reset --hard`, never force-removes a dirty
//! worktree without an explicit confirmation, and never deletes a repository
//! root.

#![warn(missing_docs)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use cli_master_core::{
    ApplicationError, DIFF_MAX_BYTES, ErrorCode, GIT_COMMAND_TIMEOUT, VERSION_DETECT_TIMEOUT,
};
use cli_master_safety::{
    Logger, ManagedRoots, SpawnRequest, StructuredLog, assert_managed_worktree,
    canonicalize_existing, run_command_unchecked,
};

/// Outcome of inspecting a Git worktree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorktreeStatus {
    /// Canonical worktree root.
    pub root: PathBuf,
    /// Checked-out branch, when Git reports one.
    pub branch: Option<String>,
    /// Whether porcelain status reported any changes.
    pub dirty: bool,
}

/// Bounded textual diff.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffOutput {
    /// Diff text, possibly truncated.
    pub text: String,
    /// Whether the diff exceeded [`DIFF_MAX_BYTES`].
    pub truncated: bool,
}

/// Git service backed by the system `git` executable.
#[derive(Clone, Debug)]
pub struct GitService {
    git: PathBuf,
}

impl GitService {
    /// Resolves `git` from PATH.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorCode::GitCommandFailed`] when Git is not installed.
    pub fn from_path() -> Result<Self, ApplicationError> {
        let output = run_command_unchecked(
            &SpawnRequest::new("git")
                .arg("--exec-path")
                .timeout(VERSION_DETECT_TIMEOUT)
                .env("GIT_TERMINAL_PROMPT", "0"),
        )?;
        if !output.success() {
            return Err(missing_git());
        }
        Ok(Self {
            git: PathBuf::from("git"),
        })
    }

    /// Creates a service that uses an explicit Git executable.
    #[must_use]
    pub fn new(git: impl Into<PathBuf>) -> Self {
        Self { git: git.into() }
    }

    /// Confirms that `path` is inside a Git repository and returns the toplevel.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorCode::NotAGitRepository`] when Git cannot find a toplevel.
    pub fn repository_root(&self, path: &Path) -> Result<PathBuf, ApplicationError> {
        let path = existing_directory(path)?;
        let output = self.git_at(
            &path,
            ["rev-parse", "--show-toplevel"],
            GIT_COMMAND_TIMEOUT,
            16 * 1024,
        )?;
        if !output.success() {
            return Err(ApplicationError::new(
                ErrorCode::NotAGitRepository,
                format!("{} is not a Git repository.", path.display()),
            )
            .with_action("Select a directory that contains a `.git` folder.")
            .with_context("path", path.display().to_string()));
        }
        let root = PathBuf::from(output.stdout_text());
        canonicalize_existing(&root)
    }

    /// Confirms that `path` is the root of a worktree, not a subdirectory.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorCode::InvalidPath`] when the path is not the worktree root.
    pub fn confirm_worktree_root(&self, path: &Path) -> Result<PathBuf, ApplicationError> {
        let root = self.repository_root(path)?;
        let requested = canonicalize_existing(path)?;
        if root != requested {
            return Err(ApplicationError::new(
                ErrorCode::InvalidPath,
                "The selected path is not the worktree root.",
            )
            .with_action("Select the worktree root, not a nested directory.")
            .with_context("path", requested.display().to_string())
            .with_context("worktreeRoot", root.display().to_string()));
        }
        Ok(root)
    }

    /// Detects when a stored project path no longer matches Git's toplevel.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorCode::RepositoryMoved`] when the roots differ.
    pub fn confirm_stored_root(
        &self,
        stored: &Path,
        observed: &Path,
    ) -> Result<PathBuf, ApplicationError> {
        let stored_root = self.repository_root(stored)?;
        let observed_root = self.repository_root(observed)?;
        if stored_root != observed_root {
            return Err(ApplicationError::new(
                ErrorCode::RepositoryMoved,
                "The repository was moved or is no longer at the stored path.",
            )
            .with_action("Remove the project from CLI Master and add it again.")
            .with_context("storedPath", stored_root.display().to_string())
            .with_context("observedPath", observed_root.display().to_string()));
        }
        Ok(stored_root)
    }

    /// Inspects porcelain status, including untracked files.
    ///
    /// # Errors
    ///
    /// Returns a Git error when status cannot be read.
    pub fn worktree_status(&self, path: &Path) -> Result<WorktreeStatus, ApplicationError> {
        let root = self.repository_root(path)?;
        let branch = self.current_branch(&root)?;
        let output = self.git_at(
            &root,
            [
                "--no-optional-locks",
                "status",
                "--porcelain=v2",
                "-z",
                "--untracked-files=all",
            ],
            GIT_COMMAND_TIMEOUT,
            DIFF_MAX_BYTES,
        )?;
        if !output.success() {
            return Err(git_failed(&root, "Could not read Git status."));
        }
        let dirty = output.stdout.iter().any(|byte| *byte != 0);
        Ok(WorktreeStatus {
            root,
            branch,
            dirty,
        })
    }

    /// Returns a size-capped textual diff.
    ///
    /// # Errors
    ///
    /// Returns a Git error when diff cannot be read.
    pub fn diff(&self, path: &Path) -> Result<DiffOutput, ApplicationError> {
        let root = self.repository_root(path)?;
        let output = self.git_at(
            &root,
            ["--no-optional-locks", "diff", "--no-ext-diff", "--text"],
            GIT_COMMAND_TIMEOUT,
            DIFF_MAX_BYTES,
        )?;
        if output.timed_out {
            return Err(ApplicationError::new(
                ErrorCode::CommandTimeout,
                "Git diff did not finish before the timeout.",
            )
            .with_action("Narrow the changes or retry. Large diffs are capped at 2 MiB."));
        }
        if !output.success() {
            return Err(git_failed(&root, "Could not read the Git diff."));
        }
        Ok(DiffOutput {
            truncated: output.truncated,
            text: String::from_utf8_lossy(&output.stdout).into_owned(),
        })
    }

    /// Removes a managed worktree. `--force` is used only when `allow_force`.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorCode::WorktreeDirty`] or [`ErrorCode::UnmanagedPath`] when
    /// removal is unsafe.
    pub fn remove_worktree(
        &self,
        repo: &Path,
        worktree: &Path,
        roots: &ManagedRoots,
        allow_force: bool,
    ) -> Result<(), ApplicationError> {
        let repo = self.repository_root(repo)?;
        let worktree = assert_managed_worktree(worktree, roots)?;
        let status = self.worktree_status(&worktree)?;
        if status.dirty && !allow_force {
            return Err(ApplicationError::new(
                ErrorCode::WorktreeDirty,
                format!(
                    "Worktree {} on branch {} has uncommitted changes.",
                    worktree.display(),
                    status.branch.as_deref().unwrap_or("(detached)")
                ),
            )
            .with_action("Commit or move the changes, or confirm dirty removal explicitly.")
            .with_context("path", worktree.display().to_string())
            .with_context("branch", status.branch.clone().unwrap_or_default()));
        }

        Logger::global().write(
            &StructuredLog::new(
                cli_master_safety::LogLevel::Info,
                "git",
                "worktree.remove",
                format!(
                    "removing worktree {} force={}",
                    worktree.display(),
                    allow_force
                ),
            )
            .with_project(repo.display().to_string()),
        );

        let mut args = vec![
            "-C".to_owned(),
            repo.display().to_string(),
            "worktree".to_owned(),
            "remove".to_owned(),
        ];
        if allow_force {
            args.push("--force".to_owned());
        }
        args.push(worktree.display().to_string());

        let output = run_command_unchecked(
            &SpawnRequest::new(&self.git)
                .args(args)
                .timeout(GIT_COMMAND_TIMEOUT)
                .env("GIT_TERMINAL_PROMPT", "0")
                .env("GIT_OPTIONAL_LOCKS", "0")
                .env("GIT_PAGER", "cat"),
        )?;
        if !output.success() {
            if status.dirty {
                return Err(ApplicationError::new(
                    ErrorCode::WorktreeDirty,
                    "Git refused to remove a dirty worktree.",
                )
                .with_action(
                    "Confirm dirty removal explicitly. CLI Master will not force this silently.",
                ));
            }
            return Err(git_failed(&worktree, "Git could not remove the worktree."));
        }
        Ok(())
    }

    fn current_branch(&self, root: &Path) -> Result<Option<String>, ApplicationError> {
        let output = self.git_at(
            root,
            ["branch", "--show-current"],
            GIT_COMMAND_TIMEOUT,
            4096,
        )?;
        if !output.success() {
            return Ok(None);
        }
        let name = output.stdout_text();
        if name.is_empty() {
            Ok(None)
        } else {
            Ok(Some(name))
        }
    }

    fn git_at<I, S>(
        &self,
        cwd: &Path,
        args: I,
        timeout: Duration,
        max_output: usize,
    ) -> Result<cli_master_safety::ProcessOutput, ApplicationError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        run_command_unchecked(
            &SpawnRequest::new(&self.git)
                .arg("-C")
                .arg(cwd.display().to_string())
                .args(args)
                .timeout(timeout)
                .max_output(max_output)
                .env("GIT_TERMINAL_PROMPT", "0")
                .env("GIT_OPTIONAL_LOCKS", "0")
                .env("GIT_PAGER", "cat")
                .env("PAGER", "cat")
                .env("LC_ALL", "C"),
        )
    }
}

fn existing_directory(path: &Path) -> Result<PathBuf, ApplicationError> {
    let resolved = canonicalize_existing(path)?;
    if resolved.is_dir() {
        Ok(resolved)
    } else {
        Err(ApplicationError::new(
            ErrorCode::InvalidPath,
            format!("{} is not a directory.", resolved.display()),
        )
        .with_action("Select a Git repository directory.")
        .with_context("path", resolved.display().to_string()))
    }
}

fn missing_git() -> ApplicationError {
    ApplicationError::new(ErrorCode::GitCommandFailed, "Git was not found on PATH.")
        .with_action("Install Git and make sure the CLI Master PATH includes it.")
}

fn git_failed(path: &Path, message: &str) -> ApplicationError {
    ApplicationError::new(ErrorCode::GitCommandFailed, message.to_owned())
        .with_action("Check that the repository is readable and Git is installed.")
        .with_context("path", path.display().to_string())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use tempfile::TempDir;

    use super::*;
    use cli_master_safety::ManagedRoots;

    fn git(args: &[&str], cwd: &Path) {
        let status = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_AUTHOR_NAME", "CLI Master")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "CLI Master")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .status()
            .expect("git should spawn");
        assert!(status.success(), "git {args:?} failed");
    }

    fn repo() -> (TempDir, PathBuf, GitService) {
        let temp = TempDir::new().expect("temp");
        let root = temp.path().join("repo");
        fs::create_dir_all(&root).expect("repo");
        git(&["init", "-b", "main"], &root);
        git(&["config", "user.email", "test@example.com"], &root);
        git(&["config", "user.name", "CLI Master"], &root);
        git(&["commit", "--allow-empty", "-m", "init"], &root);
        let service = GitService::from_path().expect("git");
        (temp, root, service)
    }

    #[test]
    fn detects_repository_root_and_rejects_plain_directories() {
        let (_temp, root, service) = repo();
        assert_eq!(
            service.repository_root(&root).expect("root"),
            fs::canonicalize(&root).expect("canon")
        );
        let nested = root.join("nested");
        fs::create_dir_all(&nested).expect("nested");
        assert_eq!(
            service.repository_root(&nested).expect("nested"),
            fs::canonicalize(&root).expect("canon")
        );
        let outside = TempDir::new().expect("outside");
        let error = service
            .repository_root(outside.path())
            .expect_err("plain dir");
        assert_eq!(error.code(), ErrorCode::NotAGitRepository);
        assert!(error.suggested_action().is_some());
    }

    #[test]
    fn dirty_worktree_is_not_removed_without_force() {
        let (temp, root, service) = repo();
        let data = temp.path().join("data");
        let worktree = data.join("worktrees/project/topic");
        fs::create_dir_all(worktree.parent().expect("parent")).expect("parent");
        git(
            &[
                "worktree",
                "add",
                "-b",
                "agent/topic",
                worktree.to_str().expect("utf8"),
            ],
            &root,
        );
        fs::write(worktree.join("dirty.txt"), "changes").expect("dirty");
        let roots = ManagedRoots::new(&data);
        let status = service.worktree_status(&worktree).expect("status");
        assert!(status.dirty);
        assert_eq!(status.branch.as_deref(), Some("agent/topic"));

        let error = service
            .remove_worktree(&root, &worktree, &roots, false)
            .expect_err("dirty");
        assert_eq!(error.code(), ErrorCode::WorktreeDirty);
        assert!(worktree.exists());
    }

    #[test]
    fn unicode_and_spaces_in_repository_paths_work() {
        let temp = TempDir::new().expect("temp");
        let root = temp.path().join("projeto café");
        fs::create_dir_all(&root).expect("repo");
        git(&["init", "-b", "main"], &root);
        git(&["config", "user.email", "test@example.com"], &root);
        git(&["config", "user.name", "CLI Master"], &root);
        git(&["commit", "--allow-empty", "-m", "init"], &root);
        let service = GitService::from_path().expect("git");
        let resolved = service.repository_root(&root).expect("root");
        assert!(resolved.to_string_lossy().contains("projeto café"));
    }

    #[test]
    fn command_args_do_not_use_a_shell() {
        let (_temp, root, service) = repo();
        // A path that would be destructive if interpolated into sh -c.
        let nested = root.join("hello; rm -rf should-not-run");
        fs::create_dir_all(&nested).expect("nested");
        let resolved = service.repository_root(&nested).expect("still a repo");
        assert_eq!(resolved, fs::canonicalize(&root).expect("canon"));
        assert!(!root.join("should-not-run").exists());
    }
}
