use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cli_master_core::ResponseEnvelope;
use cli_master_core::wire::HelloResponse;
use cli_master_daemon::DaemonConfig;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot, watch};

use super::backoff::BoundedBackoff;
use super::error::BridgeError;
use super::method::is_wire_method_name;
use super::relay::BridgeRelay;
use super::session::ConnectedClient;
use super::sidecar::{SidecarMode, connect_socket};
use super::status::DaemonStatus;

const DEFAULT_SPAWN_BUDGET: u32 = 3;

/// Time limits applied to connect, handshake, and forwarded requests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Timeouts {
    /// Limit for the Unix `connect(2)` call.
    pub connect: Duration,
    /// Limit for `system.hello`.
    pub handshake: Duration,
    /// Limit for a forwarded request waiting on its correlated response.
    pub request: Duration,
    /// How long to wait for a spawned sidecar to accept connections.
    pub sidecar_ready: Duration,
}

impl Timeouts {
    /// Production timeouts: one-second connect, two-second hello, ten-second RPC.
    #[must_use]
    pub const fn production() -> Self {
        Self {
            connect: Duration::from_secs(1),
            handshake: Duration::from_secs(2),
            request: Duration::from_secs(10),
            sidecar_ready: Duration::from_secs(3),
        }
    }
}

impl Default for Timeouts {
    fn default() -> Self {
        Self::production()
    }
}

/// Runtime configuration for the reconnecting daemon client.
#[derive(Clone, Debug)]
pub struct BridgeOptions {
    /// Path of `daemon.sock` from [`DaemonConfig`].
    pub socket_path: PathBuf,
    /// Connect, handshake, and RPC timeouts.
    pub timeouts: Timeouts,
    /// Bounded reconnect delay.
    pub backoff: BoundedBackoff,
    /// Whether a missing socket may start `cli-masterd`.
    pub sidecar: SidecarMode,
}

impl BridgeOptions {
    /// Discovers the per-user socket and uses production sidecar policy.
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError::Unavailable`] when data or runtime directories
    /// cannot be resolved.
    pub fn discover() -> Result<Self, BridgeError> {
        let config = DaemonConfig::discover().map_err(|error| BridgeError::Unavailable {
            detail: error.to_string(),
        })?;
        Ok(Self {
            socket_path: config.socket_path().to_path_buf(),
            timeouts: Timeouts::production(),
            backoff: BoundedBackoff::new(),
            sidecar: SidecarMode::SpawnIfMissing,
        })
    }
}

enum Command {
    Invoke {
        method: String,
        payload: Value,
        deadline: Instant,
        tx: oneshot::Sender<Result<ResponseEnvelope<Value>, BridgeError>>,
    },
    Reconnect,
    Shutdown,
}

/// Reconnecting multiplexed client used by generic Tauri commands.
#[derive(Clone)]
pub struct DaemonBridge {
    commands: mpsc::UnboundedSender<Command>,
    status: watch::Receiver<DaemonStatus>,
    request_timeout: Duration,
}

impl DaemonBridge {
    /// Starts the actor that owns connect, handshake, and reconnect.
    #[must_use]
    pub fn spawn(options: BridgeOptions, relay: Arc<dyn BridgeRelay>) -> Self {
        let (commands, command_rx) = mpsc::unbounded_channel();
        let (status_tx, status) = watch::channel(DaemonStatus::Connecting);
        let request_timeout = options.timeouts.request;
        spawn_actor(run_actor(command_rx, options, relay, status_tx));
        Self {
            commands,
            status,
            request_timeout,
        }
    }

    /// Forwards one wire request. Application errors stay in the envelope.
    ///
    /// # Errors
    ///
    /// Returns a transport [`BridgeError`] when the method name is invalid or
    /// the socket cannot complete the round trip.
    pub async fn invoke(
        &self,
        method: String,
        payload: Value,
    ) -> Result<ResponseEnvelope<Value>, BridgeError> {
        if !is_wire_method_name(&method) {
            return Err(BridgeError::InvalidMethod { method });
        }
        let (tx, rx) = oneshot::channel();
        self.commands
            .send(Command::Invoke {
                method: method.clone(),
                payload,
                deadline: Instant::now() + self.request_timeout,
                tx,
            })
            .map_err(|_| BridgeError::Disconnected {
                method: method.clone(),
                request_id: cli_master_core::RequestId::new(),
            })?;
        match tokio::time::timeout(self.request_timeout, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(BridgeError::Disconnected {
                method,
                request_id: cli_master_core::RequestId::new(),
            }),
            Err(_) => Err(BridgeError::Timeout {
                method,
                request_id: cli_master_core::RequestId::new(),
            }),
        }
    }

    /// Latest observed connection status.
    #[must_use]
    pub fn status(&self) -> DaemonStatus {
        self.status.borrow().clone()
    }

    /// Subscribes to status changes.
    #[cfg(test)]
    #[must_use]
    pub fn subscribe_status(&self) -> watch::Receiver<DaemonStatus> {
        self.status.clone()
    }

    /// Clears backoff or incompatibility and starts a new connect attempt.
    pub fn request_reconnect(&self) {
        let _ = self.commands.send(Command::Reconnect);
    }

    /// Drops the client socket without signaling the daemon process.
    pub fn shutdown(&self) {
        let _ = self.commands.send(Command::Shutdown);
    }
}

fn spawn_actor<F>(future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            handle.spawn(future);
        }
        Err(_) => {
            tauri::async_runtime::spawn(future);
        }
    }
}

fn publish(status_tx: &watch::Sender<DaemonStatus>, relay: &dyn BridgeRelay, status: DaemonStatus) {
    relay.status(status.clone());
    let _ = status_tx.send(status);
}

async fn run_actor(
    mut commands: mpsc::UnboundedReceiver<Command>,
    mut options: BridgeOptions,
    relay: Arc<dyn BridgeRelay>,
    status_tx: watch::Sender<DaemonStatus>,
) {
    let mut spawn_budget = DEFAULT_SPAWN_BUDGET;
    let mut shutting_down = false;
    while !shutting_down {
        publish(&status_tx, relay.as_ref(), DaemonStatus::Connecting);
        match connect_and_handshake(&options, Arc::clone(&relay), &mut spawn_budget).await {
            Ok((session, hello)) => {
                options.backoff.reset();
                spawn_budget = DEFAULT_SPAWN_BUDGET;
                publish(
                    &status_tx,
                    relay.as_ref(),
                    DaemonStatus::Ready {
                        protocol_version: hello.protocol_version,
                        daemon_version: hello.daemon_version,
                        instance_id: hello.instance_id,
                    },
                );
                shutting_down = ready_loop(&session, &mut commands).await;
            }
            Err(error @ BridgeError::Incompatible { .. }) => {
                publish(&status_tx, relay.as_ref(), incompatible_status(&error));
                shutting_down = wait_for_reconnect(&mut commands, &error).await;
                options.backoff.reset();
            }
            Err(error) => {
                if let Some(delay) = options.backoff.next_delay() {
                    publish(
                        &status_tx,
                        relay.as_ref(),
                        DaemonStatus::Reconnecting {
                            attempt: options.backoff.attempt(),
                        },
                    );
                    shutting_down = sleep_or_interrupt(delay, &mut commands, &error).await;
                } else {
                    publish(
                        &status_tx,
                        relay.as_ref(),
                        DaemonStatus::Unavailable {
                            detail: error_detail(&error),
                        },
                    );
                    shutting_down = wait_for_reconnect(&mut commands, &error).await;
                    options.backoff.reset();
                }
            }
        }
    }
}

async fn connect_and_handshake(
    options: &BridgeOptions,
    relay: Arc<dyn BridgeRelay>,
    spawn_budget: &mut u32,
) -> Result<(Arc<ConnectedClient>, HelloResponse), BridgeError> {
    let stream = connect_socket(
        &options.socket_path,
        options.sidecar,
        options.timeouts.connect,
        options.timeouts.sidecar_ready,
        spawn_budget,
    )
    .await?;
    let session = ConnectedClient::from_stream(stream, relay);
    match session.handshake(options.timeouts.handshake).await {
        Ok(hello) => Ok((session, hello)),
        Err(error) => {
            drop(session);
            Err(error)
        }
    }
}

async fn ready_loop(
    session: &Arc<ConnectedClient>,
    commands: &mut mpsc::UnboundedReceiver<Command>,
) -> bool {
    loop {
        tokio::select! {
            command = commands.recv() => match command {
                None | Some(Command::Shutdown) => return true,
                Some(Command::Reconnect) => return false,
                Some(Command::Invoke { method, payload, deadline, tx }) => {
                    if tx.is_closed() || Instant::now() > deadline {
                        let _ = tx.send(Err(BridgeError::Timeout {
                            method,
                            request_id: cli_master_core::RequestId::new(),
                        }));
                        continue;
                    }
                    let session = Arc::clone(session);
                    let request_timeout = deadline.saturating_duration_since(Instant::now());
                    tokio::spawn(async move {
                        let result = session.invoke(method, payload, request_timeout).await;
                        let _ = tx.send(result);
                    });
                }
            },
            () = session.wait_closed() => return false,
        }
    }
}

async fn wait_for_reconnect(
    commands: &mut mpsc::UnboundedReceiver<Command>,
    sticky: &BridgeError,
) -> bool {
    loop {
        match commands.recv().await {
            None | Some(Command::Shutdown) => return true,
            Some(Command::Reconnect) => return false,
            Some(Command::Invoke { tx, .. }) => {
                let _ = tx.send(Err(sticky.clone()));
            }
        }
    }
}

async fn sleep_or_interrupt(
    delay: Duration,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    sticky: &BridgeError,
) -> bool {
    let sleep = tokio::time::sleep(delay);
    tokio::pin!(sleep);
    loop {
        tokio::select! {
            () = &mut sleep => return false,
            command = commands.recv() => match command {
                None | Some(Command::Shutdown) => return true,
                Some(Command::Reconnect) => return false,
                Some(Command::Invoke { tx, .. }) => {
                    let _ = tx.send(Err(sticky.clone()));
                }
            }
        }
    }
}

fn incompatible_status(error: &BridgeError) -> DaemonStatus {
    match error {
        BridgeError::Incompatible {
            protocol_version,
            daemon_version,
            detail,
        } => DaemonStatus::Incompatible {
            protocol_version: *protocol_version,
            daemon_version: daemon_version.clone(),
            detail: detail.clone(),
        },
        _ => DaemonStatus::Unavailable {
            detail: error_detail(error),
        },
    }
}

fn error_detail(error: &BridgeError) -> String {
    match error {
        BridgeError::Unavailable { detail }
        | BridgeError::Protocol { detail }
        | BridgeError::Incompatible { detail, .. } => detail.clone(),
        other => other.code().to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    use cli_master_core::PROTOCOL_V1;
    use serde_json::json;
    use tokio::net::UnixListener;

    use super::{BridgeOptions, DaemonBridge, Timeouts};
    use crate::bridge::backoff::BoundedBackoff;
    use crate::bridge::error::BridgeError;
    use crate::bridge::relay::NoopRelay;
    use crate::bridge::sidecar::SidecarMode;
    use crate::bridge::status::DaemonStatus;
    use crate::bridge::test_support::{read_request, temp_socket, write_hello, write_response};

    fn test_options(socket: std::path::PathBuf) -> BridgeOptions {
        BridgeOptions {
            socket_path: socket,
            timeouts: Timeouts {
                connect: Duration::from_millis(200),
                handshake: Duration::from_millis(400),
                request: Duration::from_millis(400),
                sidecar_ready: Duration::from_millis(200),
            },
            backoff: BoundedBackoff::with_limits(
                Duration::from_millis(20),
                Duration::from_millis(40),
                8,
            ),
            sidecar: SidecarMode::Disabled,
        }
    }

    async fn wait_status<F>(bridge: &DaemonBridge, predicate: F) -> DaemonStatus
    where
        F: Fn(&DaemonStatus) -> bool,
    {
        let mut rx = bridge.subscribe_status();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if predicate(&rx.borrow()) {
                    return rx.borrow().clone();
                }
                tokio::select! {
                    result = rx.changed() => {
                        result.expect("status watch");
                    }
                    () = tokio::time::sleep(Duration::from_millis(15)) => {}
                }
            }
        })
        .await
        .expect("status should change")
    }

    #[tokio::test]
    async fn reconnects_after_the_server_drops_the_socket() {
        let (_dir, socket) = temp_socket();
        let listener = UnixListener::bind(&socket).expect("bind");
        let accepts = Arc::new(AtomicU32::new(0));
        let server_accepts = Arc::clone(&accepts);
        let server = tokio::spawn(async move {
            for round in 0..2_u32 {
                let (mut stream, _) = listener.accept().await.expect("accept");
                server_accepts.fetch_add(1, Ordering::SeqCst);
                let hello = read_request(&mut stream).await;
                write_hello(&mut stream, hello.request_id, PROTOCOL_V1).await;
                if round == 0 {
                    tokio::time::sleep(Duration::from_millis(80)).await;
                    drop(stream);
                    continue;
                }
                let request = read_request(&mut stream).await;
                write_response(&mut stream, request.request_id, json!({"ok": true})).await;
                std::future::pending::<()>().await;
            }
        });

        let bridge = DaemonBridge::spawn(test_options(socket), Arc::new(NoopRelay));
        let first = wait_status(&bridge, |status| {
            matches!(status, DaemonStatus::Ready { .. }) && accepts.load(Ordering::SeqCst) >= 1
        })
        .await;
        assert!(matches!(first, DaemonStatus::Ready { .. }));
        let second = wait_status(&bridge, |status| {
            matches!(status, DaemonStatus::Ready { .. }) && accepts.load(Ordering::SeqCst) >= 2
        })
        .await;
        assert!(matches!(second, DaemonStatus::Ready { .. }));
        let response = bridge
            .invoke("state.snapshot".into(), json!({}))
            .await
            .expect("invoke after reconnect");
        assert!(matches!(
            response.payload,
            cli_master_core::ResponsePayload::Success { .. }
        ));
        assert!(accepts.load(Ordering::SeqCst) >= 2);
        bridge.shutdown();
        server.abort();
    }

    #[tokio::test]
    async fn incompatible_hello_does_not_retry() {
        let (_dir, socket) = temp_socket();
        let listener = UnixListener::bind(&socket).expect("bind");
        let accepts = Arc::new(AtomicU32::new(0));
        let server_accepts = Arc::clone(&accepts);
        let server = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                server_accepts.fetch_add(1, Ordering::SeqCst);
                let hello = read_request(&mut stream).await;
                write_hello(&mut stream, hello.request_id, PROTOCOL_V1 + 1).await;
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        });

        let bridge = DaemonBridge::spawn(test_options(socket), Arc::new(NoopRelay));
        wait_status(&bridge, |status| {
            matches!(status, DaemonStatus::Incompatible { .. })
        })
        .await;
        tokio::time::sleep(Duration::from_millis(250)).await;
        assert_eq!(accepts.load(Ordering::SeqCst), 1);
        let error = bridge
            .invoke("state.snapshot".into(), json!({}))
            .await
            .expect_err("sticky incompatible");
        assert!(matches!(error, BridgeError::Incompatible { .. }));
        bridge.shutdown();
        server.abort();
    }

    #[tokio::test]
    async fn missing_socket_is_unavailable_not_incompatible() {
        let (_dir, socket) = temp_socket();
        let mut options = test_options(socket);
        options.backoff =
            BoundedBackoff::with_limits(Duration::from_millis(10), Duration::from_millis(10), 2);
        let bridge = DaemonBridge::spawn(options, Arc::new(NoopRelay));
        wait_status(&bridge, |status| {
            matches!(status, DaemonStatus::Unavailable { .. })
        })
        .await;
        let error = bridge
            .invoke("system.hello".into(), json!({}))
            .await
            .expect_err("unavailable");
        assert!(matches!(error, BridgeError::Unavailable { .. }));
        bridge.shutdown();
    }

    #[tokio::test]
    async fn handshake_against_an_in_process_daemon() {
        let temporary = tempfile::TempDir::new().expect("temporary directory");
        let config = cli_master_daemon::DaemonConfig::from_paths(
            temporary.path().join("data"),
            temporary.path().join("run"),
        );
        let socket = config.socket_path().to_path_buf();
        let daemon = cli_master_daemon::Daemon::bind(config).expect("daemon should bind");
        let cancellation = tokio_util::sync::CancellationToken::new();
        let child = cancellation.clone();
        let task = tokio::spawn(async move { daemon.run(child).await });
        let mut options = test_options(socket);
        options.timeouts.connect = Duration::from_secs(1);
        options.timeouts.handshake = Duration::from_secs(2);
        options.timeouts.request = Duration::from_secs(2);
        let bridge = DaemonBridge::spawn(options, Arc::new(NoopRelay));
        wait_status(&bridge, |status| {
            matches!(status, DaemonStatus::Ready { .. })
        })
        .await;
        let response = bridge
            .invoke("system.hello".into(), json!({}))
            .await
            .expect("hello through the bridge");
        assert!(matches!(
            response.payload,
            cli_master_core::ResponsePayload::Success { .. }
        ));
        bridge.shutdown();
        cancellation.cancel();
        task.await
            .expect("daemon task should join")
            .expect("daemon should stop");
    }

    #[tokio::test]
    async fn invalid_method_is_rejected_locally() {
        let (_dir, socket) = temp_socket();
        let bridge = DaemonBridge::spawn(test_options(socket), Arc::new(NoopRelay));
        let error = bridge
            .invoke("not a method".into(), json!({}))
            .await
            .expect_err("invalid method");
        assert!(matches!(error, BridgeError::InvalidMethod { .. }));
        bridge.shutdown();
    }
}
