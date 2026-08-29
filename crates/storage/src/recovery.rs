use cli_master_core::{SessionId, SessionStatus};
use rusqlite::{Transaction, params};

use crate::Storage;
use crate::error::StorageError;
use crate::models::{StoredSession, validate_daemon_instance_id, validate_timestamp};
use crate::sessions::list_sessions_from_connection;

/// Inputs supplied by the daemon before it accepts IPC clients.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryContext<'a> {
    /// Fresh daemon lifetime identifier.
    pub current_daemon_instance_id: &'a str,
    /// Sessions for which the current in-memory manager owns a live PTY.
    pub live_session_ids: &'a [SessionId],
    /// Reconciliation time as Unix epoch milliseconds.
    pub updated_at_ms: i64,
}

/// Why one persisted session was kept or changed during reconciliation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReconciliationReason {
    /// Non-live metadata already represented a known durable state.
    Known,
    /// The current process owns the session's live runtime handles.
    Running,
    /// The recorded owner matches this daemon, but no live runtime exists.
    ProcessGone,
    /// A previous daemon lifetime left process-bearing metadata behind.
    DaemonRestarted,
    /// The session already recorded a terminal result.
    ExitedNormally,
}

/// Result of reconciling one durable session row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconciliationEvent {
    /// Session that was inspected.
    pub session_id: SessionId,
    /// Status before reconciliation.
    pub previous_status: SessionStatus,
    /// Status after reconciliation.
    pub new_status: SessionStatus,
    /// Reason the status was retained or changed.
    pub reason: ReconciliationReason,
}

impl Storage {
    /// Reconciles durable session state against runtime handles owned in memory.
    ///
    /// A persisted PID is never consulted for liveness. Any process-bearing row
    /// absent from `live_session_ids` becomes `unknown` and loses its PID and
    /// daemon ownership fields in the same transaction.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid context, poisoned locking, or database
    /// failure. On error, no partial reconciliation commits.
    pub fn reconcile_sessions(
        &self,
        context: &RecoveryContext<'_>,
    ) -> Result<Vec<ReconciliationEvent>, StorageError> {
        validate_daemon_instance_id(context.current_daemon_instance_id)?;
        validate_timestamp("session updated_at_ms", context.updated_at_ms)?;
        self.transaction(|transaction| reconcile_transaction(transaction, context))
    }
}

fn reconcile_transaction(
    transaction: &Transaction<'_>,
    context: &RecoveryContext<'_>,
) -> Result<Vec<ReconciliationEvent>, StorageError> {
    let sessions = list_sessions_from_connection(transaction)?;
    let mut events = Vec::with_capacity(sessions.len());

    for stored in sessions {
        events.push(reconcile_session(transaction, context, &stored)?);
    }

    Ok(events)
}

fn reconcile_session(
    transaction: &Transaction<'_>,
    context: &RecoveryContext<'_>,
    stored: &StoredSession,
) -> Result<ReconciliationEvent, StorageError> {
    let previous_status = stored.status;
    let (new_status, reason) = if matches!(
        previous_status,
        SessionStatus::Exited | SessionStatus::Failed
    ) {
        (previous_status, ReconciliationReason::ExitedNormally)
    } else if context.live_session_ids.contains(&stored.id) {
        (previous_status, ReconciliationReason::Running)
    } else if !previous_status.is_live() {
        (previous_status, ReconciliationReason::Known)
    } else if stored.daemon_instance_id.as_deref() == Some(context.current_daemon_instance_id) {
        (SessionStatus::Unknown, ReconciliationReason::ProcessGone)
    } else {
        (
            SessionStatus::Unknown,
            ReconciliationReason::DaemonRestarted,
        )
    };

    if new_status == SessionStatus::Unknown && previous_status.is_live() {
        let error_code = match reason {
            ReconciliationReason::ProcessGone => "process_gone",
            ReconciliationReason::DaemonRestarted => "daemon_restarted",
            _ => "runtime_missing",
        };
        transaction.execute(
            "UPDATE sessions
             SET status = 'unknown', runtime_pid = NULL, daemon_instance_id = NULL,
                 exit_code = NULL, error_code = ?1, updated_at = ?2
             WHERE id = ?3",
            params![error_code, context.updated_at_ms, stored.id.to_string()],
        )?;
    }

    Ok(ReconciliationEvent {
        session_id: stored.id,
        previous_status,
        new_status,
        reason,
    })
}
