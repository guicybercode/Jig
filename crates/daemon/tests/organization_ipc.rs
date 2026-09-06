//! Real socket/SQLite acceptance for organization without process side effects.
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

async fn started_session(client: &mut Client, project_id: &Value) -> Value {
    let agent = success(client, "agent.custom.create", json!({
        "displayName":"Organization test child", "command":{"executable":"/bin/cat","args":[],"env":{}}
    })).await;
    let session = success(client, "session.create", json!({
        "projectId":project_id,"agentId":agent["id"],"name":"Visible process","isolation":"current"
    })).await;
    success(client, "session.start", json!({"sessionId":session["id"]})).await;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let list = success(client, "session.list", json!({"projectId":project_id})).await;
            let current = &list["sessions"][0];
            if current["status"] == "running" {
                return current.clone();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn archive_and_workflow_preserve_the_live_process_and_survive_restart() {
    let root = TempDir::new().unwrap();
    let daemon = Running::start(root.path());
    let mut client = daemon.connect().await;
    let project_id = project(&mut client, root.path(), "organized").await;
    let session = started_session(&mut client, &project_id).await;
    let session_target = json!({"kind":"session","id":session["id"]});
    let project_target = json!({"kind":"project","id":project_id});
    let targets = json!([session_target, project_target]);
    let defaults = success(&mut client, "organization.get", json!({"targets":targets})).await;
    assert_eq!(defaults["entries"][0]["workflow"], "backlog");
    assert_eq!(defaults["entries"][0]["target"], session_target);
    assert_eq!(defaults["entries"][1]["target"], project_target);
    assert_eq!(defaults["entries"][1]["revision"], 0);
    assert!(defaults["entries"][1]["updatedAtMs"].is_null());
    let pinned = success(&mut client, "organization.save", json!({
        "target":project_target,"expectedRevision":0,"pinned":true,"archived":true,"workflow":null
    })).await;
    let mut saved = Value::Null;
    for (revision, workflow) in ["in_progress", "in_review", "blocked", "done", "backlog"]
        .iter()
        .enumerate()
    {
        saved = success(&mut client, "organization.save", json!({
            "target":session_target,"expectedRevision":revision,"pinned":true,"archived":true,"workflow":workflow
        })).await;
        assert_eq!(saved["revision"], revision + 1);
        let live = success(&mut client, "session.list", json!({"projectId":project_id})).await;
        assert_eq!(live["sessions"][0], session);
    }
    let mut second = daemon.connect().await;
    assert_eq!(error_code(exchange(&mut second, "organization.save", json!({
        "target":session_target,"expectedRevision":0,"pinned":false,"archived":false,"workflow":"done"
    })).await), "organization_conflict");
    let current = success(&mut second, "organization.get", json!({"targets":targets})).await;
    assert_eq!(current["entries"], json!([saved, pinned]));
    success(
        &mut client,
        "session.stop",
        json!({"sessionId":session["id"]}),
    )
    .await;
    drop(client);
    drop(second);
    daemon.stop().await;
    let daemon = Running::start(root.path());
    let mut client = daemon.connect().await;
    assert_eq!(
        success(&mut client, "organization.get", json!({"targets":targets})).await,
        current
    );
    success(
        &mut client,
        "session.delete",
        json!({"sessionId":session["id"]}),
    )
    .await;
    assert_eq!(
        error_code(
            exchange(
                &mut client,
                "organization.get",
                json!({"targets":[session_target]})
            )
            .await
        ),
        "session_not_found"
    );
    success(
        &mut client,
        "project.remove",
        json!({"projectId":project_id}),
    )
    .await;
    assert_eq!(
        error_code(
            exchange(
                &mut client,
                "organization.get",
                json!({"targets":[project_target]})
            )
            .await
        ),
        "project_not_found"
    );
    assert!(root.path().join("organized/.git").is_dir());
    daemon.stop().await;
}

#[tokio::test]
async fn organization_rejects_process_fields_and_invalid_batches_without_echoing_values() {
    let root = TempDir::new().unwrap();
    let daemon = Running::start(root.path());
    let mut client = daemon.connect().await;
    let id = project(&mut client, root.path(), "validation").await;
    let target = json!({"kind":"project","id":id});
    for targets in [
        json!([]),
        json!([target, target]),
        json!([{"kind":"path","id":"private-canary"}]),
    ] {
        let response = exchange(&mut client, "organization.get", json!({"targets":targets})).await;
        assert!(
            !serde_json::to_string(&response)
                .unwrap()
                .contains("private-canary")
        );
        assert_eq!(error_code(response), "invalid_payload");
    }
    for payload in [
        json!({"target":target,"expectedRevision":0,"pinned":false,"archived":true,"workflow":"private-canary"}),
        json!({"target":target,"expectedRevision":0,"pinned":false,"archived":true,"workflow":null,"status":"private-canary"}),
        json!({"target":target,"expectedRevision":0,"pinned":false,"archived":true}),
        json!({"target":target,"expectedRevision":-1,"pinned":false,"archived":true,"workflow":null}),
    ] {
        let response = exchange(&mut client, "organization.save", payload).await;
        assert!(
            !serde_json::to_string(&response)
                .unwrap()
                .contains("private-canary")
        );
        assert_eq!(error_code(response), "invalid_payload");
    }
    let empty = success(&mut client, "organization.get", json!({"targets":[target]})).await;
    assert_eq!(empty["entries"][0]["revision"], 0);
    assert_eq!(empty["entries"][0]["archived"], false);
    daemon.stop().await;
}
