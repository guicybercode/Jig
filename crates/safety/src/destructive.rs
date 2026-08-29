use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use cli_master_core::{
    ApplicationError, CONFIRMATION_TTL, ErrorCode, ProjectId, SessionId, WorktreeId,
};
use uuid::Uuid;

use crate::paths::{ManagedRoots, assert_managed_worktree, resolve_path};

/// Kind of irreversible or process-stopping operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DestructiveKind {
    /// Unregister a project. The repository directory is never deleted.
    RemoveProject,
    /// Delete session metadata after the process has stopped.
    DeleteSession,
    /// Stop a live process group.
    StopProcess,
    /// Remove a managed Git worktree directory.
    RemoveWorktree,
    /// Delete a custom agent definition.
    DeleteCustomAgent,
}

/// Caller-supplied facts for a destructive operation.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct DestructiveRequest {
    /// Operation to perform.
    pub kind: DestructiveKind,
    /// Target filesystem path, when relevant.
    pub path: Option<PathBuf>,
    /// Branch name shown in the confirmation dialog.
    pub branch: Option<String>,
    /// Session that currently owns a worktree or process.
    pub session_id: Option<SessionId>,
    /// Project being unregistered.
    pub project_id: Option<ProjectId>,
    /// Worktree being removed.
    pub worktree_id: Option<WorktreeId>,
    /// Whether Git reports uncommitted changes.
    pub dirty: bool,
    /// Whether a live session still uses the resource.
    pub in_use: bool,
    /// Whether the caller explicitly confirmed dirty removal.
    pub allow_dirty: bool,
    /// Whether the caller explicitly confirmed SIGKILL.
    pub force: bool,
}

/// User-visible plan returned by a prepare step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemovalPlan {
    /// Operation that will run if confirmed.
    pub kind: DestructiveKind,
    /// Path shown to the user.
    pub path: Option<PathBuf>,
    /// Branch shown to the user.
    pub branch: Option<String>,
    /// Whether the worktree currently has changes.
    pub dirty: bool,
    /// Whether a session is still using the resource.
    pub in_use: bool,
    /// Short-lived token required by the confirm step.
    pub token: String,
    /// Whether `allowDirty` must be set to continue.
    pub requires_allow_dirty: bool,
}

/// Git-facing worktree removal decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorktreeRemovalState {
    /// Canonical worktree path.
    pub path: PathBuf,
    /// Checked-out branch.
    pub branch: String,
    /// Whether Git reports a dirty tree.
    pub dirty: bool,
    /// Whether a live session still uses it.
    pub in_use: bool,
    /// Whether `git worktree remove --force` is permitted.
    pub allow_force: bool,
}

/// In-memory confirmation tokens tied to an observed fingerprint.
#[derive(Debug, Default)]
pub struct ConfirmationStore {
    pending: HashMap<String, PendingConfirmation>,
}

#[derive(Debug)]
struct PendingConfirmation {
    kind: DestructiveKind,
    fingerprint: String,
    expires_at: Instant,
}

impl ConfirmationStore {
    /// Creates an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Validates a destructive request and issues a confirmation token.
    ///
    /// # Errors
    ///
    /// Returns an error when the operation is currently unsafe or the path is
    /// not managed.
    pub fn prepare(
        &mut self,
        request: &DestructiveRequest,
        roots: &ManagedRoots,
    ) -> Result<RemovalPlan, ApplicationError> {
        self.expire();
        validate_request(request, roots)?;
        let token = Uuid::now_v7().to_string();
        self.pending.insert(
            token.clone(),
            PendingConfirmation {
                kind: request.kind,
                fingerprint: fingerprint(request),
                expires_at: Instant::now() + CONFIRMATION_TTL,
            },
        );
        Ok(RemovalPlan {
            kind: request.kind,
            path: request.path.clone(),
            branch: request.branch.clone(),
            dirty: request.dirty,
            in_use: request.in_use,
            token,
            requires_allow_dirty: request.kind == DestructiveKind::RemoveWorktree && request.dirty,
        })
    }

    /// Rechecks the observed state and consumes the token.
    ///
    /// # Errors
    ///
    /// Returns an error when the token is missing, expired, or the resource
    /// changed since prepare.
    pub fn confirm(
        &mut self,
        token: &str,
        request: &DestructiveRequest,
        roots: &ManagedRoots,
    ) -> Result<WorktreeRemovalState, ApplicationError> {
        self.expire();
        let pending = self.pending.remove(token).ok_or_else(|| {
            ApplicationError::new(
                ErrorCode::ConfirmationMismatch,
                "The confirmation expired or does not match this operation.",
            )
            .with_action("Review the path and branch, then confirm again.")
        })?;
        if pending.kind != request.kind || pending.fingerprint != fingerprint(request) {
            return Err(ApplicationError::new(
                ErrorCode::ConfirmationMismatch,
                "The resource changed after confirmation was issued.",
            )
            .with_action("Review the current path, branch, and dirty state, then confirm again."));
        }
        validate_request(request, roots)?;
        worktree_state(request, roots)
    }
}

fn validate_request(
    request: &DestructiveRequest,
    roots: &ManagedRoots,
) -> Result<(), ApplicationError> {
    match request.kind {
        DestructiveKind::RemoveProject => {
            if request.in_use {
                return Err(in_use_error(
                    ErrorCode::ProjectInUse,
                    "This project still has sessions or worktrees.",
                    "Stop sessions and remove worktrees before unregistering the project.",
                ));
            }
            Ok(())
        }
        DestructiveKind::DeleteSession => {
            if request.in_use {
                return Err(in_use_error(
                    ErrorCode::SessionInUse,
                    "The session process is still running.",
                    "Stop the session before deleting its metadata.",
                ));
            }
            Ok(())
        }
        DestructiveKind::StopProcess => Ok(()),
        DestructiveKind::RemoveWorktree => validate_worktree(request, roots),
        DestructiveKind::DeleteCustomAgent => {
            if request.in_use {
                return Err(in_use_error(
                    ErrorCode::AgentInUse,
                    "This agent is still referenced by a session.",
                    "Keep the definition disabled, or delete sessions that reference it first.",
                ));
            }
            Ok(())
        }
    }
}

fn validate_worktree(
    request: &DestructiveRequest,
    roots: &ManagedRoots,
) -> Result<(), ApplicationError> {
    let path = request.path.as_ref().ok_or_else(|| {
        ApplicationError::new(ErrorCode::InvalidPath, "Worktree removal requires a path.")
            .with_action("Select a managed worktree.")
    })?;
    assert_managed_worktree(path, roots)?;
    if request.in_use {
        return Err(ApplicationError::new(
            ErrorCode::WorktreeInUse,
            format!(
                "Worktree {} on branch {} is used by a running session.",
                path.display(),
                request.branch.as_deref().unwrap_or("(unknown)")
            ),
        )
        .with_action("Stop the session before removing the worktree.")
        .with_context("path", path.display().to_string())
        .with_context("branch", request.branch.clone().unwrap_or_default()));
    }
    if request.dirty && !request.allow_dirty {
        return Err(ApplicationError::new(
            ErrorCode::WorktreeDirty,
            format!(
                "Worktree {} on branch {} has uncommitted changes.",
                path.display(),
                request.branch.as_deref().unwrap_or("(unknown)")
            ),
        )
        .with_action("Commit or move the changes, or confirm dirty removal explicitly.")
        .with_context("path", path.display().to_string())
        .with_context("branch", request.branch.clone().unwrap_or_default())
        .with_context("dirty", true));
    }
    Ok(())
}

fn worktree_state(
    request: &DestructiveRequest,
    _roots: &ManagedRoots,
) -> Result<WorktreeRemovalState, ApplicationError> {
    if request.kind != DestructiveKind::RemoveWorktree {
        return Ok(WorktreeRemovalState {
            path: request.path.clone().unwrap_or_else(|| PathBuf::from(".")),
            branch: request.branch.clone().unwrap_or_default(),
            dirty: request.dirty,
            in_use: request.in_use,
            allow_force: false,
        });
    }
    let path = request.path.as_ref().expect("validated");
    Ok(WorktreeRemovalState {
        path: resolve_path(path)?.path,
        branch: request.branch.clone().unwrap_or_default(),
        dirty: request.dirty,
        in_use: request.in_use,
        allow_force: request.allow_dirty,
    })
}

fn fingerprint(request: &DestructiveRequest) -> String {
    format!(
        "{:?}|{:?}|{:?}|{:?}|{}|{}",
        request.kind,
        request.path.as_ref().map(|path| path.display().to_string()),
        request.branch,
        request.session_id.map(|id| id.to_string()),
        request.dirty,
        request.in_use
    )
}

fn in_use_error(code: ErrorCode, message: &str, action: &str) -> ApplicationError {
    ApplicationError::new(code, message.to_owned()).with_action(action.to_owned())
}

impl ConfirmationStore {
    fn expire(&mut self) {
        let now = Instant::now();
        self.pending.retain(|_, pending| pending.expires_at > now);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;
    use crate::paths::ManagedRoots;

    fn worktree_request(path: PathBuf, dirty: bool, allow_dirty: bool) -> DestructiveRequest {
        DestructiveRequest {
            kind: DestructiveKind::RemoveWorktree,
            path: Some(path),
            branch: Some("agent/topic".to_owned()),
            session_id: None,
            project_id: None,
            worktree_id: None,
            dirty,
            in_use: false,
            allow_dirty,
            force: false,
        }
    }

    #[test]
    fn dirty_worktree_requires_explicit_allow_dirty() {
        let temp = TempDir::new().expect("temp");
        let data = temp.path().join("data");
        let worktree = data.join("worktrees/project/topic");
        fs::create_dir_all(&worktree).expect("worktree");
        let roots = ManagedRoots::new(&data);
        let mut store = ConfirmationStore::new();
        let request = worktree_request(worktree, true, false);
        let error = store.prepare(&request, &roots).expect_err("dirty");
        assert_eq!(error.code(), ErrorCode::WorktreeDirty);
        assert!(error.suggested_action().is_some());
    }

    #[test]
    fn in_use_worktree_cannot_be_removed() {
        let temp = TempDir::new().expect("temp");
        let data = temp.path().join("data");
        let worktree = data.join("worktrees/project/topic");
        fs::create_dir_all(&worktree).expect("worktree");
        let roots = ManagedRoots::new(&data);
        let mut store = ConfirmationStore::new();
        let mut request = worktree_request(worktree, false, false);
        request.in_use = true;
        let error = store.prepare(&request, &roots).expect_err("in use");
        assert_eq!(error.code(), ErrorCode::WorktreeInUse);
    }

    #[test]
    fn unmanaged_path_cannot_be_deleted() {
        let temp = TempDir::new().expect("temp");
        let data = temp.path().join("data");
        fs::create_dir_all(data.join("worktrees")).expect("root");
        let unmanaged = temp.path().join("not-managed");
        fs::create_dir_all(&unmanaged).expect("unmanaged");
        let roots = ManagedRoots::new(&data);
        let mut store = ConfirmationStore::new();
        let request = worktree_request(unmanaged, false, false);
        let error = store.prepare(&request, &roots).expect_err("unmanaged");
        assert_eq!(error.code(), ErrorCode::UnmanagedPath);
    }

    #[test]
    fn clean_managed_worktree_issues_and_consumes_a_token() {
        let temp = TempDir::new().expect("temp");
        let data = temp.path().join("data");
        let worktree = data.join("worktrees/project/topic");
        fs::create_dir_all(&worktree).expect("worktree");
        let roots = ManagedRoots::new(&data);
        let mut store = ConfirmationStore::new();
        let request = worktree_request(worktree, false, false);
        let plan = store.prepare(&request, &roots).expect("prepare");
        assert_eq!(plan.branch.as_deref(), Some("agent/topic"));
        assert!(!plan.requires_allow_dirty);
        let state = store
            .confirm(&plan.token, &request, &roots)
            .expect("confirm");
        assert!(!state.allow_force);
        store
            .confirm(&plan.token, &request, &roots)
            .expect_err("token is single use");
    }

    #[test]
    fn project_remove_never_deletes_the_repository_directory() {
        let temp = TempDir::new().expect("temp");
        let project = temp.path().join("repo");
        fs::create_dir_all(&project).expect("repo");
        let roots = ManagedRoots::new(temp.path().join("data")).with_project_root(&project);
        let mut store = ConfirmationStore::new();
        let request = DestructiveRequest {
            kind: DestructiveKind::RemoveProject,
            path: Some(project.clone()),
            branch: None,
            session_id: None,
            project_id: None,
            worktree_id: None,
            dirty: false,
            in_use: false,
            allow_dirty: false,
            force: false,
        };
        let plan = store.prepare(&request, &roots).expect("metadata only");
        assert_eq!(plan.kind, DestructiveKind::RemoveProject);
        assert!(project.exists());
    }
}
