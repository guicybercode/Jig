use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use crate::GitError;
use crate::error::truncate_stderr;
use crate::status::{GitDiff, GitStatus, parse_porcelain_v2};
use crate::worktree::{RemovalPlan, WorktreeCreate, WorktreeInfo, slugify};

const DIFF_LIMIT: usize = 2 * 1024 * 1024;

/// Executes Git with an argument array and never a shell string.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitService {
    executable: PathBuf,
}

impl GitService {
    /// Creates a service around an already resolved Git executable.
    ///
    /// # Errors
    ///
    /// Returns an error when the path is empty.
    pub fn new(executable: impl Into<PathBuf>) -> Result<Self, GitError> {
        let executable = executable.into();
        if executable.as_os_str().is_empty() {
            return Err(GitError::GitNotFound);
        }
        Ok(Self { executable })
    }

    /// Resolves `git` from the current process `PATH`.
    ///
    /// # Errors
    ///
    /// Returns [`GitError::GitNotFound`] when no executable `git` exists.
    pub fn from_path_env() -> Result<Self, GitError> {
        let path_value = std::env::var_os("PATH").ok_or(GitError::GitNotFound)?;
        for directory in std::env::split_paths(&path_value) {
            let candidate = directory.join("git");
            if is_executable(&candidate) {
                return Self::new(candidate);
            }
        }
        Err(GitError::GitNotFound)
    }

    /// Returns the resolved Git executable.
    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// Returns the canonical repository root for a path inside a work tree.
    ///
    /// # Errors
    ///
    /// Returns an error when Git is missing, the path is not a repository, or
    /// Git fails.
    pub fn discover_repository(&self, path: &Path) -> Result<PathBuf, GitError> {
        let output = self.run(Some(path), &["rev-parse", "--show-toplevel"])?;
        let root = stdout_string("rev-parse", &output)?.trim().to_owned();
        if root.is_empty() {
            return Err(GitError::NotARepository(path.to_path_buf()));
        }
        Ok(PathBuf::from(root))
    }

    /// Creates a repository with a first commit. Used by tests and fixtures.
    ///
    /// # Errors
    ///
    /// Returns an error when Git fails to initialize or commit.
    pub fn init_with_commit(&self, path: &Path, message: &str) -> Result<PathBuf, GitError> {
        fs::create_dir_all(path).map_err(GitError::Spawn)?;
        self.run(Some(path), &["init"])?;
        self.run(Some(path), &["config", "user.name", "CLI Master Tests"])?;
        self.run(
            Some(path),
            &["config", "user.email", "tests@cli-master.local"],
        )?;
        let readme = path.join("README.md");
        fs::write(&readme, "fixture repository\n").map_err(GitError::Spawn)?;
        self.run(Some(path), &["add", "README.md"])?;
        self.run(Some(path), &["commit", "-m", message])?;
        self.discover_repository(path)
    }

    /// Returns porcelain v2 status for a work tree.
    ///
    /// # Errors
    ///
    /// Returns an error when Git fails or output cannot be parsed.
    pub fn status(&self, worktree: &Path) -> Result<GitStatus, GitError> {
        let output = self.run(
            Some(worktree),
            &[
                "status",
                "--porcelain=v2",
                "-z",
                "--untracked-files=all",
                "--branch",
            ],
        )?;
        parse_porcelain_v2(&output.stdout)
    }

    /// Returns a size-capped textual diff.
    ///
    /// # Errors
    ///
    /// Returns an error when Git fails.
    pub fn diff(&self, worktree: &Path) -> Result<GitDiff, GitError> {
        let output = self.run(
            Some(worktree),
            &["diff", "--no-color", "--no-ext-diff", "--no-prefix"],
        )?;
        let mut text =
            String::from_utf8(output.stdout).map_err(|_| GitError::InvalidUtf8("diff"))?;
        let truncated = text.len() > DIFF_LIMIT;
        if truncated {
            text.truncate(DIFF_LIMIT);
        }
        Ok(GitDiff { text, truncated })
    }

    /// Suggests a collision-resistant branch name for a task label.
    #[must_use]
    pub fn suggest_branch(task_label: &str) -> String {
        format!("agent/{}-{}", slugify(task_label), short_suffix())
    }

    /// Suggests a worktree directory below a managed root.
    #[must_use]
    pub fn suggest_worktree_path(
        managed_root: &Path,
        project_id: &str,
        task_label: &str,
    ) -> PathBuf {
        managed_root
            .join("worktrees")
            .join(project_id)
            .join(format!("{}-{}", slugify(task_label), short_suffix()))
    }

    /// Creates a new branch and worktree under the managed root.
    ///
    /// # Errors
    ///
    /// Returns an error when the path escapes the managed root or Git fails.
    pub fn create_worktree(&self, request: &WorktreeCreate<'_>) -> Result<WorktreeInfo, GitError> {
        let repository = self.discover_repository(request.repository)?;
        let path = request.path.clone().unwrap_or_else(|| {
            Self::suggest_worktree_path(
                request.managed_root,
                request.project_id,
                request.task_label,
            )
        });
        let branch = request
            .branch
            .clone()
            .unwrap_or_else(|| Self::suggest_branch(request.task_label));
        ensure_managed_path(request.managed_root, &path)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(GitError::Spawn)?;
        }
        self.run(
            Some(&repository),
            &[
                "worktree",
                "add",
                "-b",
                &branch,
                path.to_str().ok_or(GitError::InvalidUtf8("worktree add"))?,
            ],
        )?;
        Ok(WorktreeInfo {
            path,
            branch,
            is_primary: false,
        })
    }

    /// Lists worktrees registered with the repository.
    ///
    /// # Errors
    ///
    /// Returns an error when Git fails.
    pub fn list_worktrees(&self, repository: &Path) -> Result<Vec<WorktreeInfo>, GitError> {
        let repository = self.discover_repository(repository)?;
        let output = self.run(
            Some(&repository),
            &["worktree", "list", "--porcelain", "-z"],
        )?;
        parse_worktree_list(&output.stdout)
    }

    /// Inspects a worktree and issues a short-lived confirmation token.
    ///
    /// # Errors
    ///
    /// Returns an error when the path is not a removable worktree.
    pub fn prepare_remove(
        &self,
        repository: &Path,
        worktree: &Path,
    ) -> Result<RemovalPlan, GitError> {
        let repository = self.discover_repository(repository)?;
        let worktrees = self.list_worktrees(&repository)?;
        let info = worktrees
            .iter()
            .find(|info| paths_equal(&info.path, worktree))
            .ok_or_else(|| GitError::UnknownWorktree(worktree.to_path_buf()))?;
        if info.is_primary {
            return Err(GitError::PrimaryWorktree(info.path.clone()));
        }
        let status = self.status(&info.path)?;
        let token = removal_token(&info.path, status.is_dirty);
        Ok(RemovalPlan {
            repository,
            path: info.path.clone(),
            branch: info.branch.clone(),
            is_dirty: status.is_dirty,
            token,
        })
    }

    /// Removes a worktree after re-checking the confirmation token.
    ///
    /// Dirty worktrees are refused unless `allow_dirty` is true. The token is
    /// recomputed from current Git state and must match.
    ///
    /// # Errors
    ///
    /// Returns [`GitError::DirtyWorktree`] or [`GitError::StaleRemovalToken`]
    /// when removal is unsafe.
    pub fn remove_worktree(&self, plan: &RemovalPlan, allow_dirty: bool) -> Result<(), GitError> {
        let current = self.prepare_remove(&plan.repository, &plan.path)?;
        if current.token != plan.token || current.is_dirty != plan.is_dirty {
            return Err(GitError::StaleRemovalToken);
        }
        if current.is_dirty && !allow_dirty {
            return Err(GitError::DirtyWorktree { path: current.path });
        }
        let path = current
            .path
            .to_str()
            .ok_or(GitError::InvalidUtf8("worktree remove"))?;
        if allow_dirty && current.is_dirty {
            self.run(
                Some(&current.repository),
                &["worktree", "remove", "--force", path],
            )?;
        } else {
            self.run(Some(&current.repository), &["worktree", "remove", path])?;
        }
        Ok(())
    }

    fn run(&self, cwd: Option<&Path>, args: &[&str]) -> Result<Output, GitError> {
        let mut command = Command::new(&self.executable);
        command.arg("--no-pager");
        if let Some(cwd) = cwd {
            command.arg("-C").arg(cwd);
        }
        command.args(args);
        command.env("GIT_TERMINAL_PROMPT", "0");
        command.env("GIT_OPTIONAL_LOCKS", "0");
        command.env("GIT_PAGER", "cat");
        command.env("LC_ALL", "C");
        command.stdin(Stdio::null());
        let output = command.output().map_err(GitError::Spawn)?;
        if output.status.success() {
            Ok(output)
        } else {
            Err(GitError::CommandFailed {
                operation: args.first().copied().unwrap_or("git").to_owned(),
                status: output.status,
                stderr: truncate_stderr(&output.stderr),
            })
        }
    }
}

fn stdout_string(operation: &'static str, output: &Output) -> Result<String, GitError> {
    String::from_utf8(output.stdout.clone()).map_err(|_| GitError::InvalidUtf8(operation))
}

fn is_executable(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|metadata| {
        metadata.is_file() && {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                metadata.permissions().mode() & 0o111 != 0
            }
            #[cfg(not(unix))]
            {
                true
            }
        }
    })
}

fn ensure_managed_path(root: &Path, path: &Path) -> Result<(), GitError> {
    fs::create_dir_all(root).map_err(GitError::Spawn)?;
    let canonical_root = root.canonicalize().map_err(GitError::Spawn)?;
    let parent = path
        .parent()
        .ok_or_else(|| GitError::PathOutsideManagedRoot {
            path: path.to_path_buf(),
            root: canonical_root.clone(),
        })?;
    fs::create_dir_all(parent).map_err(GitError::Spawn)?;
    let canonical_parent = parent.canonicalize().map_err(GitError::Spawn)?;
    let name = path
        .file_name()
        .ok_or_else(|| GitError::PathOutsideManagedRoot {
            path: path.to_path_buf(),
            root: canonical_root.clone(),
        })?;
    let canonical = canonical_parent.join(name);
    if !canonical.starts_with(&canonical_root) {
        return Err(GitError::PathOutsideManagedRoot {
            path: canonical,
            root: canonical_root,
        });
    }
    Ok(())
}

fn parse_worktree_list(bytes: &[u8]) -> Result<Vec<WorktreeInfo>, GitError> {
    let text =
        String::from_utf8(bytes.to_vec()).map_err(|_| GitError::InvalidUtf8("worktree list"))?;
    let mut worktrees = Vec::new();
    let mut current_path: Option<PathBuf> = None;
    let mut current_branch = "HEAD".to_owned();
    let mut is_primary = false;

    for record in text.split('\0') {
        if record.is_empty() {
            continue;
        }
        for line in record.lines() {
            if let Some(path) = line.strip_prefix("worktree ") {
                if let Some(existing) = current_path.take() {
                    worktrees.push(WorktreeInfo {
                        path: existing,
                        branch: std::mem::replace(&mut current_branch, "HEAD".to_owned()),
                        is_primary,
                    });
                    is_primary = false;
                }
                current_path = Some(PathBuf::from(path));
            } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
                branch.clone_into(&mut current_branch);
            } else if line == "bare" {
                is_primary = true;
            }
        }
    }
    if let Some(existing) = current_path {
        worktrees.push(WorktreeInfo {
            path: existing,
            branch: current_branch,
            is_primary,
        });
    }

    if let Some(first) = worktrees.first_mut() {
        first.is_primary = true;
    }
    Ok(worktrees)
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn removal_token(path: &Path, is_dirty: bool) -> String {
    format!("{}:{is_dirty}", path.display())
}

fn short_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.subsec_nanos());
    format!("{nanos:x}")
}
