use cli_master_core::{CommandSpec, SessionId};

use crate::error::{SagaError, SagaErrorKind};

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
    /// Structured launch command. Never a shell string.
    pub command: &'a CommandSpec,
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
        let _ = request;
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
