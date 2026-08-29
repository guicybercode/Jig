use serde::{Deserialize, Serialize};

use crate::error_codes;

/// Lifecycle state of one agent session.
///
/// Persisted rows record this status. Live PTY identity is owned by
/// `SessionManager` and is never serialized as a handle. `unknown` is a
/// reconciliation result, not a process state the UI can request.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Metadata exists. No process has been spawned yet.
    Created,
    /// Launch is in progress. A PTY and child may exist, but readiness is not
    /// confirmed.
    Starting,
    /// The process exists and recent PTY input/output indicates activity.
    Running,
    /// The process exists but no PTY activity occurred for the idle interval.
    Idle,
    /// Stop was requested. The process group is being signaled.
    Stopping,
    /// The process ended and an exit code was recorded.
    Exited,
    /// Validation, spawn, PTY, or abnormal exit failed with an actionable error.
    Failed,
    /// Metadata claims a live session that this daemon instance cannot prove.
    #[serde(other)]
    Unknown,
}

impl SessionStatus {
    /// Every status that protocol v1 can persist and send on the wire.
    pub const ALL: [Self; 8] = [
        Self::Created,
        Self::Starting,
        Self::Running,
        Self::Idle,
        Self::Stopping,
        Self::Exited,
        Self::Failed,
        Self::Unknown,
    ];

    /// Returns the stable `snake_case` wire value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Idle => "idle",
            Self::Stopping => "stopping",
            Self::Exited => "exited",
            Self::Failed => "failed",
            Self::Unknown => "unknown",
        }
    }

    /// Returns whether a process/PTY may exist for this status.
    #[must_use]
    pub const fn is_live(self) -> bool {
        matches!(
            self,
            Self::Starting | Self::Running | Self::Idle | Self::Stopping
        )
    }

    /// Returns whether the process is gone and metadata is terminal.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Exited | Self::Failed)
    }

    /// Returns whether this row must be reconciled after a daemon restart.
    #[must_use]
    pub const fn requires_recovery(self) -> bool {
        self.is_live()
    }

    /// Returns whether `next` is a legal transition from this status.
    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Created, Self::Starting | Self::Failed)
                | (
                    Self::Starting,
                    Self::Running | Self::Failed | Self::Stopping
                )
                | (
                    Self::Running,
                    Self::Idle | Self::Stopping | Self::Exited | Self::Failed
                )
                | (
                    Self::Idle,
                    Self::Running | Self::Stopping | Self::Exited | Self::Failed
                )
                | (Self::Stopping, Self::Exited | Self::Failed)
                | (Self::Exited | Self::Failed | Self::Unknown, Self::Starting)
                | (Self::Unknown, Self::Failed)
        )
    }

    /// Applies a requested transition.
    ///
    /// # Errors
    ///
    /// Returns [`SessionTransitionError`] when `next` is not allowed.
    pub const fn transition(self, next: Self) -> Result<Self, SessionTransitionError> {
        if self.can_transition_to(next) {
            Ok(next)
        } else {
            Err(SessionTransitionError {
                from: self,
                to: next,
            })
        }
    }

    /// Status used when a formerly live row is loaded by a new daemon instance.
    #[must_use]
    pub const fn recovered_from_crash(self) -> Self {
        if self.requires_recovery() {
            Self::Unknown
        } else {
            self
        }
    }
}

/// Rejection for an illegal session status change.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionTransitionError {
    /// Status stored before the rejected change.
    pub from: SessionStatus,
    /// Status the caller attempted to apply.
    pub to: SessionStatus,
}

impl SessionTransitionError {
    /// Converts this transition failure into the public IPC error.
    #[must_use]
    pub fn to_application_error(self) -> crate::ApplicationError {
        crate::ApplicationError::new(
            error_codes::SESSION_INVALID_TRANSITION,
            format!(
                "session cannot move from {} to {}",
                self.from.as_str(),
                self.to.as_str()
            ),
        )
        .with_action("Wait for the current operation to finish, or restart the session.")
        .with_detail("from", self.from.as_str())
        .with_detail("to", self.to.as_str())
    }
}

impl std::fmt::Display for SessionTransitionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid session transition: {} -> {}",
            self.from.as_str(),
            self.to.as_str()
        )
    }
}

impl std::error::Error for SessionTransitionError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_statuses_have_stable_wire_values() {
        let cases = [
            (SessionStatus::Created, "created"),
            (SessionStatus::Starting, "starting"),
            (SessionStatus::Running, "running"),
            (SessionStatus::Idle, "idle"),
            (SessionStatus::Stopping, "stopping"),
            (SessionStatus::Exited, "exited"),
            (SessionStatus::Failed, "failed"),
            (SessionStatus::Unknown, "unknown"),
        ];

        for (status, wire_value) in cases {
            let encoded = serde_json::to_string(&status).expect("status should serialize");
            assert_eq!(encoded, format!("\"{wire_value}\""));
            assert_eq!(status.as_str(), wire_value);
        }
    }

    #[test]
    fn unknown_wire_value_decodes_as_unknown() {
        let status: SessionStatus =
            serde_json::from_str("\"waiting_for_vendor\"").expect("unknown status should decode");
        assert_eq!(status, SessionStatus::Unknown);
    }

    #[test]
    fn legal_happy_path_reaches_exited() {
        let status = SessionStatus::Created
            .transition(SessionStatus::Starting)
            .and_then(|status| status.transition(SessionStatus::Running))
            .and_then(|status| status.transition(SessionStatus::Idle))
            .and_then(|status| status.transition(SessionStatus::Stopping))
            .and_then(|status| status.transition(SessionStatus::Exited))
            .expect("happy path should be legal");
        assert_eq!(status, SessionStatus::Exited);
    }

    #[test]
    fn restart_is_only_allowed_from_terminal_or_unknown() {
        assert!(SessionStatus::Exited.can_transition_to(SessionStatus::Starting));
        assert!(SessionStatus::Failed.can_transition_to(SessionStatus::Starting));
        assert!(SessionStatus::Unknown.can_transition_to(SessionStatus::Starting));
        assert!(!SessionStatus::Running.can_transition_to(SessionStatus::Starting));
        assert!(!SessionStatus::Created.can_transition_to(SessionStatus::Running));
        assert!(!SessionStatus::Stopping.can_transition_to(SessionStatus::Running));
        assert!(!SessionStatus::Exited.can_transition_to(SessionStatus::Idle));
    }

    #[test]
    fn daemon_recovery_marks_live_rows_unknown() {
        assert_eq!(
            SessionStatus::Running.recovered_from_crash(),
            SessionStatus::Unknown
        );
        assert_eq!(
            SessionStatus::Stopping.recovered_from_crash(),
            SessionStatus::Unknown
        );
        assert_eq!(
            SessionStatus::Created.recovered_from_crash(),
            SessionStatus::Created
        );
        assert_eq!(
            SessionStatus::Exited.recovered_from_crash(),
            SessionStatus::Exited
        );
    }

    #[test]
    fn live_statuses_are_exactly_the_process_bearing_ones() {
        for status in SessionStatus::ALL {
            assert_eq!(
                status.is_live(),
                matches!(
                    status,
                    SessionStatus::Starting
                        | SessionStatus::Running
                        | SessionStatus::Idle
                        | SessionStatus::Stopping
                )
            );
        }
    }
}
