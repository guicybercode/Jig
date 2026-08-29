use std::path::Path;

use cli_master_core::{AgentId, CommandSpec, ProjectId, SessionId, WorktreeId};

use crate::{SagaError, SagaErrorKind, SessionManager, TerminalSize};

/// Result of spawning an agent process for a prepared session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpawnedSession {
    /// Operating-system process identifier of the child.
    pub pid: u32,
    /// Daemon-local PTY handle, when a PTY was attached.
    pub pty_id: Option<String>,
}

/// Inputs required to launch an agent against a prepared working directory.
#[derive(Clone, Debug)]
pub struct SpawnRequest<'a> {
    /// Session whose runtime state will be updated after spawn.
    pub session_id: SessionId,
    /// Project that owns the durable and in-memory session records.
    pub project_id: ProjectId,
    /// Agent definition used to build `command`.
    pub agent_id: AgentId,
    /// Canonical user-facing session name.
    pub name: &'a str,
    /// Structured launch command. Never a shell string.
    pub command: &'a CommandSpec,
    /// Managed branch, when worktree isolation is enabled.
    pub branch: Option<&'a str>,
    /// Managed worktree identifier, when isolation is enabled.
    pub worktree_id: Option<WorktreeId>,
    /// Managed worktree path, when isolation is enabled.
    pub worktree_path: Option<&'a Path>,
    /// Initial PTY columns.
    pub cols: u16,
    /// Initial PTY rows.
    pub rows: u16,
}

/// Process launcher used by the create saga. The daemon supplies a real PTY
/// implementation; tests inject a fake.
pub trait SessionSpawner: Send + Sync {
    /// Starts the agent process for `request`.
    ///
    /// # Errors
    ///
    /// Returns an error when the process cannot be started. The saga then
    /// compensates the Git worktree if one was created.
    fn spawn(&self, request: SpawnRequest<'_>) -> Result<SpawnedSession, SagaError>;

    /// Stops and forgets a process that was spawned by an uncommitted saga.
    ///
    /// # Errors
    ///
    /// Returns an error if the runtime cannot find or tear down the session.
    fn rollback(&self, _session_id: SessionId) -> Result<(), SagaError> {
        Ok(())
    }

    /// Returns whether this runtime currently owns a live process for `session_id`.
    ///
    /// Durable status is checked independently; removal is blocked when either
    /// source reports a live owner.
    #[must_use]
    fn is_live(&self, _session_id: SessionId) -> bool {
        false
    }
}

/// Test double that records spawn attempts and can fail on demand.
#[derive(Debug)]
pub struct FakeSpawner {
    pid: u32,
    fail: bool,
}

impl FakeSpawner {
    /// Returns a spawner that reports a stable non-zero pid.
    #[must_use]
    pub const fn succeeding(pid: u32) -> Self {
        Self { pid, fail: false }
    }

    /// Returns a spawner that fails every spawn attempt.
    #[must_use]
    pub const fn failing() -> Self {
        Self {
            pid: 4242,
            fail: true,
        }
    }
}

impl SessionSpawner for FakeSpawner {
    fn spawn(&self, request: SpawnRequest<'_>) -> Result<SpawnedSession, SagaError> {
        if self.fail {
            return Err(SagaError::new(
                SagaErrorKind::InjectedFailure,
                "Fake session spawner refused to start a process",
                "Use FakeSpawner::succeeding in tests that need a running session",
            )
            .with_session_id(request.session_id));
        }
        if self.pid == 0 {
            return Err(SagaError::new(
                SagaErrorKind::InvalidInput,
                "Fake session spawner pid must be greater than zero",
                "Construct FakeSpawner::succeeding with a non-zero pid",
            ));
        }
        Ok(SpawnedSession {
            pid: self.pid,
            pty_id: Some(format!("pty-{}", request.session_id.as_uuid().simple())),
        })
    }
}

impl SessionSpawner for SessionManager {
    fn spawn(&self, request: SpawnRequest<'_>) -> Result<SpawnedSession, SagaError> {
        let size = TerminalSize::new(request.rows, request.cols).map_err(SagaError::from)?;
        let handle = self
            .spawn_with_id(request.session_id, request.command, size)
            .map_err(SagaError::from)?;
        let pid = handle.pid.ok_or_else(|| {
            SagaError::new(
                SagaErrorKind::Spawn,
                "process exited before session creation committed",
                "Retry the session with an executable that remains available",
            )
            .with_session_id(request.session_id)
        })?;
        Ok(SpawnedSession {
            pid,
            pty_id: Some(format!("pty-{}", request.session_id.as_uuid().simple())),
        })
    }

    fn rollback(&self, session_id: SessionId) -> Result<(), SagaError> {
        let snapshot = match self.snapshot(session_id) {
            Ok(snapshot) => snapshot,
            Err(crate::SessionError::NotFound { .. }) => return Ok(()),
            Err(error) => return Err(SagaError::from(error)),
        };
        if snapshot.status.is_live() {
            self.kill(session_id).map_err(SagaError::from)?;
        }
        self.remove(session_id).map_err(SagaError::from)
    }

    fn is_live(&self, session_id: SessionId) -> bool {
        self.snapshot(session_id)
            .is_ok_and(|session| session.status.is_live())
    }
}
