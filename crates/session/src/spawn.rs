use std::path::Path;

use cli_master_core::{AgentId, CommandSpec, ProjectId, SessionId, WorktreeId};

use crate::SessionManager;
use crate::error::{SessionError, SessionErrorKind};

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
    fn spawn(&self, request: SpawnRequest<'_>) -> Result<SpawnedSession, SessionError>;

    /// Stops and forgets a process that was spawned by an uncommitted saga.
    ///
    /// # Errors
    ///
    /// Returns an error if the runtime cannot find or tear down the session.
    fn rollback(&self, _session_id: SessionId) -> Result<(), SessionError> {
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
    fn spawn(&self, request: SpawnRequest<'_>) -> Result<SpawnedSession, SessionError> {
        if self.fail {
            return Err(SessionError::new(
                SessionErrorKind::InjectedFailure,
                "Fake session spawner refused to start a process",
                "Use FakeSpawner::succeeding in tests that need a running session",
            )
            .with_session_id(request.session_id));
        }
        if self.pid == 0 {
            return Err(SessionError::new(
                SessionErrorKind::InvalidInput,
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
    fn spawn(&self, request: SpawnRequest<'_>) -> Result<SpawnedSession, SessionError> {
        let session_id = request.session_id;
        match self.create_prepared(&request) {
            Ok(session) => {
                let Some(pid) = session.pid else {
                    let _ = self.rollback_created(session_id);
                    return Err(SessionError::Spawn(
                        "process exited before session creation committed".to_owned(),
                    ));
                };
                Ok(SpawnedSession {
                    pid,
                    pty_id: session.pty_id,
                })
            }
            Err(error) => {
                let _ = self.rollback_created(session_id);
                Err(error)
            }
        }
    }

    fn rollback(&self, session_id: SessionId) -> Result<(), SessionError> {
        self.rollback_created(session_id)
    }

    fn is_live(&self, session_id: SessionId) -> bool {
        self.get(session_id)
            .is_some_and(|session| session.status.is_live())
    }
}
