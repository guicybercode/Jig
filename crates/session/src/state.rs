use std::time::{Instant, SystemTime, UNIX_EPOCH};

use cli_master_core::SessionStatus;

use crate::TerminalSize;

pub(super) struct RuntimeState {
    pub status: SessionStatus,
    pub exit_code: Option<i32>,
    pub created_at_ms: i64,
    pub last_activity_at_ms: i64,
    pub last_activity_instant: Instant,
    pub terminal_size: TerminalSize,
    pub stop_requested: bool,
    pub reader_finished: bool,
    pub io_access: IoAccess,
    pub io_failure_reported: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum IoAccess {
    Open,
    InputClosed,
    Closed,
}

impl IoAccess {
    pub fn accepts_input(self) -> bool {
        self == Self::Open
    }

    pub fn accepts_output(self) -> bool {
        self != Self::Closed
    }
}

pub(super) fn is_live(status: SessionStatus) -> bool {
    matches!(
        status,
        SessionStatus::Starting | SessionStatus::Running | SessionStatus::Idle
    )
}

pub(super) fn is_terminal(status: SessionStatus) -> bool {
    matches!(
        status,
        SessionStatus::Exited | SessionStatus::Failed | SessionStatus::Unknown
    )
}

pub(super) fn unix_epoch_ms() -> i64 {
    let milliseconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    i64::try_from(milliseconds).unwrap_or(i64::MAX)
}
