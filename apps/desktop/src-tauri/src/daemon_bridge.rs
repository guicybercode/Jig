use std::env;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::{collections::HashMap, sync::Arc};

use cli_master_daemon::{DaemonConfig, MAX_FRAME_LENGTH};
use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, EventTarget, State, Webview};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::sync::{Mutex, oneshot};
use tokio::time::{Duration, sleep};

use crate::browser::is_main_webview;

const STARTUP_ATTEMPTS: usize = 30;
const STARTUP_RETRY_DELAY: Duration = Duration::from_millis(100);
const DAEMON_PATH_ENV: &str = "CLI_MASTER_DAEMON_PATH";

/// Serializes daemon startup so simultaneous frontend requests do not launch
/// competing sidecars.
pub(crate) struct DaemonBridge {
    startup_lock: Mutex<()>,
    terminal_relays: Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
}

impl Default for DaemonBridge {
    fn default() -> Self {
        Self {
            startup_lock: Mutex::new(()),
            terminal_relays: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

/// Forwards one versioned frontend envelope to the per-user daemon.
#[tauri::command]
pub(crate) async fn daemon_request(
    request: Value,
    caller: Webview,
    bridge: State<'_, DaemonBridge>,
) -> Result<Value, BridgeError> {
    require_main_caller(&caller)?;
    bridge.request(&request).await
}

/// Opens a persistent daemon event stream for one terminal session.
#[tauri::command]
pub(crate) async fn daemon_terminal_subscribe(
    request: Value,
    caller: Webview,
    app: AppHandle,
    bridge: State<'_, DaemonBridge>,
) -> Result<Value, BridgeError> {
    require_main_caller(&caller)?;
    bridge.subscribe_terminal(&request, app).await
}

/// Stops the desktop relay and closes its daemon socket for one terminal.
#[tauri::command]
pub(crate) async fn daemon_terminal_unsubscribe(
    session_id: String,
    caller: Webview,
    bridge: State<'_, DaemonBridge>,
) -> Result<(), BridgeError> {
    require_main_caller(&caller)?;
    bridge.unsubscribe_terminal(&session_id).await;
    Ok(())
}

impl DaemonBridge {
    async fn request(&self, request: &Value) -> Result<Value, BridgeError> {
        let config = DaemonConfig::discover()
            .map_err(|error| BridgeError::unavailable("resolve daemon paths", error.to_string()))?;

        match send_request(config.socket_path(), request).await {
            Ok(response) => return Ok(response),
            Err(error) if !error.is_connection_failure() => {
                return Err(BridgeError::unavailable("exchange daemon request", error));
            }
            Err(_) => {}
        }

        let _startup_guard = self.startup_lock.lock().await;

        // A request that acquired the lock first may already have completed
        // daemon startup while this request was waiting.
        if let Ok(response) = send_request(config.socket_path(), request).await {
            return Ok(response);
        }

        start_daemon().map_err(BridgeError::startup)?;

        let mut last_error = None;
        for _ in 0..STARTUP_ATTEMPTS {
            sleep(STARTUP_RETRY_DELAY).await;
            match send_request(config.socket_path(), request).await {
                Ok(response) => return Ok(response),
                Err(error) => last_error = Some(error),
            }
        }

        Err(BridgeError::unavailable(
            "connect after daemon startup",
            last_error.unwrap_or(BridgeFailure::MissingResponse),
        ))
    }

    async fn subscribe_terminal(
        &self,
        request: &Value,
        app: AppHandle,
    ) -> Result<Value, BridgeError> {
        let session_id = request
            .get("payload")
            .and_then(|payload| payload.get("sessionId"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                BridgeError::invalid_request("terminal subscription is missing a sessionId")
            })?
            .to_owned();
        let config = DaemonConfig::discover()
            .map_err(|error| BridgeError::unavailable("resolve daemon paths", error.to_string()))?;

        let opened = match open_subscription(config.socket_path(), request).await {
            Ok(opened) => opened,
            Err(error) if !error.is_connection_failure() => {
                return Err(BridgeError::unavailable(
                    "open terminal event stream",
                    error,
                ));
            }
            Err(_) => {
                let _startup_guard = self.startup_lock.lock().await;
                if let Ok(opened) = open_subscription(config.socket_path(), request).await {
                    opened
                } else {
                    start_daemon().map_err(BridgeError::startup)?;
                    let mut last_error = BridgeFailure::MissingResponse;
                    let mut opened = None;
                    for _ in 0..STARTUP_ATTEMPTS {
                        sleep(STARTUP_RETRY_DELAY).await;
                        match open_subscription(config.socket_path(), request).await {
                            Ok(stream) => {
                                opened = Some(stream);
                                break;
                            }
                            Err(error) => last_error = error,
                        }
                    }
                    opened.ok_or_else(|| {
                        BridgeError::unavailable("connect terminal event stream", last_error)
                    })?
                }
            }
        };

        let (mut stream, response) = opened;
        if response.get("status").and_then(Value::as_str) != Some("success") {
            return Ok(response);
        }

        let (cancel, mut cancelled) = oneshot::channel();
        if let Some(previous) = self.terminal_relays.lock().await.insert(session_id, cancel) {
            let _ = previous.send(());
        }
        tauri::async_runtime::spawn(async move {
            loop {
                let next = tokio::select! {
                    _ = &mut cancelled => break,
                    next = read_frame(&mut stream) => next,
                };
                let Ok(event) = next else {
                    break;
                };
                if app
                    .emit_to(EventTarget::webview("main"), "daemon:event", event)
                    .is_err()
                {
                    break;
                }
            }
        });
        Ok(response)
    }

    async fn unsubscribe_terminal(&self, session_id: &str) {
        if let Some(cancel) = self.terminal_relays.lock().await.remove(session_id) {
            let _ = cancel.send(());
        }
    }
}

fn require_main_caller(caller: &Webview) -> Result<(), BridgeError> {
    if is_main_webview(caller) {
        Ok(())
    } else {
        Err(BridgeError::forbidden())
    }
}

async fn send_request(socket_path: &Path, request: &Value) -> Result<Value, BridgeFailure> {
    let encoded = encode_request(request)?;
    let mut stream = UnixStream::connect(socket_path)
        .await
        .map_err(BridgeFailure::Connect)?;
    exchange_frame(&mut stream, &encoded).await
}

async fn open_subscription(
    socket_path: &Path,
    request: &Value,
) -> Result<(UnixStream, Value), BridgeFailure> {
    let encoded = encode_request(request)?;
    let mut stream = UnixStream::connect(socket_path)
        .await
        .map_err(BridgeFailure::Connect)?;
    write_frame(&mut stream, &encoded).await?;
    let response = read_frame(&mut stream).await?;
    Ok((stream, response))
}

fn encode_request(request: &Value) -> Result<Vec<u8>, BridgeFailure> {
    let encoded = serde_json::to_vec(request).map_err(BridgeFailure::Encode)?;
    if encoded.len() > MAX_FRAME_LENGTH {
        return Err(BridgeFailure::FrameTooLarge(encoded.len()));
    }
    Ok(encoded)
}

async fn exchange_frame<Stream>(
    stream: &mut Stream,
    encoded_request: &[u8],
) -> Result<Value, BridgeFailure>
where
    Stream: AsyncRead + AsyncWrite + Unpin,
{
    write_frame(stream, encoded_request).await?;
    read_frame(stream).await
}

async fn write_frame<Stream>(
    stream: &mut Stream,
    encoded_request: &[u8],
) -> Result<(), BridgeFailure>
where
    Stream: AsyncWrite + Unpin,
{
    let frame_length = u32::try_from(encoded_request.len())
        .map_err(|_| BridgeFailure::FrameTooLarge(encoded_request.len()))?;
    stream
        .write_all(&frame_length.to_be_bytes())
        .await
        .map_err(|error| BridgeFailure::Io("write frame length", error))?;
    stream
        .write_all(encoded_request)
        .await
        .map_err(|error| BridgeFailure::Io("write request frame", error))?;
    Ok(())
}

async fn read_frame<Stream>(stream: &mut Stream) -> Result<Value, BridgeFailure>
where
    Stream: AsyncRead + Unpin,
{
    let mut frame_header = [0_u8; 4];
    stream
        .read_exact(&mut frame_header)
        .await
        .map_err(|error| BridgeFailure::Io("read frame length", error))?;
    let response_length = u32::from_be_bytes(frame_header) as usize;
    if response_length > MAX_FRAME_LENGTH {
        return Err(BridgeFailure::FrameTooLarge(response_length));
    }

    let mut response = vec![0_u8; response_length];
    stream
        .read_exact(&mut response)
        .await
        .map_err(|error| BridgeFailure::Io("read response frame", error))?;
    serde_json::from_slice(&response).map_err(BridgeFailure::Decode)
}

fn start_daemon() -> Result<(), String> {
    let executable = daemon_executable()?;
    Command::new(&executable)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("could not start {}: {error}", executable.display()))
}

fn daemon_executable() -> Result<PathBuf, String> {
    if let Some(configured) = env::var_os(DAEMON_PATH_ENV).filter(|value| !value.is_empty()) {
        let path = PathBuf::from(configured);
        if path.is_file() {
            return Ok(path);
        }
        return Err(format!(
            "{DAEMON_PATH_ENV} does not point to a file: {}",
            path.display()
        ));
    }

    let desktop_executable = env::current_exe()
        .map_err(|error| format!("could not resolve desktop executable: {error}"))?;
    let daemon_name = format!("cli-masterd{}", env::consts::EXE_SUFFIX);
    let sibling = desktop_executable.with_file_name(daemon_name);
    if sibling.is_file() {
        return Ok(sibling);
    }

    Err(format!(
        "daemon executable was not found beside the desktop executable: {}",
        sibling.display()
    ))
}

#[derive(Debug)]
enum BridgeFailure {
    Connect(io::Error),
    Io(&'static str, io::Error),
    Encode(serde_json::Error),
    Decode(serde_json::Error),
    FrameTooLarge(usize),
    MissingResponse,
}

impl BridgeFailure {
    const fn is_connection_failure(&self) -> bool {
        matches!(self, Self::Connect(_))
    }
}

impl fmt::Display for BridgeFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connect(error) => write!(formatter, "connect to daemon socket: {error}"),
            Self::Io(operation, error) => write!(formatter, "{operation}: {error}"),
            Self::Encode(error) => write!(formatter, "encode request: {error}"),
            Self::Decode(error) => write!(formatter, "decode response: {error}"),
            Self::FrameTooLarge(length) => write!(
                formatter,
                "daemon frame contains {length} bytes; maximum is {MAX_FRAME_LENGTH}"
            ),
            Self::MissingResponse => formatter.write_str("daemon returned no response"),
        }
    }
}

/// Stable error shape consumed by the frontend IPC client.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BridgeError {
    code: &'static str,
    message: &'static str,
    action: &'static str,
    details: Value,
}

impl BridgeError {
    fn forbidden() -> Self {
        Self {
            code: "desktop_bridge_forbidden",
            message: "Only the main application webview may access the daemon bridge.",
            action: "Return to the main Jig interface and retry the operation.",
            details: json!({}),
        }
    }

    fn startup(reason: impl fmt::Display) -> Self {
        Self {
            code: "daemon_start_failed",
            message: "The local daemon could not be started.",
            action: "Build cli-masterd or set CLI_MASTER_DAEMON_PATH, then retry the connection.",
            details: json!({ "reason": reason.to_string() }),
        }
    }

    fn unavailable(operation: &'static str, reason: impl fmt::Display) -> Self {
        Self {
            code: "daemon_unavailable",
            message: "The local daemon could not be reached.",
            action: "Retry the connection or open Diagnostics for startup details.",
            details: json!({
                "operation": operation,
                "reason": reason.to_string(),
            }),
        }
    }

    fn invalid_request(reason: impl fmt::Display) -> Self {
        Self {
            code: "invalid_terminal_subscription",
            message: "The terminal subscription request is invalid.",
            action: "Close the terminal card and open it again.",
            details: json!({ "reason": reason.to_string() }),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn exchanges_one_length_delimited_json_frame() {
        let expected_request = json!({
            "kind": "request",
            "version": 1,
            "requestId": "01900000-0000-7000-8000-000000000001",
            "method": "system.hello",
            "payload": {},
        });
        let expected_response = json!({
            "kind": "response",
            "version": 1,
            "requestId": "01900000-0000-7000-8000-000000000001",
            "status": "success",
            "data": { "protocolVersion": 1 },
        });
        let request_for_server = expected_request.clone();
        let response_for_server = expected_response.clone();
        let encoded_request = encode_request(&expected_request).expect("encode bridge request");
        let (mut bridge_stream, mut daemon_stream) = tokio::io::duplex(4096);

        let server = tokio::spawn(async move {
            let mut header = [0_u8; 4];
            daemon_stream
                .read_exact(&mut header)
                .await
                .expect("read request length");
            let mut request = vec![0_u8; u32::from_be_bytes(header) as usize];
            daemon_stream
                .read_exact(&mut request)
                .await
                .expect("read request frame");
            let decoded: Value = serde_json::from_slice(&request).expect("decode request frame");
            assert_eq!(decoded, request_for_server);

            let response = serde_json::to_vec(&response_for_server).expect("encode response");
            let response_length = u32::try_from(response.len()).expect("response fits frame");
            daemon_stream
                .write_all(&response_length.to_be_bytes())
                .await
                .expect("write response length");
            daemon_stream
                .write_all(&response)
                .await
                .expect("write response frame");
        });

        let response = exchange_frame(&mut bridge_stream, &encoded_request)
            .await
            .expect("bridge request succeeds");

        assert_eq!(response, expected_response);
        server.await.expect("mock daemon task succeeds");
    }

    #[tokio::test]
    async fn rejects_request_larger_than_the_daemon_limit_before_connecting() {
        let oversized_request = Value::String("x".repeat(MAX_FRAME_LENGTH));

        let error = encode_request(&oversized_request).expect_err("oversized request must fail");

        assert!(matches!(error, BridgeFailure::FrameTooLarge(_)));
    }
}
