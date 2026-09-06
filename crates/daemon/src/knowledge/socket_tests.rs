//! Discovery acceptance over the real socket with isolated filesystem roots.
use std::{fs, path::Path, sync::Arc, time::Duration};

use cli_master_core::{RequestEnvelope, ResponseEnvelope, ResponsePayload};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::net::UnixStream;
use tokio_util::{
    codec::{Framed, LengthDelimitedCodec},
    sync::CancellationToken,
};

use super::{Daemon, MAX_FRAME_LENGTH};
use crate::{
    DaemonConfig,
    knowledge::discovery::{DiscoveryRoots, DiscoveryService},
};

type Client = Framed<UnixStream, LengthDelimitedCodec>;

struct Running {
    config: DaemonConfig,
    cancellation: CancellationToken,
    task: tokio::task::JoinHandle<Result<(), crate::DaemonError>>,
}

impl Running {
    fn start(root: &Path) -> Self {
        let config = DaemonConfig::from_paths(root.join("data"), root.join("run"));
        let mut daemon = Daemon::bind(config.clone()).unwrap();
        let state = Arc::get_mut(&mut daemon.state).unwrap();
        state.knowledge_discovery = std::sync::Mutex::new(DiscoveryService::new(DiscoveryRoots {
            home: Some(root.join("home")),
            codex_home: Some(root.join("home/.codex")),
            admin_skills: root.join("admin/skills"),
        }));
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

fn write(root: &Path, path: &str, content: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn source(scan: &Value, suffix: &str) -> Value {
    scan["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["sourcePath"].as_str().unwrap().ends_with(suffix))
        .unwrap_or_else(|| panic!("missing {suffix}"))
        .clone()
}

#[tokio::test]
async fn discovery_and_explicit_read_use_scoped_capabilities_without_session_effects() {
    let root = TempDir::new().unwrap();
    write(
        root.path(),
        "home/.codex/AGENTS.md",
        "Global private rule body",
    );
    write(
        root.path(),
        "home/.codex/auth.json",
        "private-credential-canary",
    );
    write(root.path(), "project/AGENTS.md", "Project rule body");
    write(
        root.path(),
        "project/.agents/skills/review/SKILL.md",
        "---\nname: review\n---\nRead this skill.",
    );
    write(
        root.path(),
        "project/.agents/skills/review/run.sh",
        "never execute me",
    );
    let daemon = Running::start(root.path());
    let mut client = daemon.connect().await;
    let project = success(
        &mut client,
        "project.add",
        json!({"path":root.path().join("project")}),
    )
    .await;
    let before = success(&mut client, "state.snapshot", json!({})).await;
    let global = success(&mut client, "knowledge.discover", json!({})).await;
    assert!(
        global["entries"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| entry["scope"] != "project")
    );
    let scan = success(
        &mut client,
        "knowledge.discover",
        json!({"projectId":project["id"]}),
    )
    .await;
    let serialized = serde_json::to_string(&scan).unwrap();
    for excluded in [
        "Global private rule body",
        "Project rule body",
        "private-credential-canary",
        "auth.json",
        "run.sh",
    ] {
        assert!(!serialized.contains(excluded));
    }
    assert_eq!(scan["truncated"], false);
    let entry = source(&scan, "project/.agents/skills/review/SKILL.md");
    assert_eq!(entry["scope"], "project");
    assert_eq!(entry["availability"], "available");
    assert!(!entry["precedenceHint"].as_str().unwrap().is_empty());
    let read = success(
        &mut client,
        "knowledge.read",
        json!({"scanId":scan["scanId"],"entryId":entry["entryId"]}),
    )
    .await;
    assert_eq!(read["entry"], entry);
    assert_eq!(read["content"], "---\nname: review\n---\nRead this skill.");
    let after = success(&mut client, "state.snapshot", json!({})).await;
    assert_eq!(after["sessions"], before["sessions"]);
    assert_eq!(after["worktrees"], before["worktrees"]);
    success(
        &mut client,
        "project.remove",
        json!({"projectId":project["id"]}),
    )
    .await;
    assert_eq!(
        error_code(
            exchange(
                &mut client,
                "knowledge.read",
                json!({"scanId":scan["scanId"],"entryId":entry["entryId"]})
            )
            .await
        ),
        "project_not_found"
    );
    assert!(root.path().join("project/AGENTS.md").is_file());
    drop(client);
    daemon.stop().await;
    assert_scan_expired_after_restart(root.path(), &global).await;
}

async fn assert_scan_expired_after_restart(root: &Path, global: &Value) {
    let daemon = Running::start(root);
    let mut client = daemon.connect().await;
    let entry = source(global, "home/.codex/AGENTS.md");
    assert_eq!(
        error_code(
            exchange(
                &mut client,
                "knowledge.read",
                json!({"scanId":global["scanId"],"entryId":entry["entryId"]})
            )
            .await
        ),
        "knowledge_scan_expired"
    );
    daemon.stop().await;
}

#[tokio::test]
async fn discovery_rejects_paths_and_reads_detect_external_source_changes() {
    let root = TempDir::new().unwrap();
    write(root.path(), "project/AGENTS.md", "Original source");
    let daemon = Running::start(root.path());
    let mut client = daemon.connect().await;
    let project = success(
        &mut client,
        "project.add",
        json!({"path":root.path().join("project")}),
    )
    .await;
    let scan = success(
        &mut client,
        "knowledge.discover",
        json!({"projectId":project["id"]}),
    )
    .await;
    let entry = source(&scan, "project/AGENTS.md");
    fs::write(
        root.path().join("project/AGENTS.md"),
        "Changed source with a different length",
    )
    .unwrap();
    assert_eq!(
        error_code(
            exchange(
                &mut client,
                "knowledge.read",
                json!({"scanId":scan["scanId"],"entryId":entry["entryId"]})
            )
            .await
        ),
        "knowledge_source_changed"
    );
    for (method, payload) in [
        ("knowledge.discover", json!({"path":"private-path-canary"})),
        (
            "knowledge.read",
            json!({"scanId":scan["scanId"],"entryId":entry["entryId"],"path":"private-path-canary"}),
        ),
        (
            "knowledge.read",
            json!({"scanId":"private-path-canary","entryId":entry["entryId"]}),
        ),
    ] {
        let response = exchange(&mut client, method, payload).await;
        assert!(
            !serde_json::to_string(&response)
                .unwrap()
                .contains("private-path-canary")
        );
        assert_eq!(error_code(response), "invalid_payload");
    }
    assert_eq!(
        error_code(
            exchange(
                &mut client,
                "knowledge.discover",
                json!({"projectId":uuid::Uuid::now_v7()})
            )
            .await
        ),
        "project_not_found"
    );
    let refreshed = success(
        &mut client,
        "knowledge.discover",
        json!({"projectId":project["id"]}),
    )
    .await;
    let entry = source(&refreshed, "project/AGENTS.md");
    let read = success(
        &mut client,
        "knowledge.read",
        json!({"scanId":refreshed["scanId"],"entryId":entry["entryId"]}),
    )
    .await;
    assert_eq!(read["content"], "Changed source with a different length");
    daemon.stop().await;
}
