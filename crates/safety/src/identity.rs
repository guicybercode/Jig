use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use cli_master_core::{
    ApplicationError, ErrorCode, PROCESS_FORCE_KILL_TIMEOUT, PROCESS_STOP_GRACE,
};

use crate::process::{SpawnRequest, run_command_unchecked};

/// Recorded identity of a child the current daemon instance spawned.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessIdentity {
    /// Operating-system process identifier.
    pub pid: u32,
    /// Opaque start-time token used to detect PID reuse.
    pub start_token: String,
    /// Daemon instance that created the process.
    pub daemon_instance_id: String,
}

/// Confirmed stop of a live session process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessStopPlan {
    /// Process to stop.
    pub identity: ProcessIdentity,
    /// Whether SIGKILL may be sent after the grace period.
    pub force: bool,
}

/// Reads a start-time token for `pid` without signaling it.
///
/// # Errors
///
/// Returns [`ErrorCode::ProcessIdentityMismatch`] when the process cannot be
/// identified safely.
pub fn record_identity(
    pid: u32,
    daemon_instance_id: impl Into<String>,
) -> Result<ProcessIdentity, ApplicationError> {
    Ok(ProcessIdentity {
        pid,
        start_token: read_start_token(pid)?,
        daemon_instance_id: daemon_instance_id.into(),
    })
}

/// Stops a process only when its recorded identity still matches.
///
/// Stale PIDs are never signaled. Force-kill is never implied; the caller must
/// set [`ProcessStopPlan::force`] after an explicit confirmation.
///
/// # Errors
///
/// Returns an error when the PID has been reused, the process has already
/// exited, or the stop command fails.
pub fn stop_process(
    plan: &ProcessStopPlan,
    current_daemon_instance_id: &str,
) -> Result<(), ApplicationError> {
    if plan.identity.daemon_instance_id != current_daemon_instance_id {
        return Err(ApplicationError::new(
            ErrorCode::ProcessIdentityMismatch,
            "Refusing to stop a process owned by a different daemon instance.",
        )
        .not_recoverable()
        .with_action("Restart the session from CLI Master instead of killing a reused PID.")
        .with_context("pid", i64::from(plan.identity.pid)));
    }

    let current = read_start_token(plan.identity.pid)?;
    if current != plan.identity.start_token {
        return Err(ApplicationError::new(
            ErrorCode::ProcessIdentityMismatch,
            "The recorded PID no longer matches the original process.",
        )
        .not_recoverable()
        .with_action("Do not kill this PID. Restart the session if it is still needed.")
        .with_context("pid", i64::from(plan.identity.pid)));
    }

    send_signal(plan.identity.pid, "TERM")?;
    if wait_until_gone(plan.identity.pid, PROCESS_STOP_GRACE) {
        return Ok(());
    }

    if !plan.force {
        return Err(ApplicationError::new(
            ErrorCode::ConfirmationRequired,
            "The process did not exit after SIGTERM.",
        )
        .with_action("Confirm a force stop if you want CLI Master to send SIGKILL.")
        .with_context("pid", i64::from(plan.identity.pid)));
    }

    let current = read_start_token(plan.identity.pid)?;
    if current != plan.identity.start_token {
        return Err(ApplicationError::new(
            ErrorCode::ProcessIdentityMismatch,
            "The PID was reused before SIGKILL could be sent.",
        )
        .not_recoverable()
        .with_action("Do not kill this PID.")
        .with_context("pid", i64::from(plan.identity.pid)));
    }

    send_signal(plan.identity.pid, "KILL")?;
    if wait_until_gone(plan.identity.pid, PROCESS_FORCE_KILL_TIMEOUT) {
        Ok(())
    } else {
        Err(ApplicationError::new(
            ErrorCode::PtySpawnFailed,
            "The process is still alive after SIGKILL.",
        )
        .with_action("Inspect the process manually. It may be uninterruptible.")
        .with_context("pid", i64::from(plan.identity.pid)))
    }
}

fn read_start_token(pid: u32) -> Result<String, ApplicationError> {
    let linux = Path::new("/proc").join(pid.to_string()).join("stat");
    if linux.exists() {
        let contents = fs::read_to_string(&linux).map_err(|error| {
            identity_error(pid, "Could not read process start time.").with_source(&error)
        })?;
        return parse_linux_starttime(&contents)
            .ok_or_else(|| identity_error(pid, "Process start time was missing from /proc."));
    }

    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "lstart="])
        .output()
        .map_err(|error| {
            identity_error(pid, "Could not inspect the process.").with_source(&error)
        })?;
    if !output.status.success() {
        return Err(identity_error(
            pid,
            "The process is no longer running, so it will not be signaled.",
        ));
    }
    let token = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if token.is_empty() {
        Err(identity_error(pid, "Process start time was unavailable."))
    } else {
        Ok(token)
    }
}

fn parse_linux_starttime(stat: &str) -> Option<String> {
    let (_prefix, rest) = stat.rsplit_once(')')?;
    rest.split_whitespace().nth(19).map(ToOwned::to_owned)
}

fn send_signal(pid: u32, signal: &str) -> Result<(), ApplicationError> {
    let output = run_command_unchecked(
        &SpawnRequest::new("/bin/kill")
            .arg(format!("-{signal}"))
            .arg(pid.to_string())
            .timeout(Duration::from_secs(2)),
    )?;
    if output.success() || output.exit_code == Some(1) {
        Ok(())
    } else {
        Err(ApplicationError::new(
            ErrorCode::PtySpawnFailed,
            format!("Could not send {signal} to process {pid}."),
        )
        .with_action("Retry the stop, or inspect the process manually.")
        .with_context("pid", i64::from(pid)))
    }
}

fn wait_until_gone(pid: u32, timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if !Path::new("/proc").join(pid.to_string()).exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    !Path::new("/proc").join(pid.to_string()).exists()
}

fn identity_error(pid: u32, message: &str) -> ApplicationError {
    ApplicationError::new(ErrorCode::ProcessIdentityMismatch, message.to_owned())
        .not_recoverable()
        .with_action("Do not signal this PID. Restart the session from CLI Master.")
        .with_context("pid", i64::from(pid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_identity_for_current_process() {
        let pid = std::process::id();
        let identity =
            record_identity(pid, "daemon-1").expect("current process should be readable");
        assert_eq!(identity.pid, pid);
        assert!(!identity.start_token.is_empty());
    }

    #[test]
    fn refuses_to_stop_a_foreign_daemon_instance() {
        let identity =
            record_identity(std::process::id(), "other-daemon").expect("identity should load");
        let error = stop_process(
            &ProcessStopPlan {
                identity,
                force: false,
            },
            "this-daemon",
        )
        .expect_err("foreign instance must be refused");
        assert_eq!(error.code(), ErrorCode::ProcessIdentityMismatch);
    }
}
