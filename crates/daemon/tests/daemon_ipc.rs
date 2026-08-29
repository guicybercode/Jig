use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

use cli_master_core::{
    DaemonInstanceId, EnvelopeKind, IpcMethod, PROTOCOL_V1, RequestEnvelope, RequestId,
    ResponseEnvelope, ResponsePayload, error_codes,
};
use cli_master_daemon::{
    Daemon, DaemonConfig, DaemonError, HelloResponse, MAX_FRAME_LENGTH, StateSnapshot,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;
use tokio::task::JoinHandle;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tokio_util::sync::CancellationToken;

struct RunningDaemon {
    config: DaemonConfig,
    instance_id: DaemonInstanceId,
    cancellation: CancellationToken,
    task: JoinHandle<Result<(), DaemonError>>,
}

impl RunningDaemon {
    fn start(root: &Path) -> Self {
        let config = DaemonConfig::from_paths(root.join("data"), root.join("run"));
        let daemon = Daemon::bind(config.clone()).expect("daemon should bind");
        let instance_id = daemon.instance_id();
        let cancellation = CancellationToken::new();
        let task_cancellation = cancellation.clone();
        let task = tokio::spawn(async move { daemon.run(task_cancellation).await });
        Self {
            config,
            instance_id,
            cancellation,
            task,
        }
    }

    async fn stop(self) {
        self.cancellation.cancel();
        self.task
            .await
            .expect("daemon task should join")
            .expect("daemon should stop cleanly");
    }
}

fn framed(stream: UnixStream) -> Framed<UnixStream, LengthDelimitedCodec> {
    LengthDelimitedCodec::builder()
        .max_frame_length(MAX_FRAME_LENGTH)
        .new_framed(stream)
}

async fn connect(path: &Path) -> Framed<UnixStream, LengthDelimitedCodec> {
    let stream = UnixStream::connect(path)
        .await
        .expect("client should connect");
    framed(stream)
}

async fn exchange(
    client: &mut Framed<UnixStream, LengthDelimitedCodec>,
    request: &RequestEnvelope<Value>,
) -> ResponseEnvelope<Value> {
    client
        .send(
            serde_json::to_vec(request)
                .expect("request should encode")
                .into(),
        )
        .await
        .expect("request should send");
    let bytes = client
        .next()
        .await
        .expect("response frame should arrive")
        .expect("response frame should be valid");
    serde_json::from_slice(&bytes).expect("response should decode")
}

fn failure_code(response: ResponseEnvelope<Value>) -> String {
    match response.payload {
        ResponsePayload::Error { error } => error.code,
        ResponsePayload::Success { data } => panic!("expected failure, received {data}"),
    }
}

fn hello_request() -> RequestEnvelope<Value> {
    RequestEnvelope::v1(
        IpcMethod::SystemHello,
        json!({
            "protocolVersion": PROTOCOL_V1,
            "client": "daemon-integration-test",
        }),
    )
}

#[tokio::test]
async fn handshake_is_exact_and_connection_accepts_multiple_requests() {
    let temporary = TempDir::new().expect("temporary directory should exist");
    let daemon = RunningDaemon::start(temporary.path());
    let mut client = connect(daemon.config.socket_path()).await;

    let request = hello_request();
    let response = exchange(&mut client, &request).await;
    assert_eq!(response.kind, EnvelopeKind::Response);
    assert_eq!(response.version, PROTOCOL_V1);
    assert_eq!(response.request_id, request.request_id);
    let ResponsePayload::Success { data } = response.payload else {
        panic!("hello should succeed");
    };
    assert_eq!(
        data,
        json!({
            "protocolVersion": PROTOCOL_V1,
            "daemonInstanceId": daemon.instance_id,
            "appVersion": env!("CARGO_PKG_VERSION"),
            "platform": std::env::consts::OS,
        })
    );
    let hello: HelloResponse = serde_json::from_value(data).expect("typed hello should decode");
    assert_eq!(hello.daemon_instance_id, daemon.instance_id);

    let second = hello_request();
    assert!(matches!(
        exchange(&mut client, &second).await.payload,
        ResponsePayload::Success { .. }
    ));
    daemon.stop().await;
}

#[tokio::test]
async fn snapshot_reports_daemon_identity_and_typed_empty_collections() {
    let temporary = TempDir::new().expect("temporary directory should exist");
    let daemon = RunningDaemon::start(temporary.path());
    let mut client = connect(daemon.config.socket_path()).await;

    let response = exchange(
        &mut client,
        &RequestEnvelope::v1(IpcMethod::StateSnapshot, json!({})),
    )
    .await;
    let ResponsePayload::Success { data } = response.payload else {
        panic!("snapshot should succeed");
    };
    let snapshot: StateSnapshot = serde_json::from_value(data).expect("snapshot should decode");
    assert_eq!(snapshot.daemon.instance_id, daemon.instance_id);
    assert_eq!(snapshot.daemon.protocol_version, PROTOCOL_V1);
    assert!(snapshot.projects.is_empty());
    assert!(snapshot.agents.is_empty());
    assert!(snapshot.custom_agents.is_empty());
    assert!(snapshot.sessions.is_empty());
    assert!(snapshot.worktrees.is_empty());

    daemon.stop().await;
}

#[tokio::test]
async fn invalid_kind_version_and_method_return_correlated_errors() {
    let temporary = TempDir::new().expect("temporary directory should exist");
    let daemon = RunningDaemon::start(temporary.path());
    let mut client = connect(daemon.config.socket_path()).await;

    let wrong_kind = RequestEnvelope {
        kind: EnvelopeKind::Event,
        version: PROTOCOL_V1,
        request_id: RequestId::new(),
        method: IpcMethod::SystemHello,
        payload: hello_request().payload,
    };
    assert_eq!(
        failure_code(exchange(&mut client, &wrong_kind).await),
        error_codes::PROTOCOL_INVALID_PAYLOAD
    );

    let wrong_version = RequestEnvelope {
        kind: EnvelopeKind::Request,
        version: PROTOCOL_V1 + 1,
        request_id: RequestId::new(),
        method: IpcMethod::SystemHello,
        payload: hello_request().payload,
    };
    assert_eq!(
        failure_code(exchange(&mut client, &wrong_version).await),
        error_codes::PROTOCOL_UNSUPPORTED
    );

    let invalid_hello = RequestEnvelope::v1(IpcMethod::SystemHello, json!({}));
    assert_eq!(
        failure_code(exchange(&mut client, &invalid_hello).await),
        error_codes::PROTOCOL_INVALID_PAYLOAD
    );

    let unknown: RequestEnvelope<Value> = serde_json::from_value(json!({
        "kind": "request",
        "version": PROTOCOL_V1,
        "requestId": RequestId::new(),
        "method": "unknown.method",
        "payload": {},
    }))
    .expect("unknown method should decode to the sentinel variant");
    assert_eq!(
        failure_code(exchange(&mut client, &unknown).await),
        error_codes::PROTOCOL_UNKNOWN_METHOD
    );
    daemon.stop().await;
}

#[tokio::test]
async fn oversized_frame_only_disconnects_the_offending_client() {
    let temporary = TempDir::new().expect("temporary directory should exist");
    let daemon = RunningDaemon::start(temporary.path());
    let mut stream = UnixStream::connect(daemon.config.socket_path())
        .await
        .expect("client should connect");
    let oversized = u32::try_from(MAX_FRAME_LENGTH + 1).expect("limit should fit in prefix");
    stream
        .write_all(&oversized.to_be_bytes())
        .await
        .expect("oversized header should send");

    let closed = tokio::time::timeout(Duration::from_secs(2), stream.readable())
        .await
        .expect("server should react to oversized frame");
    assert!(closed.is_ok());
    let mut byte = [0_u8; 1];
    assert_eq!(
        stream
            .try_read(&mut byte)
            .expect("connection should be closed"),
        0
    );

    let mut reconnected = connect(daemon.config.socket_path()).await;
    let response = exchange(&mut reconnected, &hello_request()).await;
    assert!(matches!(response.payload, ResponsePayload::Success { .. }));
    daemon.stop().await;
}

#[tokio::test]
async fn lock_rejects_a_second_daemon_then_allows_sequential_reconnect() {
    let temporary = TempDir::new().expect("temporary directory should exist");
    let first = RunningDaemon::start(temporary.path());
    let config = first.config.clone();

    let Err(error) = Daemon::bind(config.clone()) else {
        panic!("second daemon must not bind");
    };
    assert!(matches!(error, DaemonError::AlreadyRunning { .. }));
    first.stop().await;
    assert!(!config.socket_path().exists());

    let second = Daemon::bind(config.clone()).expect("daemon should restart after lock release");
    let cancellation = CancellationToken::new();
    let child = cancellation.clone();
    let task = tokio::spawn(async move { second.run(child).await });
    let mut client = connect(config.socket_path()).await;
    assert!(matches!(
        exchange(&mut client, &hello_request()).await.payload,
        ResponsePayload::Success { .. }
    ));
    drop(client);
    cancellation.cancel();
    task.await
        .expect("daemon task should join")
        .expect("daemon should stop");
}

#[tokio::test]
async fn runtime_paths_and_socket_are_private() {
    let temporary = TempDir::new().expect("temporary directory should exist");
    let daemon = RunningDaemon::start(temporary.path());

    assert_eq!(mode(daemon.config.data_directory()), 0o700);
    assert_eq!(mode(daemon.config.runtime_directory()), 0o700);
    assert_eq!(mode(daemon.config.lock_path()), 0o600);
    assert_eq!(mode(daemon.config.socket_path()), 0o600);
    daemon.stop().await;
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path)
        .unwrap_or_else(|error| panic!("{} should have metadata: {error}", path.display()))
        .permissions()
        .mode()
        & 0o777
}

#[allow(dead_code)]
fn assert_io_error_is_send_sync(_: io::Error) {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<DaemonError>();
}
