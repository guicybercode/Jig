use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use cli_master_core::{
    EnvelopeKind, PROTOCOL_V1, Project, RequestEnvelope, RequestId, ResponseEnvelope,
    ResponsePayload, wire::DiagnosticsResponse,
};
use cli_master_daemon::{
    Daemon, DaemonConfig, DaemonError, HelloResponse, MAX_FRAME_LENGTH, StateSnapshot,
};
use cli_master_storage::LATEST_SCHEMA_VERSION;
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
    instance_id: uuid::Uuid,
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

fn create_git_repository(path: &Path) {
    fs::create_dir(path).expect("repository directory should exist");
    let output = Command::new("git")
        .args(["init", "--initial-branch=main"])
        .current_dir(path)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("Git should start");
    assert!(
        output.status.success(),
        "Git init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn handshake_is_exact_and_connection_accepts_multiple_requests() {
    let temporary = TempDir::new().expect("temporary directory should exist");
    let daemon = RunningDaemon::start(temporary.path());
    let mut client = connect(daemon.config.socket_path()).await;

    let request = RequestEnvelope::v1("system.hello", json!({}));
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
            "daemonVersion": env!("CARGO_PKG_VERSION"),
            "instanceId": daemon.instance_id,
        })
    );
    let hello: HelloResponse = serde_json::from_value(data).expect("typed hello should decode");
    assert_eq!(hello.instance_id, daemon.instance_id);

    let second = RequestEnvelope::v1("system.hello", json!(null));
    assert!(matches!(
        exchange(&mut client, &second).await.payload,
        ResponsePayload::Success { .. }
    ));
    daemon.stop().await;
}

#[tokio::test]
async fn snapshot_reports_applied_migration_and_typed_empty_collections() {
    let temporary = TempDir::new().expect("temporary directory should exist");
    let daemon = RunningDaemon::start(temporary.path());
    let mut client = connect(daemon.config.socket_path()).await;

    let response = exchange(
        &mut client,
        &RequestEnvelope::v1("state.snapshot", json!({})),
    )
    .await;
    let ResponsePayload::Success { data } = response.payload else {
        panic!("snapshot should succeed");
    };
    let snapshot: StateSnapshot = serde_json::from_value(data).expect("snapshot should decode");
    assert_eq!(snapshot.schema_version, LATEST_SCHEMA_VERSION);
    assert!(snapshot.projects.is_empty());
    assert!(snapshot.agents.is_empty());
    assert!(snapshot.sessions.is_empty());
    assert!(snapshot.worktrees.is_empty());

    daemon.stop().await;
}

#[tokio::test]
async fn diagnostics_report_sanitized_runtime_metadata() {
    let temporary = TempDir::new().expect("temporary directory should exist");
    let daemon = RunningDaemon::start(temporary.path());
    let mut client = connect(daemon.config.socket_path()).await;

    let response = exchange(
        &mut client,
        &RequestEnvelope::v1("diagnostics.get", json!({})),
    )
    .await;
    let ResponsePayload::Success { data } = response.payload else {
        panic!("diagnostics should succeed");
    };
    let diagnostics: DiagnosticsResponse =
        serde_json::from_value(data).expect("diagnostics should decode");

    assert_eq!(diagnostics.protocol_version, PROTOCOL_V1);
    assert_eq!(diagnostics.schema_version, LATEST_SCHEMA_VERSION);
    assert_eq!(
        diagnostics.daemon_instance_id.into_uuid(),
        daemon.instance_id,
    );
    assert_eq!(diagnostics.data_path, daemon.config.data_directory());
    assert_eq!(diagnostics.runtime_path, daemon.config.runtime_directory());
    assert_eq!(
        diagnostics.log_path,
        daemon.config.data_directory().join("logs"),
    );
    assert!(diagnostics.log_path.is_dir());
    assert!(
        diagnostics
            .effective_path
            .iter()
            .all(|path| path.is_absolute())
    );
    assert!(diagnostics.recent_issues.is_empty());

    daemon.stop().await;
}

#[tokio::test]
async fn project_registration_is_validated_persisted_and_mutable() {
    let temporary = TempDir::new().expect("temporary directory should exist");
    let repository = temporary.path().join("repository");
    create_git_repository(&repository);
    let daemon = RunningDaemon::start(temporary.path());
    let mut client = connect(daemon.config.socket_path()).await;

    let response = exchange(
        &mut client,
        &RequestEnvelope::v1(
            "project.add",
            json!({ "path": repository.to_string_lossy() }),
        ),
    )
    .await;
    let ResponsePayload::Success { data } = response.payload else {
        panic!("project registration should succeed");
    };
    let project: Project = serde_json::from_value(data).expect("project should decode");
    assert_eq!(project.name, "repository");
    assert_eq!(project.path, repository.canonicalize().unwrap());
    assert_eq!(project.repository_root, Some(project.path.clone()));
    assert_eq!(project.current_branch.as_deref(), Some("main"));

    let duplicate = exchange(
        &mut client,
        &RequestEnvelope::v1(
            "project.add",
            json!({ "path": repository.to_string_lossy() }),
        ),
    )
    .await;
    assert_eq!(failure_code(duplicate), "project_already_registered");
    daemon.stop().await;

    let daemon = RunningDaemon::start(temporary.path());
    let mut client = connect(daemon.config.socket_path()).await;
    let response = exchange(
        &mut client,
        &RequestEnvelope::v1("state.snapshot", json!({})),
    )
    .await;
    let ResponsePayload::Success { data } = response.payload else {
        panic!("snapshot should succeed");
    };
    let snapshot: StateSnapshot = serde_json::from_value(data).expect("snapshot should decode");
    assert_eq!(snapshot.projects, vec![project.clone()]);

    let response = exchange(
        &mut client,
        &RequestEnvelope::v1(
            "project.rename",
            json!({ "projectId": project.id, "name": "Renamed project" }),
        ),
    )
    .await;
    let ResponsePayload::Success { data } = response.payload else {
        panic!("project rename should succeed");
    };
    let renamed: Project = serde_json::from_value(data).expect("renamed project should decode");
    assert_eq!(renamed.name, "Renamed project");

    let response = exchange(&mut client, &RequestEnvelope::v1("project.list", json!({}))).await;
    let ResponsePayload::Success { data } = response.payload else {
        panic!("project list should succeed");
    };
    assert_eq!(data["projects"][0]["name"], json!("Renamed project"));

    let response = exchange(
        &mut client,
        &RequestEnvelope::v1("project.remove", json!({ "projectId": project.id })),
    )
    .await;
    assert!(matches!(response.payload, ResponsePayload::Success { .. }));

    let response = exchange(&mut client, &RequestEnvelope::v1("project.list", json!({}))).await;
    let ResponsePayload::Success { data } = response.payload else {
        panic!("project list should succeed");
    };
    assert_eq!(data["projects"], json!([]));
    daemon.stop().await;
}

#[tokio::test]
async fn project_registration_accepts_a_plain_folder() {
    let temporary = TempDir::new().expect("temporary directory should exist");
    let folder = temporary.path().join("plain-folder");
    fs::create_dir(&folder).expect("plain folder should exist");
    let daemon = RunningDaemon::start(temporary.path());
    let mut client = connect(daemon.config.socket_path()).await;

    let response = exchange(
        &mut client,
        &RequestEnvelope::v1("project.add", json!({ "path": folder.to_string_lossy() })),
    )
    .await;
    let ResponsePayload::Success { data } = response.payload else {
        panic!("a plain project folder should be accepted");
    };
    let project: Project = serde_json::from_value(data).expect("project should decode");
    assert_eq!(project.name, "plain-folder");
    assert_eq!(project.path, folder.canonicalize().unwrap());
    assert_eq!(project.repository_root, None);
    assert_eq!(project.current_branch, None);
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
        method: "system.hello".to_owned(),
        payload: json!({}),
    };
    assert_eq!(
        failure_code(exchange(&mut client, &wrong_kind).await),
        "invalid_envelope_kind"
    );

    let wrong_version = RequestEnvelope {
        kind: EnvelopeKind::Request,
        version: PROTOCOL_V1 + 1,
        request_id: RequestId::new(),
        method: "system.hello".to_owned(),
        payload: json!({}),
    };
    assert_eq!(
        failure_code(exchange(&mut client, &wrong_version).await),
        "unsupported_protocol_version"
    );

    let unknown = RequestEnvelope::v1("unknown.method", json!({}));
    assert_eq!(
        failure_code(exchange(&mut client, &unknown).await),
        "method_not_found"
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
    let response = exchange(
        &mut reconnected,
        &RequestEnvelope::v1("system.hello", json!({})),
    )
    .await;
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
        exchange(&mut client, &RequestEnvelope::v1("system.hello", json!({})))
            .await
            .payload,
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
