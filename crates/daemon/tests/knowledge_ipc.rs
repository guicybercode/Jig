//! Real socket/SQLite acceptance for local prompts and context.
use std::{path::Path, process::Command, time::Duration};

use cli_master_core::{RequestEnvelope, ResponseEnvelope, ResponsePayload};
use cli_master_daemon::{Daemon, DaemonConfig, MAX_FRAME_LENGTH};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::net::UnixStream;
use tokio_util::{
    codec::{Framed, LengthDelimitedCodec},
    sync::CancellationToken,
};

type Client = Framed<UnixStream, LengthDelimitedCodec>;

struct Running {
    config: DaemonConfig,
    cancellation: CancellationToken,
    task: tokio::task::JoinHandle<Result<(), cli_master_daemon::DaemonError>>,
}

impl Running {
    fn start(root: &Path) -> Self {
        let config = DaemonConfig::from_paths(root.join("data"), root.join("run"));
        let daemon = Daemon::bind(config.clone()).unwrap();
        let cancellation = CancellationToken::new();
        let token = cancellation.clone();
        let task = tokio::spawn(async move { daemon.run(token).await });
        Self {
            config,
            cancellation,
            task,
        }
    }

    async fn connect(&self) -> Client {
        LengthDelimitedCodec::builder()
            .max_frame_length(MAX_FRAME_LENGTH)
            .new_framed(
                UnixStream::connect(self.config.socket_path())
                    .await
                    .unwrap(),
            )
    }

    async fn stop(self) {
        self.cancellation.cancel();
        tokio::time::timeout(Duration::from_secs(5), self.task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}

async fn exchange(client: &mut Client, method: &str, payload: Value) -> ResponseEnvelope<Value> {
    let request = RequestEnvelope::v1(method, payload);
    client
        .send(serde_json::to_vec(&request).unwrap().into())
        .await
        .unwrap();
    let bytes = tokio::time::timeout(Duration::from_secs(5), client.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(bytes.len() <= MAX_FRAME_LENGTH);
    let response: ResponseEnvelope<Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(response.request_id, request.request_id);
    response
}

async fn success(client: &mut Client, method: &str, payload: Value) -> Value {
    match exchange(client, method, payload).await.payload {
        ResponsePayload::Success { data } => data,
        ResponsePayload::Error { error } => panic!("{method}: {}", error.code),
    }
}

fn error_code(response: ResponseEnvelope<Value>) -> String {
    match response.payload {
        ResponsePayload::Error { error } => error.code,
        ResponsePayload::Success { .. } => panic!("expected request failure"),
    }
}

async fn project(client: &mut Client, root: &Path, name: &str) -> Value {
    let path = root.join(name);
    std::fs::create_dir(&path).unwrap();
    assert!(
        Command::new("git")
            .args(["init", "--initial-branch=main"])
            .current_dir(&path)
            .output()
            .unwrap()
            .status
            .success()
    );
    success(client, "project.add", json!({"path":path, "name":name})).await["id"].clone()
}

#[tokio::test]
async fn knowledge_scopes_conflicts_restart_and_no_session_effects() {
    let root = TempDir::new().unwrap();
    let daemon = Running::start(root.path());
    let mut client = daemon.connect().await;
    let mut second = daemon.connect().await;
    let first_project = project(&mut client, root.path(), "first").await;
    let other_project = project(&mut client, root.path(), "other").await;
    let before = success(&mut client, "state.snapshot", json!({})).await;
    let global = success(&mut client, "knowledge.save", json!({
        "kind":"prompt", "projectId":null, "title":"Review % literal", "body":"Review this change without sending anything."
    })).await;
    assert!(global["projectId"].is_null());
    let local = success(&mut client, "knowledge.save", json!({
        "kind":"context", "projectId":first_project, "title":"Decision", "body":"Keep the original process owner."
    })).await;
    let page = success(
        &mut second,
        "knowledge.list",
        json!({"projectId":first_project}),
    )
    .await;
    assert_eq!(page["entries"].as_array().unwrap().len(), 2);
    assert!(page["nextCursor"].is_null());
    let other = success(
        &mut second,
        "knowledge.list",
        json!({"projectId":other_project}),
    )
    .await;
    assert_eq!(other["entries"], json!([global]));
    let literal = success(&mut second, "knowledge.list", json!({"query":"%"})).await;
    assert_eq!(literal["entries"], json!([global]));
    let updated = success(
        &mut client,
        "knowledge.save",
        json!({
            "id":global["id"], "expectedRevision":1, "kind":"prompt", "projectId":null,
            "title":"Reviewed", "body":"Updated prompt"
        }),
    )
    .await;
    assert_eq!(updated["revision"], 2);
    assert_eq!(updated["createdAtMs"], global["createdAtMs"]);
    for method in ["knowledge.save", "knowledge.delete"] {
        let payload = if method == "knowledge.save" {
            json!({
                "id":global["id"], "expectedRevision":1, "kind":"prompt", "projectId":null,
                "title":"stale", "body":"must not replace updated text"
            })
        } else {
            json!({"id":global["id"], "expectedRevision":1})
        };
        assert_eq!(
            error_code(exchange(&mut second, method, payload).await),
            "knowledge_conflict"
        );
    }
    let after = success(&mut client, "state.snapshot", json!({})).await;
    assert_eq!(after["sessions"], before["sessions"]);
    assert_eq!(after["worktrees"], before["worktrees"]);
    drop(client);
    drop(second);
    daemon.stop().await;

    let daemon = Running::start(root.path());
    let mut client = daemon.connect().await;
    let page = success(
        &mut client,
        "knowledge.list",
        json!({"projectId":first_project}),
    )
    .await;
    assert!(page["entries"].as_array().unwrap().contains(&updated));
    assert!(page["entries"].as_array().unwrap().contains(&local));
    success(
        &mut client,
        "knowledge.delete",
        json!({"id":updated["id"],"expectedRevision":2}),
    )
    .await;
    success(
        &mut client,
        "project.remove",
        json!({"projectId":first_project}),
    )
    .await;
    assert!(root.path().join("first/.git").is_dir());
    let empty = success(&mut client, "knowledge.list", json!({})).await;
    assert_eq!(empty, json!({"entries":[],"nextCursor":null}));
    daemon.stop().await;
}

#[tokio::test]
async fn knowledge_worst_case_escaped_pages_and_safe_invalid_payloads() {
    let root = TempDir::new().unwrap();
    let daemon = Running::start(root.path());
    let mut client = daemon.connect().await;
    let body = "\u{1}".repeat(65_536);
    let mut ids = Vec::new();
    for index in 0..3 {
        let entry = success(
            &mut client,
            "knowledge.save",
            json!({
                "kind":"prompt", "title":format!("Large {index}"), "body":body, "projectId":null
            }),
        )
        .await;
        ids.push(entry["id"].clone());
    }
    let mut seen = Vec::new();
    let mut cursor = Value::Null;
    for _ in 0..4 {
        let page = success(&mut client, "knowledge.list", json!({"cursor":cursor})).await;
        for entry in page["entries"].as_array().unwrap() {
            assert_eq!(entry["body"], body);
            seen.push(entry["id"].clone());
        }
        cursor = page["nextCursor"].clone();
        if cursor.is_null() {
            break;
        }
    }
    assert!(cursor.is_null());
    assert_eq!(seen, ids);
    for payload in [
        json!({"kind":"private-submitted-content", "title":"text", "body":"private-submitted-content"}),
        json!({"kind":"prompt", "title":"text", "body":"private-submitted-content", "expectedRevision":0}),
        json!({"kind":"prompt", "title":"text", "body":"x".repeat(65_537)}),
        json!({"kind":"prompt", "title":"text", "body":"\0"}),
    ] {
        let response = exchange(&mut client, "knowledge.save", payload).await;
        assert!(
            !serde_json::to_string(&response)
                .unwrap()
                .contains("private-submitted-content")
        );
        assert_eq!(error_code(response), "invalid_payload");
    }
    assert_eq!(
        success(&mut client, "state.snapshot", json!({})).await["sessions"],
        json!([])
    );
    daemon.stop().await;
}
