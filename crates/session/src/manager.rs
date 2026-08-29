use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use cli_master_core::{CommandSpec, SessionId};
use parking_lot::{Condvar, Mutex};

use crate::{
    SessionError, SessionHandle, SessionManagerConfig, SessionSnapshot, SessionSubscription,
    TerminalSize, runtime::SessionRuntime,
};

/// Thread-safe owner of independent local PTY sessions.
#[derive(Clone)]
pub struct SessionManager {
    inner: Arc<ManagerInner>,
}

struct ManagerInner {
    registry: Mutex<ManagerRegistry>,
    registry_changed: Condvar,
    config: Arc<SessionManagerConfig>,
}

#[derive(Default)]
struct ManagerRegistry {
    sessions: HashMap<SessionId, Arc<SessionRuntime>>,
    spawning: HashSet<SessionId>,
    closing: bool,
}

struct SpawnReservation<'a> {
    inner: &'a ManagerInner,
    session_id: SessionId,
    completed: bool,
}

impl SessionManager {
    /// Creates an empty manager with validated resource bounds and deadlines.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::InvalidConfiguration`] when a bound or deadline
    /// cannot provide safe bounded behavior.
    pub fn new(config: SessionManagerConfig) -> Result<Self, SessionError> {
        config.validate()?;
        Ok(Self {
            inner: Arc::new(ManagerInner {
                registry: Mutex::new(ManagerRegistry::default()),
                registry_changed: Condvar::new(),
                config: Arc::new(config),
            }),
        })
    }

    /// Starts a structured executable directly in a new native PTY.
    ///
    /// The current process environment is inherited for normal CLI
    /// authentication, then `CommandSpec` environment entries are applied as
    /// exact overrides. No shell is inserted and no argument is interpolated.
    ///
    /// # Errors
    ///
    /// Returns an error when the working directory, PTY, process launch, or a
    /// required background worker cannot be initialized.
    pub fn spawn(
        &self,
        command: &CommandSpec,
        terminal_size: TerminalSize,
    ) -> Result<SessionHandle, SessionError> {
        self.spawn_with_id(SessionId::new(), command, terminal_size)
    }

    /// Starts a structured executable with a caller-allocated identifier.
    ///
    /// This supports durable creation sagas in which metadata is persisted
    /// before the operating-system process is launched.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::DuplicateSessionId`] before launching a process
    /// when the identifier is already registered. Other failures match
    /// [`Self::spawn`].
    pub fn spawn_with_id(
        &self,
        session_id: SessionId,
        command: &CommandSpec,
        terminal_size: TerminalSize,
    ) -> Result<SessionHandle, SessionError> {
        let reservation = self.inner.reserve_spawn(session_id)?;
        let runtime = SessionRuntime::spawn(
            session_id,
            command,
            terminal_size,
            Arc::clone(&self.inner.config),
        )?;
        let handle = runtime.handle();
        reservation.commit(runtime);
        Ok(handle)
    }

    /// Returns snapshots for every known session ordered by identifier.
    #[must_use]
    pub fn list(&self) -> Vec<SessionSnapshot> {
        let runtimes = self
            .inner
            .registry
            .lock()
            .sessions
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let mut snapshots = runtimes
            .iter()
            .map(|runtime| runtime.snapshot())
            .collect::<Vec<_>>();
        snapshots.sort_by_key(|snapshot| snapshot.id);
        snapshots
    }

    /// Returns current state for one managed session.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::NotFound`] for an unknown identifier.
    pub fn snapshot(&self, session_id: SessionId) -> Result<SessionSnapshot, SessionError> {
        Ok(self.runtime(session_id)?.snapshot())
    }

    /// Subscribes before capturing replay state so reconnecting clients cannot
    /// miss output produced during the handoff.
    ///
    /// Live output may overlap the replay snapshot; clients should de-duplicate
    /// by the monotonically increasing output sequence.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown session or a cursor greater than the
    /// latest output sequence.
    pub fn reconnect(
        &self,
        session_id: SessionId,
        after_sequence: u64,
    ) -> Result<SessionSubscription, SessionError> {
        self.runtime(session_id)?.reconnect(after_sequence)
    }

    /// Writes raw bytes to the target session's PTY.
    ///
    /// Sending byte `0x03` has normal terminal Ctrl+C semantics when the child
    /// has terminal signal processing enabled.
    ///
    /// # Errors
    ///
    /// Returns an error for missing/completed sessions, oversized input,
    /// bounded queue backpressure, deadline expiry, or operating-system I/O.
    pub fn write(&self, session_id: SessionId, bytes: &[u8]) -> Result<(), SessionError> {
        self.runtime(session_id)?.write(bytes)
    }

    /// Propagates validated dimensions to the target PTY.
    ///
    /// # Errors
    ///
    /// Returns an error for missing/completed sessions or an OS resize failure.
    pub fn resize(
        &self,
        session_id: SessionId,
        terminal_size: TerminalSize,
    ) -> Result<(), SessionError> {
        self.runtime(session_id)?.resize(terminal_size)
    }

    /// Gracefully stops one session, then escalates only its proven process tree.
    ///
    /// Stop first writes terminal Ctrl+C, then uses `SIGHUP`, and finally
    /// `SIGKILL` when each configured deadline expires.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown session, signal failure, or force-kill
    /// timeout.
    pub fn stop(&self, session_id: SessionId) -> Result<SessionSnapshot, SessionError> {
        self.runtime(session_id)?.stop()
    }

    /// Immediately sends `SIGKILL` to one identity-verified session process tree.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown session, signal failure, or reap timeout.
    pub fn kill(&self, session_id: SessionId) -> Result<SessionSnapshot, SessionError> {
        self.runtime(session_id)?.kill()
    }

    /// Removes one completed runtime and joins its background workers.
    ///
    /// # Errors
    ///
    /// Returns an error when the identifier is unknown, the process is live,
    /// or a worker terminated unexpectedly.
    pub fn remove(&self, session_id: SessionId) -> Result<(), SessionError> {
        let runtime = self.runtime(session_id)?;
        let snapshot = runtime.snapshot();
        if matches!(
            snapshot.status,
            cli_master_core::SessionStatus::Starting
                | cli_master_core::SessionStatus::Running
                | cli_master_core::SessionStatus::Idle
        ) {
            return Err(SessionError::RemoveLive {
                session_id,
                status: snapshot.status,
            });
        }
        runtime.close_and_join_workers()?;
        self.inner.registry.lock().sessions.remove(&session_id);
        Ok(())
    }

    /// Gracefully stops all sessions and joins all background workers.
    ///
    /// Every runtime is attempted even if an earlier stop fails. The first
    /// error is returned after cleanup attempts finish.
    ///
    /// # Errors
    ///
    /// Returns the first lifecycle or worker cleanup failure.
    pub fn shutdown(&self) -> Result<(), SessionError> {
        let runtimes = {
            let mut registry = self.inner.registry.lock();
            registry.closing = true;
            while !registry.spawning.is_empty() {
                self.inner.registry_changed.wait(&mut registry);
            }
            registry.sessions.values().cloned().collect::<Vec<_>>()
        };
        let mut first_error = None;
        for runtime in &runtimes {
            if let Err(error) = runtime.stop() {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        for runtime in &runtimes {
            if let Err(error) = runtime.close_and_join_workers() {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn runtime(&self, session_id: SessionId) -> Result<Arc<SessionRuntime>, SessionError> {
        self.inner
            .registry
            .lock()
            .sessions
            .get(&session_id)
            .cloned()
            .ok_or(SessionError::NotFound { session_id })
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        let config = SessionManagerConfig::default();
        Self {
            inner: Arc::new(ManagerInner {
                registry: Mutex::new(ManagerRegistry::default()),
                registry_changed: Condvar::new(),
                config: Arc::new(config),
            }),
        }
    }
}

impl ManagerInner {
    fn reserve_spawn(&self, session_id: SessionId) -> Result<SpawnReservation<'_>, SessionError> {
        let mut registry = self.registry.lock();
        if registry.closing {
            return Err(SessionError::ManagerShuttingDown);
        }
        if registry.sessions.contains_key(&session_id) || registry.spawning.contains(&session_id) {
            return Err(SessionError::DuplicateSessionId { session_id });
        }
        registry.spawning.insert(session_id);
        Ok(SpawnReservation {
            inner: self,
            session_id,
            completed: false,
        })
    }
}

impl SpawnReservation<'_> {
    fn commit(mut self, runtime: Arc<SessionRuntime>) {
        let mut registry = self.inner.registry.lock();
        let reserved = registry.spawning.remove(&self.session_id);
        debug_assert!(
            reserved,
            "spawn identifier must remain reserved until commit"
        );
        let previous = registry.sessions.insert(self.session_id, runtime);
        debug_assert!(
            previous.is_none(),
            "reserved spawn identifier must be unique"
        );
        self.completed = true;
        drop(registry);
        self.inner.registry_changed.notify_all();
    }
}

impl Drop for SpawnReservation<'_> {
    fn drop(&mut self) {
        if self.completed {
            return;
        }
        self.inner.registry.lock().spawning.remove(&self.session_id);
        self.inner.registry_changed.notify_all();
    }
}

impl Drop for ManagerInner {
    fn drop(&mut self) {
        let runtimes = self
            .registry
            .get_mut()
            .sessions
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for runtime in &runtimes {
            runtime.best_effort_terminate();
        }
        for runtime in &runtimes {
            let _ = runtime.close_and_join_workers();
        }
    }
}
