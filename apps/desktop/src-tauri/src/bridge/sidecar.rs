use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::net::UnixStream;
use tokio::process::Command;

use super::error::BridgeError;
use super::locate::{DaemonBinarySource, LocateEnv, LocateError, locate_daemon_binary};
use super::log;

/// Whether the actor may spawn `cli-masterd` when the socket is missing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SidecarMode {
    /// Never spawn. Used by tests that provide a mock socket.
    #[allow(
        dead_code,
        reason = "selected by tests that drive a mock Unix listener"
    )]
    Disabled,
    /// Spawn a detached sidecar when connect fails and a binary can be found.
    SpawnIfMissing,
}

/// Connects to the daemon socket, spawning a sidecar if allowed and needed.
///
/// The child is placed in its own process group, is not `kill_on_drop`, and is
/// reaped in the background. Closing the desktop window therefore cannot kill
/// live agent sessions owned by the daemon.
///
/// A successful connect is returned to the caller. The function never drops a
/// live stream after probing, so mock servers and `cli-masterd` both see the
/// handshake on the first accepted connection.
///
/// # Errors
///
/// Returns [`BridgeError::Unavailable`] when the socket cannot be reached.
pub async fn connect_socket(
    socket: &Path,
    mode: SidecarMode,
    connect_timeout: Duration,
    ready_timeout: Duration,
    spawn_budget: &mut u32,
) -> Result<UnixStream, BridgeError> {
    if let Some(stream) = try_connect(socket, connect_timeout).await {
        return Ok(stream);
    }
    if !matches!(mode, SidecarMode::SpawnIfMissing) {
        return Err(BridgeError::Unavailable {
            detail: "daemon socket is not listening".to_owned(),
        });
    }
    if *spawn_budget == 0 {
        return Err(BridgeError::Unavailable {
            detail: "daemon sidecar spawn budget exhausted".to_owned(),
        });
    }
    spawn_sidecar()?;
    *spawn_budget -= 1;
    wait_for_stream(socket, ready_timeout).await
}

async fn try_connect(socket: &Path, connect_timeout: Duration) -> Option<UnixStream> {
    tokio::time::timeout(connect_timeout, UnixStream::connect(socket))
        .await
        .ok()
        .and_then(Result::ok)
}

async fn wait_for_stream(socket: &Path, timeout: Duration) -> Result<UnixStream, BridgeError> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(stream) = UnixStream::connect(socket).await {
            return Ok(stream);
        }
        if Instant::now() >= deadline {
            return Err(BridgeError::Unavailable {
                detail: "daemon socket did not become ready".to_owned(),
            });
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn spawn_sidecar() -> Result<(), BridgeError> {
    let located = locate_daemon_binary(&LocateEnv::from_process()).map_err(|error| {
        BridgeError::Unavailable {
            detail: match error {
                LocateError::InvalidEnvironment => {
                    "CLI_MASTERD is not an absolute executable file".to_owned()
                }
                LocateError::NotFound => "cli-masterd executable was not found".to_owned(),
            },
        }
    })?;
    let mut command = Command::new(&located.path);
    command.stdin(Stdio::null());
    command.stdout(Stdio::null());
    command.stderr(Stdio::null());
    command.kill_on_drop(false);
    command.process_group(0);
    let mut child = command.spawn().map_err(|_| BridgeError::Unavailable {
        detail: "could not spawn cli-masterd sidecar".to_owned(),
    })?;
    let pid = child.id().ok_or_else(|| BridgeError::Unavailable {
        detail: "cli-masterd sidecar produced no pid".to_owned(),
    })?;
    log::sidecar_spawned(source_label(located.source), pid);
    tokio::spawn(async move {
        match child.wait().await {
            Ok(status) => tracing::info!(success = status.success(), "cli-masterd sidecar exited"),
            Err(error) => tracing::warn!(%error, "could not wait for cli-masterd sidecar"),
        }
    });
    Ok(())
}

const fn source_label(source: DaemonBinarySource) -> &'static str {
    match source {
        DaemonBinarySource::Environment => "environment",
        DaemonBinarySource::ExecutableDirectory => "executable_directory",
        DaemonBinarySource::Path => "path",
        DaemonBinarySource::WorkspaceTarget => "workspace_target",
    }
}
