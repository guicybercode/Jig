use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use cli_master_core::wire::{HelloResponse, StateSnapshotResponse, method};
use cli_master_core::{
    AgentSource, EnvelopeKind, EventEnvelope, PROTOCOL_V1, Project, RequestEnvelope, RequestId,
    ResponseEnvelope, ResponsePayload, Session, wire::DiagnosticsResponse,
};
use cli_master_daemon::{
    Daemon, DaemonConfig, DaemonError, HelloResponse, MAX_FRAME_LENGTH, StateSnapshot,
};
use cli_master_daemon::{Daemon, DaemonConfig, DaemonError, MAX_FRAME_LENGTH};
use cli_master_storage::{LATEST_SCHEMA_VERSION, Storage, StoredAgent, StoredSession};
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

    let request = RequestEnvelope::v1(method::SYSTEM_HELLO, json!({}));
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

    let second = RequestEnvelope::v1(method::SYSTEM_HELLO, json!({}));
    assert!(matches!(
        exchange(&mut client, &second).await.payload,
        ResponsePayload::Success { .. }
    ));
    let invalid = RequestEnvelope::v1(method::SYSTEM_HELLO, json!({ "unexpected": true }));
    assert_eq!(
        failure_code(exchange(&mut client, &invalid).await),
        "invalid_payload"
    );
    daemon.stop().await;
}

#[tokio::test]
async fn snapshot_reports_applied_migration_and_builtin_terminal_agents() {
    let temporary = TempDir::new().expect("temporary directory should exist");
    let daemon = RunningDaemon::start(temporary.path());
    let mut client = connect(daemon.config.socket_path()).await;

    let response = exchange(
        &mut client,
        &RequestEnvelope::v1(method::STATE_SNAPSHOT, json!({})),
    )
    .await;
    let ResponsePayload::Success { data } = response.payload else {
        panic!("snapshot should succeed");
    };
    let snapshot: StateSnapshotResponse =
        serde_json::from_value(data).expect("snapshot should decode");
    assert_eq!(snapshot.schema_version, LATEST_SCHEMA_VERSION);
    assert!(snapshot.projects.is_empty());
    assert_eq!(snapshot.agents.len(), 4);
    assert!(snapshot.agents.iter().all(|agent| agent.enabled));
    assert!(
        snapshot
            .agents
            .iter()
            .all(|agent| agent.source == AgentSource::BuiltIn)
    );
    assert!(
        snapshot
            .agents
            .iter()
            .any(|agent| agent.display_name.as_str() == "Shell")
    );
    assert!(snapshot.sessions.is_empty());
    assert!(snapshot.worktrees.is_empty());

    daemon.stop().await;
}

#[tokio::test]
async fn live_terminal_accepts_input_and_streams_pty_output() {
    let temporary = TempDir::new().expect("temporary directory should exist");
    let project_directory = temporary.path().join("terminal-project");
    fs::create_dir(&project_directory).expect("project directory should exist");
    let daemon = RunningDaemon::start(temporary.path());
    let mut control = connect(daemon.config.socket_path()).await;

    let project_response = exchange(
        &mut control,
        &RequestEnvelope::v1(
            "project.add",
            json!({ "path": project_directory.to_string_lossy() }),
        ),
    )
    .await;
    let ResponsePayload::Success { data } = project_response.payload else {
        panic!("project registration should succeed");
    };
    let project: Project = serde_json::from_value(data).expect("project should decode");

    let snapshot_response = exchange(
        &mut control,
        &RequestEnvelope::v1("state.snapshot", json!({})),
    )
    .await;
    let ResponsePayload::Success { data } = snapshot_response.payload else {
        panic!("snapshot should succeed");
    };
    let snapshot: StateSnapshot = serde_json::from_value(data).expect("snapshot should decode");
    let shell = snapshot
        .agents
        .iter()
        .find(|agent| agent.display_name.as_str() == "Shell")
        .expect("shell agent should be available");

    let create_response = exchange(
        &mut control,
        &RequestEnvelope::v1(
            "session.create",
            json!({
                "projectId": project.id,
                "name": "Terminal test",
                "agentId": shell.id,
                "isolation": "current"
            }),
        ),
    )
    .await;
    let ResponsePayload::Success { data } = create_response.payload else {
        panic!("session creation should succeed");
    };
    let created: Session = serde_json::from_value(data).expect("session should decode");

    let start_response = exchange(
        &mut control,
        &RequestEnvelope::v1("session.start", json!({ "sessionId": created.id })),
    )
    .await;
    let ResponsePayload::Success { data } = start_response.payload else {
        panic!("session start should succeed");
    };
    let started: Session = serde_json::from_value(data).expect("started session should decode");
    assert!(matches!(
        started.status,
        cli_master_core::SessionStatus::Starting | cli_master_core::SessionStatus::Running
    ));
    assert!(started.pid.is_some());

    let mut events = connect(daemon.config.socket_path()).await;
    let subscribe_response = exchange(
        &mut events,
        &RequestEnvelope::v1("session.subscribe", json!({ "sessionId": created.id })),
    )
    .await;
    assert!(matches!(
        subscribe_response.payload,
        ResponsePayload::Success { .. }
    ));

    let write_response = exchange(
        &mut control,
        &RequestEnvelope::v1(
            "session.write",
            json!({
                "sessionId": created.id,
                "base64": "ZWNobyBKSUdfUFRZX09LCg=="
            }),
        ),
    )
    .await;
    assert!(matches!(
        write_response.payload,
        ResponsePayload::Success { .. }
    ));

    let output = tokio::time::timeout(Duration::from_secs(5), async {
        let mut output = Vec::new();
        loop {
            let bytes = events
                .next()
                .await
                .expect("event frame should arrive")
                .expect("event frame should be valid");
            let event: EventEnvelope<Value> =
                serde_json::from_slice(&bytes).expect("event should decode");
            if event.event == "session.output" {
                let encoded = event.payload["base64"]
                    .as_str()
                    .expect("output should contain base64");
                output.extend(decode_test_base64(encoded));
                if output
                    .windows(b"JIG_PTY_OK".len())
                    .any(|part| part == b"JIG_PTY_OK")
                {
                    break output;
                }
            }
        }
    })
    .await
    .expect("terminal marker should arrive before timeout");
    assert!(
        output
            .windows(b"JIG_PTY_OK".len())
            .any(|part| part == b"JIG_PTY_OK")
    );

    let stop_response = exchange(
        &mut control,
        &RequestEnvelope::v1("session.stop", json!({ "sessionId": created.id })),
    )
    .await;
    assert!(matches!(
        stop_response.payload,
        ResponsePayload::Success { .. }
    ));
    drop(events);
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
        method: method::SYSTEM_HELLO.to_owned(),
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
        method: method::SYSTEM_HELLO.to_owned(),
        payload: json!({}),
    };
    assert_eq!(
        failure_code(exchange(&mut client, &wrong_version).await),
        "unsupported_protocol_version"
    );

    let known = RequestEnvelope::v1(method::PROJECT_LIST, json!({}));
    assert_eq!(
        failure_code(exchange(&mut client, &known).await),
        "method_not_implemented"
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
        &RequestEnvelope::v1(method::SYSTEM_HELLO, json!({})),
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
        exchange(
            &mut client,
            &RequestEnvelope::v1(method::SYSTEM_HELLO, json!({})),
        )
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
async fn bind_reconciles_stale_runtime_before_accepting_clients() {
    let temporary = TempDir::new().expect("temporary directory should exist");
    let config =
        DaemonConfig::from_paths(temporary.path().join("data"), temporary.path().join("run"));
    fs::create_dir_all(config.data_directory()).expect("data directory should exist");
    let project_id = ProjectId::new();
    let agent_id = AgentId::new();
    let session_id = SessionId::new();
    {
        let storage =
            Storage::open_migrated(config.database_path()).expect("storage should migrate");
        storage
            .insert_project(&Project {
                id: project_id,
                name: "Recovery".to_owned(),
                path: temporary.path().to_path_buf(),
                repository_root: None,
                current_branch: None,
                created_at_ms: 1,
                last_opened_at_ms: 1,
            })
            .expect("project should insert");
        storage
            .insert_agent(&StoredAgent {
                id: agent_id,
                source: AgentSource::BuiltIn,
                display_name: "Codex".to_owned(),
                executable: "codex".to_owned(),
                args: Vec::new(),
                env: BTreeMap::new(),
                enabled: true,
                created_at_ms: 1,
                updated_at_ms: 1,
            })
            .expect("agent should insert");
        storage
            .insert_session(&StoredSession {
                id: session_id,
                project_id,
                agent_id,
                name: "Stale".to_owned(),
                cwd: temporary.path().to_path_buf(),
                status: SessionStatus::Running,
                runtime_pid: Some(1),
                daemon_instance_id: Some("previous-daemon".to_owned()),
                exit_code: None,
                error_code: None,
                created_at_ms: 1,
                updated_at_ms: 1,
                last_activity_at_ms: Some(1),
            })
            .expect("stale session should insert");
    }

    let daemon = RunningDaemon::start(temporary.path());
    daemon.stop().await;

    let storage = Storage::open_migrated(config.database_path()).expect("storage should reopen");
    let recovered = storage
        .get_session(session_id)
        .expect("session should load")
        .expect("session should exist");
    assert_eq!(recovered.status, SessionStatus::Unknown);
    assert_eq!(recovered.runtime_pid, None);
    assert_eq!(recovered.daemon_instance_id, None);
    assert_eq!(recovered.error_code.as_deref(), Some("daemon_restarted"));
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

fn decode_test_base64(encoded: &str) -> Vec<u8> {
    fn value(byte: u8) -> Option<u8> {
        match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a' + 26),
            b'0'..=b'9' => Some(byte - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    let mut decoded = Vec::new();
    for chunk in encoded.as_bytes().chunks_exact(4) {
        let a = value(chunk[0]).expect("base64 a");
        let b = value(chunk[1]).expect("base64 b");
        let c = if chunk[2] == b'=' {
            0
        } else {
            value(chunk[2]).expect("base64 c")
        };
        let d = if chunk[3] == b'=' {
            0
        } else {
            value(chunk[3]).expect("base64 d")
        };
        decoded.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            decoded.push((b << 4) | (c >> 2));
        }
        if chunk[3] != b'=' {
            decoded.push((c << 6) | d);
        }
    }
    decoded
}

#[allow(dead_code)]
fn assert_io_error_is_send_sync(_: io::Error) {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<DaemonError>();
}
