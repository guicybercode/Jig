use cli_master_core::{SessionId, SessionStatus};
use rusqlite::Connection;

use crate::error::StorageError;
use crate::records::{ReconciliationEvent, ReconciliationReason, RecoveryContext};
use crate::repo::SessionRepository;

const LIVE_STATUSES: [SessionStatus; 3] = [
    SessionStatus::Starting,
    SessionStatus::Running,
    SessionStatus::Idle,
];

/// Runtime index of sessions that currently have a process in this daemon.
///
/// `SessionManager` implements this. A persisted PID must never be treated as
/// proof of liveness.
pub trait LiveSessionIndex {
    /// Returns whether this daemon currently owns a live PTY for `session_id`.
    fn is_live(&self, session_id: SessionId) -> bool;
}

impl LiveSessionIndex for [SessionId] {
    fn is_live(&self, session_id: SessionId) -> bool {
        self.contains(&session_id)
    }
}

impl LiveSessionIndex for Vec<SessionId> {
    fn is_live(&self, session_id: SessionId) -> bool {
        self.as_slice().is_live(session_id)
    }
}

/// Reconciles persisted session rows with the live session manager.
///
/// Previously `starting`/`running`/`idle` rows are marked `unknown` when this
/// process does not own them. History, exit codes, and last PIDs are kept.
/// Processes are never recreated here.
pub(crate) fn reconcile_sessions(
    connection: &Connection,
    context: &RecoveryContext<'_>,
) -> Result<Vec<ReconciliationEvent>, StorageError> {
    let sessions = SessionRepository::new(connection).list()?;
    let mut events = Vec::with_capacity(sessions.len());

    for stored in sessions {
        let session_id = stored.session.id;
        let previous_status = stored.session.status;
        let live = context.live_session_ids.is_live(session_id);
        let same_daemon = stored
            .daemon_instance_id
            .as_deref()
            .is_some_and(|id| id == context.current_daemon_instance_id);

        let (new_status, reason) = if matches!(
            previous_status,
            SessionStatus::Exited | SessionStatus::Failed
        ) {
            (previous_status, ReconciliationReason::ExitedNormally)
        } else if live {
            (previous_status, ReconciliationReason::Running)
        } else if !LIVE_STATUSES.contains(&previous_status) {
            (previous_status, ReconciliationReason::Known)
        } else if same_daemon {
            (SessionStatus::Unknown, ReconciliationReason::ProcessGone)
        } else {
            (
                SessionStatus::Unknown,
                ReconciliationReason::DaemonRestarted,
            )
        };

        if new_status != previous_status {
            SessionRepository::new(connection).mark_unknown(session_id)?;
        }

        events.push(ReconciliationEvent {
            session_id,
            previous_status,
            new_status,
            reason,
        });
    }

    Ok(events)
}
