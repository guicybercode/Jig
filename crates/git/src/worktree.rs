use std::{
    collections::{BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use cli_master_core::{ProjectId, RequestId, SessionId};
use serde::{Deserialize, Serialize};

use crate::{
    command::GitCommand,
    detect,
    error::GitError,
    naming::{self, AllocatedNames},
    paths::{
        create_missing_ancestors, ensure_within, paths_equal, real_or_absolute, reject_symlink,
        remove_empty_dirs,
    },
    status,
};

const CONFIRMATION_TTL: Duration = Duration::from_secs(5 * 60);

/// What to do when the generated branch name is already taken.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExistingBranchBehavior {
    /// Keep generating suffixed names until a free branch and directory exist.
    #[default]
    AllocateUnique,
    /// Fail if the unsuffixed `agent/<slug>` branch already exists.
    Reject,
}

/// How much of a worktree to remove after confirmation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoveScope {
    /// Run `git worktree remove` on the directory.
    Directory,
    /// Leave Git and the directory alone. The caller may drop application metadata.
    MetadataOnly,
}

/// Request to create an isolated session worktree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateWorktreeRequest {
    /// Repository path or subdirectory used to locate the repo.
    pub repository: PathBuf,
    /// Session title used to generate the branch slug.
    pub session_name: String,
    /// Session id used for a stable suffix on collision.
    pub session_id: SessionId,
    /// Project id used as the managed directory segment.
    pub project_id: ProjectId,
    /// Optional base branch or commit. Defaults to `HEAD`.
    pub base_ref: Option<String>,
    /// Collision policy for the generated branch name.
    pub existing_branch: ExistingBranchBehavior,
}

/// Successfully created worktree.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedWorktree {
    /// Created worktree path.
    pub path: PathBuf,
    /// Created branch name.
    pub branch: String,
    /// Checked-out commit.
    pub head: String,
}

/// One Git worktree as reported by `git worktree list --porcelain -z`.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeInfo {
    /// Worktree path.
    pub path: PathBuf,
    /// `HEAD` object name, when Git reported one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    /// Checked-out branch, without `refs/heads/`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Whether this is a bare worktree.
    pub bare: bool,
    /// Whether `HEAD` is detached.
    pub detached: bool,
    /// Whether Git reports the worktree as locked.
    pub locked: bool,
    /// Lock reason, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lock_reason: Option<String>,
    /// Whether Git reports the worktree as prunable.
    pub prunable: bool,
    /// Prune reason, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prunable_reason: Option<String>,
    /// Dirty flag when status was consulted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dirty: Option<bool>,
    /// Whether this is the repository's primary checkout.
    pub is_primary: bool,
}

/// Options for [`crate::GitService::inspect_worktree`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InspectOptions {
    /// When true, run `git status` to populate [`WorktreeInfo::dirty`].
    pub include_dirty: bool,
}

/// Request to inspect a worktree before removal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrepareRemoveRequest {
    /// Worktree path.
    pub path: PathBuf,
    /// Whether a session process is still using this worktree.
    pub session_is_active: bool,
    /// Directory removal versus metadata-only.
    pub scope: RemoveScope,
}

/// Reason a worktree cannot be removed yet.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoveBlocker {
    /// Uncommitted or untracked changes are present.
    Dirty,
    /// A live session still owns the worktree.
    SessionActive,
    /// Git reports the worktree as locked.
    Locked,
    /// The path is the primary repository checkout.
    PrimaryWorktree,
    /// The path is outside the managed worktree root.
    OutsideManagedRoot,
    /// The path is a symlink.
    Symlink,
}

/// Result of prepare-remove. A token is present only when removal is allowed.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareRemoveResult {
    /// Inspected worktree path.
    pub path: PathBuf,
    /// Branch currently checked out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Whether the worktree is dirty.
    pub dirty: bool,
    /// Whether the caller reported an active session.
    pub session_is_active: bool,
    /// Whether Git reports the worktree as locked.
    pub locked: bool,
    /// Whether this is the primary checkout.
    pub is_primary: bool,
    /// Requested removal scope.
    pub scope: RemoveScope,
    /// Conditions that block removal.
    pub blockers: Vec<RemoveBlocker>,
    /// Confirmation token, present only when `blockers` is empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmation_token: Option<String>,
    /// Token expiry as Unix epoch milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at_ms: Option<i64>,
}

/// Confirmed worktree removal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoveWorktreeRequest {
    /// Worktree path.
    pub path: PathBuf,
    /// Token returned by prepare-remove.
    pub confirmation_token: String,
    /// Whether a session process is still using this worktree.
    pub session_is_active: bool,
    /// Directory removal versus metadata-only.
    pub scope: RemoveScope,
}

/// Outcome of a confirmed removal.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovedWorktree {
    /// Path that was targeted.
    pub path: PathBuf,
    /// Scope that was applied.
    pub scope: RemoveScope,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RemovalFingerprint {
    path: PathBuf,
    head: Option<String>,
    dirty: bool,
    locked: bool,
    scope: RemoveScope,
}

struct PendingRemoval {
    fingerprint: RemovalFingerprint,
    expires_at: Instant,
}

pub(crate) struct RemovalTokens {
    inner: Mutex<HashMap<String, PendingRemoval>>,
}

impl RemovalTokens {
    pub(crate) fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, HashMap<String, PendingRemoval>>, GitError> {
        self.inner.lock().map_err(|_| GitError::Internal {
            message: "worktree confirmation lock was poisoned".to_owned(),
        })
    }

    fn store(&self, fingerprint: RemovalFingerprint) -> Result<(String, Instant), GitError> {
        let mut guard = self.lock()?;
        let now = Instant::now();
        guard.retain(|_, pending| pending.expires_at > now);
        let token = RequestId::new().to_string();
        let expires_at = now + CONFIRMATION_TTL;
        guard.insert(
            token.clone(),
            PendingRemoval {
                fingerprint,
                expires_at,
            },
        );
        Ok((token, expires_at))
    }

    fn take(&self, token: &str) -> Result<PendingRemoval, GitError> {
        let mut guard = self.lock()?;
        let now = Instant::now();
        guard.retain(|_, pending| pending.expires_at > now);
        match guard.remove(token) {
            Some(pending) if pending.expires_at > now => Ok(pending),
            _ => Err(GitError::ConfirmationExpired),
        }
    }
}

pub(crate) fn list_worktrees(
    executable: &Path,
    repository: &Path,
) -> Result<Vec<WorktreeInfo>, GitError> {
    let repo = detect::detect_repository(executable, repository)?;
    let output = GitCommand::new(executable)
        .read_only()
        .repo(&repo.root)
        .arg("worktree")
        .arg("list")
        .arg("--porcelain")
        .arg("-z")
        .run_checked()?;
    parse_worktree_list(&output.stdout, &repo.git_common_dir)
}

pub(crate) fn inspect_worktree(
    executable: &Path,
    path: &Path,
    options: InspectOptions,
) -> Result<WorktreeInfo, GitError> {
    crate::command::inspect_path(path)?;
    reject_symlink(path)?;
    let detected = detect::detect_repository(executable, path)?;
    let mut listed = list_worktrees(executable, &detected.root)?;
    let mut info = listed
        .drain(..)
        .find(|worktree| paths_equal(&worktree.path, &detected.root))
        .ok_or_else(|| GitError::WorktreeUnknown {
            path: path.to_path_buf(),
        })?;
    if options.include_dirty {
        let git_status = status::status(executable, &info.path)?;
        info.dirty = Some(git_status.is_dirty());
    }
    Ok(info)
}

pub(crate) fn create_worktree(
    executable: &Path,
    managed_root: &Path,
    request: &CreateWorktreeRequest,
) -> Result<CreatedWorktree, GitError> {
    let repository = detect::detect_repository(executable, &request.repository)?;
    if repository.bare {
        return Err(GitError::BareRepository {
            path: repository.root,
        });
    }

    let base_ref = request.base_ref.as_deref().unwrap_or("HEAD");
    if repository.unborn && base_ref == "HEAD" {
        return Err(GitError::UnbornHead {
            path: repository.root,
        });
    }
    let head = detect::resolve_commit(executable, &repository.root, base_ref)?;

    let names = allocate(executable, managed_root, &repository.root, request)?;
    let path = managed_root
        .join(request.project_id.to_string())
        .join(&names.directory);
    let path = ensure_within(&path, managed_root)?;
    reject_symlink(&path)?;
    if path.exists() {
        return Err(GitError::WorktreeExists { path });
    }

    let created_dirs = create_missing_ancestors(&path, managed_root)?;
    let add = GitCommand::new(executable)
        .repo(&repository.root)
        .arg("worktree")
        .arg("add")
        .arg("--no-track")
        .arg("-b")
        .arg(&names.branch)
        .arg(&path)
        .arg(&head)
        .run();

    match add {
        Ok(output) if output.success() => {}
        Ok(output) => {
            remove_empty_dirs(&created_dirs);
            let args = vec![
                "worktree".to_owned(),
                "add".to_owned(),
                "--no-track".to_owned(),
                "-b".to_owned(),
                names.branch,
                path.display().to_string(),
                head,
            ];
            return Err(GitError::CommandFailed {
                args,
                exit_code: output.exit_code,
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            });
        }
        Err(error) => {
            remove_empty_dirs(&created_dirs);
            return Err(error);
        }
    }

    match confirm_created(executable, &path, &names.branch, &head) {
        Ok(created) => Ok(created),
        Err(error) => Err(rollback(executable, &repository.root, &path, &names, error)),
    }
}

pub(crate) fn prepare_remove(
    executable: &Path,
    managed_root: &Path,
    tokens: &RemovalTokens,
    request: &PrepareRemoveRequest,
) -> Result<PrepareRemoveResult, GitError> {
    let inspection = inspect_for_remove(executable, managed_root, request)?;
    let mut result = PrepareRemoveResult {
        path: inspection.info.path.clone(),
        branch: inspection.info.branch.clone(),
        dirty: inspection.dirty,
        session_is_active: request.session_is_active,
        locked: inspection.info.locked,
        is_primary: inspection.info.is_primary,
        scope: request.scope,
        blockers: inspection.blockers.clone(),
        confirmation_token: None,
        expires_at_ms: None,
    };
    if result.blockers.is_empty() {
        let (token, expires_at) = tokens.store(inspection.fingerprint)?;
        result.confirmation_token = Some(token);
        result.expires_at_ms = Some(instant_to_epoch_ms(expires_at));
    }
    Ok(result)
}

pub(crate) fn remove_worktree(
    executable: &Path,
    managed_root: &Path,
    tokens: &RemovalTokens,
    request: &RemoveWorktreeRequest,
) -> Result<RemovedWorktree, GitError> {
    if request.confirmation_token.is_empty() {
        return Err(GitError::ConfirmationRequired);
    }
    let pending = tokens.take(&request.confirmation_token)?;
    let inspection = inspect_for_remove(
        executable,
        managed_root,
        &PrepareRemoveRequest {
            path: request.path.clone(),
            session_is_active: request.session_is_active,
            scope: request.scope,
        },
    )?;
    if pending.fingerprint != inspection.fingerprint {
        return Err(GitError::ConfirmationMismatch);
    }
    if !inspection.blockers.is_empty() {
        return Err(blocker_error(
            inspection.blockers[0],
            &inspection.info,
            &request.path,
            managed_root,
        ));
    }

    if request.scope == RemoveScope::Directory {
        GitCommand::new(executable)
            .repo(&inspection.repository_root)
            .arg("worktree")
            .arg("remove")
            .arg(&inspection.info.path)
            .run_checked()?;
    }

    Ok(RemovedWorktree {
        path: inspection.info.path,
        scope: request.scope,
    })
}

struct RemoveInspection {
    info: WorktreeInfo,
    dirty: bool,
    blockers: Vec<RemoveBlocker>,
    fingerprint: RemovalFingerprint,
    repository_root: PathBuf,
}

fn inspect_for_remove(
    executable: &Path,
    managed_root: &Path,
    request: &PrepareRemoveRequest,
) -> Result<RemoveInspection, GitError> {
    let requested_path_is_symlink =
        fs::symlink_metadata(&request.path).is_ok_and(|metadata| metadata.file_type().is_symlink());

    let detected = detect::detect_repository(executable, &request.path)?;
    let info = inspect_worktree(
        executable,
        &detected.root,
        InspectOptions {
            include_dirty: true,
        },
    )?;
    let dirty = info.dirty.unwrap_or(false);
    let mut blockers = Vec::new();

    if info.is_primary {
        blockers.push(RemoveBlocker::PrimaryWorktree);
    }
    if let Err(GitError::PathOutsideManagedRoot { .. }) = ensure_within(&info.path, managed_root) {
        blockers.push(RemoveBlocker::OutsideManagedRoot);
    }
    if requested_path_is_symlink {
        blockers.push(RemoveBlocker::Symlink);
    }
    if info.locked {
        blockers.push(RemoveBlocker::Locked);
    }
    if request.session_is_active {
        blockers.push(RemoveBlocker::SessionActive);
    }
    if dirty && request.scope == RemoveScope::Directory {
        blockers.push(RemoveBlocker::Dirty);
    }

    Ok(RemoveInspection {
        fingerprint: RemovalFingerprint {
            path: info.path.clone(),
            head: info.head.clone(),
            dirty,
            locked: info.locked,
            scope: request.scope,
        },
        dirty,
        blockers,
        repository_root: detected.root,
        info,
    })
}

fn allocate(
    executable: &Path,
    managed_root: &Path,
    repository: &Path,
    request: &CreateWorktreeRequest,
) -> Result<AllocatedNames, GitError> {
    let mut taken_branches = detect::list_local_branches(executable, repository)?
        .into_iter()
        .collect::<BTreeSet<_>>();
    let project_dir = managed_root.join(request.project_id.to_string());
    let mut taken_directories = BTreeSet::new();
    if project_dir.is_dir() {
        for entry in fs::read_dir(&project_dir).map_err(|error| GitError::SpawnFailed {
            message: error.to_string(),
        })? {
            let entry = entry.map_err(|error| GitError::SpawnFailed {
                message: error.to_string(),
            })?;
            taken_directories.insert(entry.file_name().to_string_lossy().into_owned());
        }
    }

    if request.existing_branch == ExistingBranchBehavior::Reject {
        let slug = naming::slugify(&request.session_name);
        let branch = format!("agent/{slug}");
        if taken_branches
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&branch))
        {
            return Err(GitError::BranchExists { branch });
        }
    }

    for worktree in list_worktrees(executable, repository)? {
        if let Some(branch) = &worktree.branch {
            taken_branches.insert(branch.clone());
        }
    }

    naming::allocate_names(
        &request.session_name,
        request.session_id,
        &taken_branches,
        &taken_directories,
    )
}

fn confirm_created(
    executable: &Path,
    path: &Path,
    branch: &str,
    head: &str,
) -> Result<CreatedWorktree, GitError> {
    reject_symlink(path)?;
    let info = inspect_worktree(
        executable,
        path,
        InspectOptions {
            include_dirty: false,
        },
    )?;
    if info.branch.as_deref() != Some(branch) {
        return Err(GitError::InvalidOutput {
            reason: format!(
                "created worktree branch {:?} did not match {branch}",
                info.branch
            ),
        });
    }
    if info.head.as_deref() != Some(head) {
        return Err(GitError::InvalidOutput {
            reason: "created worktree HEAD did not match the requested base commit".to_owned(),
        });
    }
    Ok(CreatedWorktree {
        path: info.path,
        branch: branch.to_owned(),
        head: head.to_owned(),
    })
}

fn rollback(
    executable: &Path,
    repository: &Path,
    path: &Path,
    names: &AllocatedNames,
    source: GitError,
) -> GitError {
    let remove = GitCommand::new(executable)
        .repo(repository)
        .arg("worktree")
        .arg("remove")
        .arg(path)
        .run();
    if path.exists() {
        let _ = fs::remove_dir(path);
    }
    let branch = GitCommand::new(executable)
        .repo(repository)
        .arg("branch")
        .arg("-d")
        .arg(&names.branch)
        .run();

    match (remove, branch) {
        (Ok(remove), Ok(branch)) if remove.success() || !path.exists() => {
            let _ = branch;
            source
        }
        (Ok(remove), Ok(branch)) => GitError::RollbackFailed {
            source: Box::new(source),
            rollback: Box::new(GitError::CommandFailed {
                args: vec!["worktree".to_owned(), "remove".to_owned()],
                exit_code: remove.exit_code.or(branch.exit_code),
                stderr: format!(
                    "{}; {}",
                    String::from_utf8_lossy(&remove.stderr).trim(),
                    String::from_utf8_lossy(&branch.stderr).trim()
                ),
            }),
        },
        (Err(error), _) | (_, Err(error)) => GitError::RollbackFailed {
            source: Box::new(source),
            rollback: Box::new(error),
        },
    }
}

fn parse_worktree_list(bytes: &[u8], git_common_dir: &Path) -> Result<Vec<WorktreeInfo>, GitError> {
    let primary_root = git_common_dir
        .file_name()
        .is_some_and(|name| name == ".git")
        .then(|| git_common_dir.parent().map(Path::to_path_buf))
        .flatten();
    let mut worktrees = Vec::new();
    let mut current = WorktreeInfo {
        path: PathBuf::new(),
        head: None,
        branch: None,
        bare: false,
        detached: false,
        locked: false,
        lock_reason: None,
        prunable: false,
        prunable_reason: None,
        dirty: None,
        is_primary: false,
    };
    let mut started = false;

    let mut rest = bytes;
    while !rest.is_empty() {
        let Some(end) = rest.iter().position(|byte| *byte == 0) else {
            return Err(GitError::InvalidOutput {
                reason: "worktree list was not NUL-terminated".to_owned(),
            });
        };
        let record = &rest[..end];
        rest = &rest[end + 1..];
        if record.is_empty() {
            if started {
                current.is_primary = primary_root
                    .as_ref()
                    .is_some_and(|root| paths_equal(&current.path, root));
                worktrees.push(std::mem::replace(
                    &mut current,
                    WorktreeInfo {
                        path: PathBuf::new(),
                        head: None,
                        branch: None,
                        bare: false,
                        detached: false,
                        locked: false,
                        lock_reason: None,
                        prunable: false,
                        prunable_reason: None,
                        dirty: None,
                        is_primary: false,
                    },
                ));
                started = false;
            }
            continue;
        }
        let line = std::str::from_utf8(record).map_err(|_| GitError::InvalidOutput {
            reason: "worktree list record was not UTF-8".to_owned(),
        })?;
        started = true;
        if let Some(path) = line.strip_prefix("worktree ") {
            current.path = real_or_absolute(Path::new(path))?;
        } else if let Some(head) = line.strip_prefix("HEAD ") {
            current.head = Some(head.to_owned());
        } else if let Some(branch) = line.strip_prefix("branch ") {
            current.branch = Some(
                branch
                    .strip_prefix("refs/heads/")
                    .unwrap_or(branch)
                    .to_owned(),
            );
        } else if line == "bare" {
            current.bare = true;
        } else if line == "detached" {
            current.detached = true;
        } else if line == "locked" {
            current.locked = true;
        } else if let Some(reason) = line.strip_prefix("locked ") {
            current.locked = true;
            current.lock_reason = Some(reason.to_owned());
        } else if line == "prunable" {
            current.prunable = true;
        } else if let Some(reason) = line.strip_prefix("prunable ") {
            current.prunable = true;
            current.prunable_reason = Some(reason.to_owned());
        }
    }
    if started {
        current.is_primary = primary_root
            .as_ref()
            .is_some_and(|root| paths_equal(&current.path, root));
        worktrees.push(current);
    }
    Ok(worktrees)
}

fn blocker_error(
    blocker: RemoveBlocker,
    info: &WorktreeInfo,
    requested_path: &Path,
    managed_root: &Path,
) -> GitError {
    match blocker {
        RemoveBlocker::Dirty => GitError::WorktreeDirty {
            path: info.path.clone(),
        },
        RemoveBlocker::SessionActive => GitError::WorktreeInUse {
            path: info.path.clone(),
        },
        RemoveBlocker::Locked => GitError::WorktreeLocked {
            path: info.path.clone(),
            reason: info.lock_reason.clone(),
        },
        RemoveBlocker::PrimaryWorktree => GitError::PrimaryWorktree {
            path: info.path.clone(),
        },
        RemoveBlocker::OutsideManagedRoot => GitError::PathOutsideManagedRoot {
            path: info.path.clone(),
            root: managed_root.to_path_buf(),
        },
        RemoveBlocker::Symlink => GitError::SymlinkRejected {
            path: requested_path.to_path_buf(),
        },
    }
}

fn instant_to_epoch_ms(deadline: Instant) -> i64 {
    let remaining = deadline.saturating_duration_since(Instant::now());
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from((now + remaining).as_millis()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{RemoveBlocker, WorktreeInfo, blocker_error};
    use crate::error::GitError;

    #[test]
    fn outside_managed_root_error_keeps_the_managed_root() {
        let worktree_path = PathBuf::from("/outside/worktree");
        let managed_root = Path::new("/managed/worktrees");
        let info = WorktreeInfo {
            path: worktree_path.clone(),
            head: None,
            branch: None,
            bare: false,
            detached: false,
            locked: false,
            lock_reason: None,
            prunable: false,
            prunable_reason: None,
            dirty: Some(false),
            is_primary: false,
        };

        let error = blocker_error(
            RemoveBlocker::OutsideManagedRoot,
            &info,
            &worktree_path,
            managed_root,
        );

        assert!(matches!(
            &error,
            GitError::PathOutsideManagedRoot { path, root }
                if path == &worktree_path && root == managed_root
        ));
        let api_error = error.to_api_error();
        assert_eq!(
            api_error
                .details
                .get("root")
                .and_then(|value| value.as_str()),
            managed_root.to_str()
        );
    }
}
