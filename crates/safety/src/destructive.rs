use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::path::PathBuf;
use std::time::Instant;

use cli_master_core::{
    AgentId, ApplicationError, CONFIRMATION_TTL, ErrorCode, ProjectId, SessionId, WorktreeId,
};
use uuid::Uuid;

use crate::paths::{ManagedRoots, assert_managed_worktree, resolve_path};

const MAX_PENDING_CONFIRMATIONS: usize = 128;

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
    /// Custom agent definition being removed.
    pub agent_id: Option<AgentId>,
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
#[derive(Clone, Eq, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
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
    /// Whether this plan authorizes escalation to `SIGKILL`.
    pub force: bool,
}

impl fmt::Debug for RemovalPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RemovalPlan")
            .field("kind", &self.kind)
            .field("path", &self.path)
            .field("branch", &self.branch)
            .field("dirty", &self.dirty)
            .field("in_use", &self.in_use)
            .field("token", &"[redacted]")
            .field("requires_allow_dirty", &self.requires_allow_dirty)
            .field("force", &self.force)
            .finish()
    }
}

/// Git-facing worktree removal decision.
#[derive(Debug, Eq, PartialEq)]
pub struct WorktreeRemovalState {
    /// Canonical worktree path.
    path: PathBuf,
    /// Checked-out branch.
    branch: String,
    /// Whether Git reports a dirty tree.
    dirty: bool,
    /// Whether a live session still uses it.
    in_use: bool,
    /// Whether `git worktree remove --force` is permitted.
    allow_force: bool,
    identity: FileIdentity,
}

/// A destructive operation whose target and observed state were confirmed.
#[derive(Debug, Eq, PartialEq)]
pub enum ConfirmedDestructiveOperation {
    /// Project metadata may be unregistered. Repository files remain untouched.
    RemoveProject {
        /// Confirmed project identifier.
        project_id: ProjectId,
    },
    /// Session metadata may be deleted after its process stopped.
    DeleteSession {
        /// Confirmed session identifier.
        session_id: SessionId,
    },
    /// A process owned by this session may be stopped.
    StopProcess {
        /// Confirmed session identifier.
        session_id: SessionId,
        /// Whether the user explicitly confirmed escalation to SIGKILL.
        force: bool,
    },
    /// A Git worktree may be removed under the contained state.
    RemoveWorktree(WorktreeRemovalState),
    /// A custom agent definition may be deleted.
    DeleteCustomAgent {
        /// Confirmed agent identifier.
        agent_id: AgentId,
    },
}

impl ConfirmedDestructiveOperation {
    /// Extracts a confirmed worktree removal.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorCode::ConfirmationMismatch`] for a different operation.
    pub fn into_worktree_removal(self) -> Result<WorktreeRemovalState, ApplicationError> {
        match self {
            Self::RemoveWorktree(state) => Ok(state),
            _ => Err(ApplicationError::new(
                ErrorCode::ConfirmationMismatch,
                "This confirmation does not authorize worktree removal.",
            )
            .not_recoverable()
            .with_action("Use the confirmation handler for the requested operation.")),
        }
    }
}

impl WorktreeRemovalState {
    /// Returns the canonical worktree path bound to the confirmation.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the branch observed when the confirmation was issued.
    #[must_use]
    pub fn branch(&self) -> &str {
        &self.branch
    }

    /// Returns whether the confirmed worktree was dirty.
    #[must_use]
    pub const fn dirty(&self) -> bool {
        self.dirty
    }

    /// Returns whether the confirmed worktree was in use.
    #[must_use]
    pub const fn in_use(&self) -> bool {
        self.in_use
    }

    /// Returns whether force removal was explicitly confirmed for a dirty tree.
    #[must_use]
    pub const fn allows_force(&self) -> bool {
        self.allow_force
    }

    /// Rechecks that `path` still names the confirmed filesystem object.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorCode::ConfirmationMismatch`] if the path was replaced,
    /// moved, or changed into a symbolic link after confirmation.
    pub fn revalidate_path(&self, path: &Path) -> Result<(), ApplicationError> {
        let resolved = resolve_path(path)?;
        let identity = file_identity(&resolved.path)?;
        if resolved.last_component_symlink
            || resolved.path != self.path
            || identity != self.identity
        {
            return Err(confirmation_changed());
        }
        Ok(())
    }
}

/// In-memory confirmation tokens tied to an observed fingerprint.
#[derive(Default)]
pub struct ConfirmationStore {
    pending: HashMap<String, PendingConfirmation>,
}

struct PendingConfirmation {
    binding: ConfirmationBinding,
    expires_at: Instant,
}

impl fmt::Debug for ConfirmationStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConfirmationStore")
            .field("pending_count", &self.pending.len())
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ConfirmationBinding {
    kind: DestructiveKind,
    path: Option<PathBuf>,
    path_identity: Option<FileIdentity>,
    branch: Option<String>,
    session_id: Option<SessionId>,
    project_id: Option<ProjectId>,
    worktree_id: Option<WorktreeId>,
    agent_id: Option<AgentId>,
    dirty: bool,
    in_use: bool,
    force: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FileIdentity {
    device: u64,
    inode: u64,
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
        validate_request(request, roots, ConfirmationPhase::Prepare)?;
        self.evict_oldest_if_full();
        let binding = confirmation_binding(request, roots)?;
        let token = Uuid::now_v7().to_string();
        self.pending.insert(
            token.clone(),
            PendingConfirmation {
                binding: binding.clone(),
                expires_at: Instant::now() + CONFIRMATION_TTL,
            },
        );
        Ok(RemovalPlan {
            kind: request.kind,
            path: binding.path,
            branch: request.branch.clone(),
            dirty: request.dirty,
            in_use: request.in_use,
            token,
            requires_allow_dirty: request.kind == DestructiveKind::RemoveWorktree && request.dirty,
            force: request.force,
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
    ) -> Result<ConfirmedDestructiveOperation, ApplicationError> {
        self.expire();
        let pending = self.pending.remove(token).ok_or_else(|| {
            ApplicationError::new(
                ErrorCode::ConfirmationMismatch,
                "The confirmation expired or does not match this operation.",
            )
            .with_action("Review the path and branch, then confirm again.")
        })?;
        validate_request(request, roots, ConfirmationPhase::Confirm)?;
        let binding = confirmation_binding(request, roots)?;
        if pending.binding != binding {
            return Err(confirmation_changed());
        }
        Ok(confirmed_operation(request, &binding))
    }
}

#[derive(Clone, Copy)]
enum ConfirmationPhase {
    Prepare,
    Confirm,
}

fn validate_request(
    request: &DestructiveRequest,
    roots: &ManagedRoots,
    phase: ConfirmationPhase,
) -> Result<(), ApplicationError> {
    if matches!(phase, ConfirmationPhase::Prepare) && request.allow_dirty {
        return Err(ApplicationError::new(
            ErrorCode::ConfirmationMismatch,
            "Dirty-worktree approval cannot be set before the confirmation step.",
        )
        .not_recoverable()
        .with_action("Prepare the operation, review the target, then confirm it."));
    }
    if request.allow_dirty && request.kind != DestructiveKind::RemoveWorktree {
        return Err(invalid_confirmation_flag("allowDirty"));
    }
    if request.force && request.kind != DestructiveKind::StopProcess {
        return Err(invalid_confirmation_flag("force"));
    }
    match request.kind {
        DestructiveKind::RemoveProject => {
            require_target(request.project_id.as_ref(), "projectId")?;
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
            require_target(request.session_id.as_ref(), "sessionId")?;
            if request.in_use {
                return Err(in_use_error(
                    ErrorCode::SessionInUse,
                    "The session process is still running.",
                    "Stop the session before deleting its metadata.",
                ));
            }
            Ok(())
        }
        DestructiveKind::StopProcess => {
            require_target(request.session_id.as_ref(), "sessionId")?;
            Ok(())
        }
        DestructiveKind::RemoveWorktree => validate_worktree(request, roots, phase),
        DestructiveKind::DeleteCustomAgent => {
            require_target(request.agent_id.as_ref(), "agentId")?;
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
    phase: ConfirmationPhase,
) -> Result<(), ApplicationError> {
    require_target(request.project_id.as_ref(), "projectId")?;
    require_target(request.worktree_id.as_ref(), "worktreeId")?;
    let path = request.path.as_ref().ok_or_else(|| {
        ApplicationError::new(ErrorCode::InvalidPath, "Worktree removal requires a path.")
            .with_action("Select a managed worktree.")
    })?;
    let resolved = assert_managed_worktree(path, roots)?;
    let metadata = fs::metadata(&resolved).map_err(|error| {
        ApplicationError::new(
            ErrorCode::InvalidPath,
            "The confirmed worktree path no longer exists.",
        )
        .not_recoverable()
        .with_action("Refresh worktrees and prepare the removal again.")
        .with_context("path", resolved.display().to_string())
        .with_source(&error)
    })?;
    if !metadata.is_dir() {
        return Err(ApplicationError::new(
            ErrorCode::InvalidPath,
            "Worktree removal requires a directory.",
        )
        .not_recoverable()
        .with_action("Refresh worktrees and select the worktree directory."));
    }
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
    if matches!(phase, ConfirmationPhase::Confirm) && request.dirty && !request.allow_dirty {
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

fn confirmed_operation(
    request: &DestructiveRequest,
    binding: &ConfirmationBinding,
) -> ConfirmedDestructiveOperation {
    match request.kind {
        DestructiveKind::RemoveProject => ConfirmedDestructiveOperation::RemoveProject {
            project_id: request.project_id.expect("project id is validated"),
        },
        DestructiveKind::DeleteSession => ConfirmedDestructiveOperation::DeleteSession {
            session_id: request.session_id.expect("session id is validated"),
        },
        DestructiveKind::StopProcess => ConfirmedDestructiveOperation::StopProcess {
            session_id: request.session_id.expect("session id is validated"),
            force: request.force,
        },
        DestructiveKind::RemoveWorktree => {
            let path = binding.path.clone().expect("worktree path is validated");
            let identity = binding
                .path_identity
                .expect("existing worktree identity is validated");
            ConfirmedDestructiveOperation::RemoveWorktree(WorktreeRemovalState {
                path,
                branch: request.branch.clone().unwrap_or_default(),
                dirty: request.dirty,
                in_use: request.in_use,
                allow_force: request.dirty && request.allow_dirty,
                identity,
            })
        }
        DestructiveKind::DeleteCustomAgent => ConfirmedDestructiveOperation::DeleteCustomAgent {
            agent_id: request.agent_id.expect("agent id is validated"),
        },
    }
}

fn confirmation_binding(
    request: &DestructiveRequest,
    roots: &ManagedRoots,
) -> Result<ConfirmationBinding, ApplicationError> {
    let path = match (&request.path, request.kind) {
        (Some(path), DestructiveKind::RemoveWorktree) => {
            Some(assert_managed_worktree(path, roots)?)
        }
        (Some(path), _) => Some(resolve_path(path)?.path),
        (None, _) => None,
    };
    let path_identity = path.as_deref().map(file_identity).transpose()?;
    Ok(ConfirmationBinding {
        kind: request.kind,
        path,
        path_identity,
        branch: request.branch.clone(),
        session_id: request.session_id,
        project_id: request.project_id,
        worktree_id: request.worktree_id,
        agent_id: request.agent_id,
        dirty: request.dirty,
        in_use: request.in_use,
        force: request.force,
    })
}

fn file_identity(path: &Path) -> Result<FileIdentity, ApplicationError> {
    let metadata = fs::metadata(path).map_err(|error| {
        ApplicationError::new(
            ErrorCode::InvalidPath,
            "The destructive target no longer exists.",
        )
        .not_recoverable()
        .with_action("Refresh the resource and prepare the operation again.")
        .with_context("path", path.display().to_string())
        .with_source(&error)
    })?;
    Ok(FileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

fn require_target<T>(value: Option<&T>, field: &str) -> Result<(), ApplicationError> {
    if value.is_some() {
        return Ok(());
    }
    Err(ApplicationError::new(
        ErrorCode::InvalidIpcPayload,
        format!("Destructive operation requires {field}."),
    )
    .not_recoverable()
    .with_action("Refresh the resource and prepare the operation again."))
}

fn confirmation_changed() -> ApplicationError {
    ApplicationError::new(
        ErrorCode::ConfirmationMismatch,
        "The resource changed after confirmation was issued.",
    )
    .with_action("Review the current target and state, then confirm again.")
}

fn invalid_confirmation_flag(flag: &str) -> ApplicationError {
    ApplicationError::new(
        ErrorCode::InvalidIpcPayload,
        format!("{flag} is not valid for this destructive operation."),
    )
    .not_recoverable()
    .with_action("Refresh the resource and prepare the operation again.")
}

fn in_use_error(code: ErrorCode, message: &str, action: &str) -> ApplicationError {
    ApplicationError::new(code, message.to_owned()).with_action(action.to_owned())
}

impl ConfirmationStore {
    fn expire(&mut self) {
        let now = Instant::now();
        self.pending.retain(|_, pending| pending.expires_at > now);
    }

    fn evict_oldest_if_full(&mut self) {
        if self.pending.len() < MAX_PENDING_CONFIRMATIONS {
            return;
        }
        let oldest = self
            .pending
            .iter()
            .min_by_key(|(_, pending)| pending.expires_at)
            .map(|(token, _)| token.clone());
        if let Some(token) = oldest {
            self.pending.remove(&token);
        }
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
            project_id: Some(ProjectId::new()),
            worktree_id: Some(WorktreeId::new()),
            agent_id: None,
            dirty,
            in_use: false,
            allow_dirty,
            force: false,
        }
    }

    #[test]
    fn dirty_worktree_plan_requires_explicit_allow_dirty_at_confirm() {
        let temp = TempDir::new().expect("temp");
        let data = temp.path().join("data");
        let worktree = data.join("worktrees/project/topic");
        fs::create_dir_all(&worktree).expect("worktree");
        let roots = ManagedRoots::new(&data);
        let mut store = ConfirmationStore::new();
        let mut request = worktree_request(worktree, true, false);
        let plan = store.prepare(&request, &roots).expect("prepare dirty plan");
        assert!(plan.requires_allow_dirty);
        let error = store
            .confirm(&plan.token, &request, &roots)
            .expect_err("dirty confirmation requires consent");
        assert_eq!(error.code(), ErrorCode::WorktreeDirty);

        request.allow_dirty = false;
        let plan = store.prepare(&request, &roots).expect("prepare again");
        request.allow_dirty = true;
        let state = store
            .confirm(&plan.token, &request, &roots)
            .expect("explicit dirty consent")
            .into_worktree_removal()
            .expect("worktree confirmation");
        assert!(state.allows_force());
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
            .expect("confirm")
            .into_worktree_removal()
            .expect("worktree confirmation");
        assert!(!state.allows_force());
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
            project_id: Some(ProjectId::new()),
            worktree_id: None,
            agent_id: None,
            dirty: false,
            in_use: false,
            allow_dirty: false,
            force: false,
        };
        let plan = store.prepare(&request, &roots).expect("metadata only");
        assert_eq!(plan.kind, DestructiveKind::RemoveProject);
        assert!(project.exists());
    }

    #[test]
    fn token_is_bound_to_worktree_and_project_ids() {
        let temp = TempDir::new().expect("temp");
        let data = temp.path().join("data");
        let worktree = data.join("worktrees/project/topic");
        fs::create_dir_all(&worktree).expect("worktree");
        let roots = ManagedRoots::new(&data);
        let mut store = ConfirmationStore::new();
        let request = worktree_request(worktree, false, false);
        let plan = store.prepare(&request, &roots).expect("prepare");
        let mut switched = request.clone();
        switched.worktree_id = Some(WorktreeId::new());

        let error = store
            .confirm(&plan.token, &switched, &roots)
            .expect_err("token must not switch worktrees");
        assert_eq!(error.code(), ErrorCode::ConfirmationMismatch);
    }

    #[test]
    fn token_detects_replacement_at_the_same_path() {
        let temp = TempDir::new().expect("temp");
        let data = temp.path().join("data");
        let worktree = data.join("worktrees/project/topic");
        fs::create_dir_all(&worktree).expect("worktree");
        let roots = ManagedRoots::new(&data);
        let mut store = ConfirmationStore::new();
        let request = worktree_request(worktree.clone(), false, false);
        let plan = store.prepare(&request, &roots).expect("prepare");
        fs::remove_dir(&worktree).expect("remove original");
        fs::create_dir(&worktree).expect("replace target");

        let error = store
            .confirm(&plan.token, &request, &roots)
            .expect_err("replacement must invalidate confirmation");
        assert_eq!(error.code(), ErrorCode::ConfirmationMismatch);
    }

    #[test]
    fn confirmed_state_revalidates_identity_before_mutation() {
        let temp = TempDir::new().expect("temp");
        let data = temp.path().join("data");
        let worktree = data.join("worktrees/project/topic");
        fs::create_dir_all(&worktree).expect("worktree");
        let roots = ManagedRoots::new(&data);
        let mut store = ConfirmationStore::new();
        let request = worktree_request(worktree.clone(), false, false);
        let plan = store.prepare(&request, &roots).expect("prepare");
        let state = store
            .confirm(&plan.token, &request, &roots)
            .expect("confirm")
            .into_worktree_removal()
            .expect("worktree state");
        fs::remove_dir(&worktree).expect("remove original");
        fs::create_dir(&worktree).expect("replace target");

        let error = state
            .revalidate_path(&worktree)
            .expect_err("confirmed inode was replaced");
        assert_eq!(error.code(), ErrorCode::ConfirmationMismatch);
    }

    #[test]
    fn debug_output_does_not_expose_confirmation_tokens() {
        let temp = TempDir::new().expect("temp");
        let data = temp.path().join("data");
        let worktree = data.join("worktrees/project/topic");
        fs::create_dir_all(&worktree).expect("worktree");
        let roots = ManagedRoots::new(&data);
        let mut store = ConfirmationStore::new();
        let request = worktree_request(worktree, false, false);
        let plan = store.prepare(&request, &roots).expect("prepare");

        assert!(!format!("{store:?}").contains(&plan.token));
        assert!(!format!("{plan:?}").contains(&plan.token));
    }

    #[test]
    fn stop_token_cannot_be_elevated_to_force() {
        let temp = TempDir::new().expect("temp");
        let roots = ManagedRoots::new(temp.path());
        let mut store = ConfirmationStore::new();
        let request = DestructiveRequest {
            kind: DestructiveKind::StopProcess,
            path: None,
            branch: None,
            session_id: Some(SessionId::new()),
            project_id: None,
            worktree_id: None,
            agent_id: None,
            dirty: false,
            in_use: true,
            allow_dirty: false,
            force: false,
        };
        let plan = store.prepare(&request, &roots).expect("graceful plan");
        assert!(!plan.force);
        let mut elevated = request.clone();
        elevated.force = true;

        let error = store
            .confirm(&plan.token, &elevated, &roots)
            .expect_err("force changes the confirmation binding");
        assert_eq!(error.code(), ErrorCode::ConfirmationMismatch);

        let forced_plan = store.prepare(&elevated, &roots).expect("forced plan");
        assert!(forced_plan.force);
        let confirmed = store
            .confirm(&forced_plan.token, &elevated, &roots)
            .expect("forced confirmation");
        assert!(matches!(
            confirmed,
            ConfirmedDestructiveOperation::StopProcess { force: true, .. }
        ));
    }

    #[test]
    fn pending_confirmation_store_is_bounded() {
        let temp = TempDir::new().expect("temp");
        let roots = ManagedRoots::new(temp.path());
        let mut store = ConfirmationStore::new();
        let request = DestructiveRequest {
            kind: DestructiveKind::DeleteSession,
            path: None,
            branch: None,
            session_id: Some(SessionId::new()),
            project_id: None,
            worktree_id: None,
            agent_id: None,
            dirty: false,
            in_use: false,
            allow_dirty: false,
            force: false,
        };
        let first = store.prepare(&request, &roots).expect("first plan");
        for _ in 1..=MAX_PENDING_CONFIRMATIONS {
            store.prepare(&request, &roots).expect("bounded plan");
        }

        assert_eq!(store.pending.len(), MAX_PENDING_CONFIRMATIONS);
        let error = store
            .confirm(&first.token, &request, &roots)
            .expect_err("oldest token is evicted");
        assert_eq!(error.code(), ErrorCode::ConfirmationMismatch);
    }
}
