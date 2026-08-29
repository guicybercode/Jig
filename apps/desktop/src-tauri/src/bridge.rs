//! Unix-socket client that forwards typed IPC to `cli-masterd`.

use std::env;
use std::fs::{self, OpenOptions};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use cli_master_core::ipc::codes;
use cli_master_core::{ApiError, RequestEnvelope, ResponseEnvelope, ResponsePayload};
use cli_master_daemon::{AppPaths, read_frame, write_frame};
use serde_json::Value;

/// Connected view of the per-user daemon socket.
#[derive(Clone, Debug)]
pub struct Bridge {
    socket: PathBuf,
}

impl Bridge {
    /// Spawns `cli-masterd` when needed and waits for the socket.
    ///
    /// # Errors
    ///
    /// Returns [`codes::DAEMON_UNAVAILABLE`] when the daemon cannot be started
    /// or the socket never appears.
    pub fn connect() -> Result<Self, ApiError> {
        let paths = AppPaths::for_user().map_err(|error| {
            ApiError::new(codes::DAEMON_UNAVAILABLE, error.to_string())
                .with_action("Check that the user data directory is writable")
        })?;
        if UnixStream::connect(&paths.socket).is_err() {
            if paths.socket.exists() {
                let _ = fs::remove_file(&paths.socket);
            }
            spawn_daemon(&paths)?;
            wait_for_socket(&paths.socket)?;
        }
        Ok(Self {
            socket: paths.socket,
        })
    }

    /// Sends one version 1 request and returns the decoded payload.
    ///
    /// # Errors
    ///
    /// Returns a daemon API error or [`codes::DAEMON_UNAVAILABLE`].
    pub fn request(&self, method: &str, payload: Value) -> Result<Value, ApiError> {
        let mut stream = UnixStream::connect(&self.socket).map_err(|error| {
            ApiError::new(codes::DAEMON_UNAVAILABLE, error.to_string())
                .with_action("Start CLI Master again so the local daemon can bind its socket")
        })?;
        let request = RequestEnvelope::v1(method, payload);
        write_frame(&mut stream, &request)
            .map_err(|error| ApiError::new(codes::DAEMON_UNAVAILABLE, error.to_string()))?;
        let body = read_frame(&mut stream)
            .map_err(|error| ApiError::new(codes::DAEMON_UNAVAILABLE, error.to_string()))?;
        let response: ResponseEnvelope<Value> = serde_json::from_slice(&body)
            .map_err(|error| ApiError::new(codes::INVALID_REQUEST, error.to_string()))?;
        match response.payload {
            ResponsePayload::Success { data } => Ok(data),
            ResponsePayload::Error { error } => Err(error),
        }
    }
}

fn spawn_daemon(paths: &AppPaths) -> Result<(), ApiError> {
    let executable = daemon_executable();
    let log_path = paths.log_dir.join("cli-masterd.log");
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|error| {
            ApiError::new(codes::DAEMON_UNAVAILABLE, error.to_string())
                .with_action("Check that the log directory is writable")
        })?;
    let stderr = log
        .try_clone()
        .map_err(|error| ApiError::new(codes::DAEMON_UNAVAILABLE, error.to_string()))?;
    Command::new(&executable)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr))
        .spawn()
        .map_err(|error| {
            ApiError::new(
                codes::DAEMON_UNAVAILABLE,
                format!("Could not start {}: {error}", executable.display()),
            )
            .with_action("Build cli-masterd or set CLI_MASTERD to its path")
        })?;
    Ok(())
}

fn wait_for_socket(socket: &Path) -> Result<(), ApiError> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if UnixStream::connect(socket).is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(40));
    }
    Err(
        ApiError::new(codes::DAEMON_UNAVAILABLE, "The local daemon did not start").with_action(
            "Inspect logs in the CLI Master data directory or start cli-masterd manually",
        ),
    )
}

fn daemon_executable() -> PathBuf {
    if let Ok(path) = env::var("CLI_MASTERD") {
        return PathBuf::from(path);
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("cli-masterd");
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    PathBuf::from("cli-masterd")
}
