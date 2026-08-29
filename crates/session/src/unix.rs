use nix::{
    errno::Errno,
    sys::signal::{self, Signal},
    unistd::{self, Pid},
};

use crate::error::SessionError;

pub(crate) fn sanitize_pgid(pgid: Option<i32>) -> Option<i32> {
    let pgid = pgid.filter(|value| *value > 1)?;
    if pgid == unistd::getpgrp().as_raw() {
        tracing::error!(
            event = "session.signal_skipped",
            reason = "own_process_group",
            pgid,
            "refusing to target the daemon process group"
        );
        return None;
    }
    Some(pgid)
}

pub(crate) fn signal_group(pgid: i32, signal: Signal) -> Result<(), SessionError> {
    let Some(pgid) = sanitize_pgid(Some(pgid)) else {
        return Ok(());
    };

    match signal::killpg(Pid::from_raw(pgid), signal) {
        Ok(()) => {
            tracing::info!(
                event = "session.signal",
                pgid,
                signal = signal.as_str(),
                "signaled session process group"
            );
            Ok(())
        }
        Err(Errno::ESRCH) => Ok(()),
        Err(error) => Err(SessionError::Signal(error.to_string())),
    }
}

pub(crate) fn interrupt_signal() -> Signal {
    Signal::SIGINT
}

pub(crate) fn terminate_signal() -> Signal {
    Signal::SIGTERM
}

pub(crate) fn kill_signal() -> Signal {
    Signal::SIGKILL
}
