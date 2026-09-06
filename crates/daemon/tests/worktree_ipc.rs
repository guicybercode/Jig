use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use cli_master_core::wire::{
    AgentRecord, ConfirmationToken, StateSnapshotResponse, WorktreeListResponse,
    WorktreePrepareRemoveResponse, WorktreeRemovalBlocker, method,
};
use cli_master_core::{
    AgentId, CommandSpec, Project, ProjectId, RequestEnvelope, ResponseEnvelope, ResponsePayload,
    Session, SessionStatus, Worktree, WorktreeId, WorktreeState,
};
use cli_master_daemon::{Daemon, DaemonConfig, DaemonError, MAX_FRAME_LENGTH};
use cli_master_session::{SessionManager, SessionManagerConfig, TerminalSize};
use cli_master_storage::{SessionRuntimeUpdate, Storage};
use futures_util::{SinkExt, StreamExt};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::net::UnixStream;
use tokio::task::JoinHandle;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tokio_util::sync::CancellationToken;

type Client = Framed<UnixStream, LengthDelimitedCodec>;

struct RunningDaemon {
    config: DaemonConfig,
    cancellation: CancellationToken,
    task: JoinHandle<Result<(), DaemonError>>,
}

impl RunningDaemon {
    fn start(root: &Path) -> Self {
        let config = DaemonConfig::from_paths(root.join("data"), root.join("run"));
        let daemon = Daemon::bind(config.clone()).expect("daemon should bind");
        let cancellation = CancellationToken::new();
        let task_cancellation = cancellation.clone();
        let task = tokio::spawn(async move { daemon.run(task_cancellation).await });
        Self {
            config,
            cancellation,
            task,
        }
    }

    async fn connect(&self) -> Client {
        let stream = UnixStream::connect(self.config.socket_path())
            .await
            .expect("client should connect");
        LengthDelimitedCodec::builder()
            .max_frame_length(MAX_FRAME_LENGTH)
            .new_framed(stream)
    }

    async fn stop(self) {
        self.cancellation.cancel();
        tokio::time::timeout(Duration::from_secs(10), self.task)
            .await
            .expect("daemon should stop before timeout")
            .expect("daemon task should join")
            .expect("daemon should stop cleanly");
    }
}

struct Fixture {
    root: TempDir,
    repository: PathBuf,
    observed_cwd: PathBuf,
    project_id: ProjectId,
    agent_id: AgentId,
}

impl Fixture {
    async fn new(project_directory: &str) -> (Self, RunningDaemon, Client) {
        let root = TempDir::new().expect("temporary directory should exist");
        let repository = root.path().join("repository");
        fs::create_dir_all(repository.join("apps/api")).unwrap();
        fs::write(repository.join("apps/api/tracked.txt"), "original\n").unwrap();
        git(&repository, &["init", "-b", "main"]);
        git(
            &repository,
            &["config", "user.email", "tests@example.invalid"],
        );
        git(&repository, &["config", "user.name", "CLI Master Tests"]);
        git(&repository, &["add", "."]);
        git(&repository, &["commit", "-m", "initial"]);
        let script = root.path().join("observe-cwd.sh");
        fs::write(&script, "pwd -P > \"$1\"\nexec /bin/cat\n").unwrap();
        let observed_cwd = root.path().join("observed-cwd");
        let daemon = RunningDaemon::start(root.path());
        let mut client = daemon.connect().await;
        let project: Project = call(
            &mut client,
            method::PROJECT_ADD,
            json!({"path": repository.join(project_directory)}),
        )
        .await;
        let agent: AgentRecord = call(
            &mut client,
            method::AGENT_CUSTOM_CREATE,
            json!({
                "displayName": "Worktree fixture",
                "command": {
                    "executable": "/bin/sh",
                    "args": [script, observed_cwd],
                    "env": {}
                }
            }),
        )
        .await;
        (
            Self {
                root,
                repository,
                observed_cwd,
                project_id: project.id,
                agent_id: agent.id,
            },
            daemon,
            client,
        )
    }

    async fn create(&self, client: &mut Client, relative_directory: Option<&str>) -> Session {
        let mut payload = json!({
            "projectId": self.project_id,
            "agentId": self.agent_id,
            "name": "Isolated task",
            "isolation": "new_worktree"
        });
        if let Some(relative_directory) = relative_directory {
            payload["relativeDirectory"] = json!(relative_directory);
        }
        call(client, method::SESSION_CREATE, payload).await
    }
}

fn git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("git fixture should start");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

async fn exchange(client: &mut Client, method: &str, payload: Value) -> ResponseEnvelope<Value> {
    let request = RequestEnvelope::v1(method, payload);
    client
        .send(serde_json::to_vec(&request).unwrap().into())
        .await
        .expect("request should send");
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let bytes = client
                .next()
                .await
                .expect("response frame should arrive")
                .expect("frame should be valid");
            let envelope: Value = serde_json::from_slice(&bytes).unwrap();
            if envelope["kind"] == "event" {
                continue;
            }
            let response: ResponseEnvelope<Value> = serde_json::from_value(envelope).unwrap();
            assert_eq!(response.request_id, request.request_id);
            return response;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{method} should respond before timeout"))
}

async fn call<T: DeserializeOwned>(client: &mut Client, method: &str, payload: Value) -> T {
    match exchange(client, method, payload).await.payload {
        ResponsePayload::Success { data } => {
            serde_json::from_value(data).expect("response should match its wire DTO")
        }
        ResponsePayload::Error { error } => panic!("{method} failed: {error:?}"),
    }
}

async fn failure(client: &mut Client, method: &str, payload: Value) -> String {
    match exchange(client, method, payload).await.payload {
        ResponsePayload::Error { error } => error.code,
        ResponsePayload::Success { data } => panic!("{method} unexpectedly succeeded: {data}"),
    }
}

async fn worktrees(client: &mut Client, project_id: Option<ProjectId>) -> Vec<Worktree> {
    let payload = project_id.map_or_else(|| json!({}), |id| json!({"projectId": id}));
    let response: WorktreeListResponse = call(client, method::WORKTREE_LIST, payload).await;
    response.worktrees
}

async fn prepare(client: &mut Client, worktree_id: WorktreeId) -> WorktreePrepareRemoveResponse {
    call(
        client,
        method::WORKTREE_PREPARE_REMOVE,
        json!({"worktreeId": worktree_id}),
    )
    .await
}

async fn ready_token(client: &mut Client, worktree_id: WorktreeId) -> ConfirmationToken {
    let response = prepare(client, worktree_id).await;
    let WorktreePrepareRemoveResponse::Ready {
        worktree_id: prepared_id,
        confirmation_token,
        expires_at_ms,
    } = response
    else {
        panic!("clean unused worktree should be removable: {response:?}");
    };
    assert_eq!(prepared_id, worktree_id);
    assert!(expires_at_ms > 1_700_000_000_000);
    confirmation_token
}

async fn remove(client: &mut Client, worktree_id: WorktreeId, token: ConfirmationToken) {
    let _: Value = call(
        client,
        method::WORKTREE_REMOVE,
        json!({"worktreeId": worktree_id, "confirmationToken": token}),
    )
    .await;
}

async fn wait_for_file(path: &Path) -> String {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(contents) = fs::read_to_string(path) {
                if !contents.is_empty() {
                    return contents;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("child should report before timeout")
}

#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "one socket lifecycle proves preparation, launch and independent metadata deletion"
)]
async fn create_prepares_without_spawning_then_start_uses_the_isolated_subdirectory() {
    let (fixture, daemon, mut client) = Fixture::new("").await;
    let created = fixture.create(&mut client, Some("apps/api")).await;
    let worktree_id = created
        .worktree_id
        .expect("session should identify its worktree");
    let path = created.worktree_path.as_ref().expect("worktree path");
    assert_eq!(created.status, SessionStatus::Unknown);
    assert_eq!(created.pid, None);
    assert_eq!(created.pty_id, None);
    assert_eq!(
        failure(
            &mut client,
            method::SESSION_WRITE,
            json!({"sessionId": created.id, "base64": "eA=="})
        )
        .await,
        "session_not_found",
        "prepared metadata must not have a writable PTY"
    );
    assert_eq!(created.cwd, path.join("apps/api"));
    assert!(
        !fixture.observed_cwd.exists(),
        "create must not run the command"
    );
    assert_eq!(
        fs::read_to_string(created.cwd.join("tracked.txt")).unwrap(),
        "original\n"
    );
    assert_ne!(path, &fixture.repository);

    let snapshot: StateSnapshotResponse =
        call(&mut client, method::STATE_SNAPSHOT, json!({})).await;
    assert_eq!(snapshot.sessions, std::slice::from_ref(&created));
    let listed = worktrees(&mut client, Some(fixture.project_id)).await;
    assert_eq!(snapshot.worktrees, listed);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, worktree_id);
    assert_eq!(listed[0].session_id, Some(created.id));
    assert_eq!(listed[0].path, *path);
    assert_eq!(Some(&listed[0].branch), created.branch.as_ref());
    assert_eq!(listed[0].state, WorktreeState::Active);
    assert!(
        worktrees(&mut client, Some(ProjectId::new()))
            .await
            .is_empty()
    );

    let started: Session = call(
        &mut client,
        method::SESSION_START,
        json!({"sessionId": created.id}),
    )
    .await;
    assert!(started.status.is_live());
    assert!(started.pid.is_some());
    assert!(started.pty_id.is_some());
    assert_eq!(started.worktree_id, Some(worktree_id));
    assert_eq!(started.cwd, created.cwd);
    assert_eq!(
        PathBuf::from(wait_for_file(&fixture.observed_cwd).await.trim()),
        created.cwd
    );

    let _: Session = call(
        &mut client,
        method::SESSION_STOP,
        json!({"sessionId": created.id}),
    )
    .await;
    let _: Value = call(
        &mut client,
        method::SESSION_DELETE,
        json!({"sessionId": created.id}),
    )
    .await;
    assert!(
        path.join("apps/api/tracked.txt").is_file(),
        "deleting metadata must preserve the checkout"
    );
    let snapshot: StateSnapshotResponse =
        call(&mut client, method::STATE_SNAPSHOT, json!({})).await;
    assert!(snapshot.sessions.is_empty());
    assert_eq!(snapshot.worktrees.len(), 1);
    assert_eq!(snapshot.worktrees[0].session_id, None);

    let token = ready_token(&mut client, worktree_id).await;
    remove(&mut client, worktree_id, token).await;
    assert!(!path.exists());
    assert!(fixture.repository.join("apps/api/tracked.txt").is_file());
    assert!(worktrees(&mut client, None).await.is_empty());
    daemon.stop().await;
}

#[tokio::test]
async fn start_rejects_a_worktree_replaced_by_a_symlink_to_the_original_project() {
    let (fixture, daemon, mut client) = Fixture::new("").await;
    let created = fixture.create(&mut client, Some("apps/api")).await;
    let path = created.worktree_path.unwrap();
    let moved = fixture.root.path().join("moved-worktree");
    fs::rename(&path, &moved).unwrap();
    symlink(&fixture.repository, &path).unwrap();

    let response = exchange(
        &mut client,
        method::SESSION_START,
        json!({"sessionId": created.id}),
    )
    .await;
    // Restore Git's registered path before checking the result so the fixture
    // stays recoverable even if a regression unexpectedly starts the command.
    fs::remove_file(&path).unwrap();
    fs::rename(&moved, &path).unwrap();

    let ResponsePayload::Error { error } = response.payload else {
        panic!("a substituted worktree path must not launch an agent");
    };
    assert_eq!(error.code, "session_directory_unavailable");
    assert!(!fixture.observed_cwd.exists());
    for root in [&path, &fixture.repository] {
        assert_eq!(
            fs::read_to_string(root.join("apps/api/tracked.txt")).unwrap(),
            "original\n"
        );
    }
    let snapshot: StateSnapshotResponse =
        call(&mut client, method::STATE_SNAPSHOT, json!({})).await;
    assert_eq!(snapshot.sessions[0].status, SessionStatus::Unknown);
    assert_eq!(snapshot.sessions[0].pid, None);
    assert_eq!(snapshot.worktrees[0].path, path);
    daemon.stop().await;
}

#[tokio::test]
async fn concurrent_stop_and_restart_keep_the_same_session_metadata_consistent_with_its_runtime() {
    let (fixture, daemon, mut control) = Fixture::new("").await;
    let created = fixture.create(&mut control, None).await;
    let mut stopping = daemon.connect().await;
    let mut restarting = daemon.connect().await;

    for _ in 0..3 {
        let _: Session = call(
            &mut control,
            method::SESSION_START,
            json!({"sessionId": created.id}),
        )
        .await;
        let (stopped, restarted) = tokio::join!(
            call::<Session>(
                &mut stopping,
                method::SESSION_STOP,
                json!({"sessionId": created.id})
            ),
            call::<Session>(
                &mut restarting,
                method::SESSION_RESTART,
                json!({"sessionId": created.id})
            ),
        );
        assert!(!stopped.status.is_live());
        assert_eq!(stopped.pid, None);
        assert!(restarted.status.is_live());
        assert!(restarted.pid.is_some());
        assert_eq!(restarted.worktree_id, created.worktree_id);

        let snapshot: StateSnapshotResponse =
            call(&mut control, method::STATE_SNAPSHOT, json!({})).await;
        assert_eq!(snapshot.sessions.len(), 1);
        let durable = &snapshot.sessions[0];
        assert_eq!(durable.id, created.id);
        let write = exchange(
            &mut control,
            method::SESSION_WRITE,
            json!({"sessionId": created.id, "base64": "cHJvYmUK"}),
        )
        .await;
        match write.payload {
            ResponsePayload::Success { .. } => {
                assert!(
                    durable.status.is_live(),
                    "a writable runtime must be recorded live"
                );
                assert_eq!(durable.pid, restarted.pid);
            }
            ResponsePayload::Error { error } => {
                assert_eq!(error.code, "session_not_running");
                assert!(!durable.status.is_live());
                assert_eq!(durable.pid, None);
            }
        }
    }

    let stopped: Session = call(
        &mut control,
        method::SESSION_STOP,
        json!({"sessionId": created.id}),
    )
    .await;
    assert!(!stopped.status.is_live());
    assert_eq!(stopped.pid, None);
    let token = ready_token(&mut control, created.worktree_id.unwrap()).await;
    remove(&mut control, created.worktree_id.unwrap(), token).await;
    daemon.stop().await;
}

#[tokio::test]
async fn dirty_worktrees_block_removal_and_changes_after_confirmation_invalidate_the_token() {
    let (fixture, daemon, mut client) = Fixture::new("").await;
    let created = fixture.create(&mut client, None).await;
    let worktree_id = created.worktree_id.unwrap();
    let path = created.worktree_path.unwrap();
    let token = ready_token(&mut client, worktree_id).await;
    fs::write(path.join("user-notes.txt"), "preserve this\n").unwrap();

    assert_eq!(
        failure(
            &mut client,
            method::WORKTREE_REMOVE,
            json!({
                "worktreeId": worktree_id, "confirmationToken": token
            })
        )
        .await,
        "worktree_confirmation_invalid"
    );
    assert_eq!(
        fs::read_to_string(path.join("user-notes.txt")).unwrap(),
        "preserve this\n"
    );
    let blocked = prepare(&mut client, worktree_id).await;
    let WorktreePrepareRemoveResponse::Blocked {
        is_dirty, blockers, ..
    } = blocked
    else {
        panic!("untracked content must block removal: {blocked:?}");
    };
    assert!(is_dirty);
    assert!(blockers.contains(&WorktreeRemovalBlocker::UntrackedFiles));
    assert_eq!(
        worktrees(&mut client, None).await[0].state,
        WorktreeState::Active
    );

    fs::remove_file(path.join("user-notes.txt")).unwrap();
    let token = ready_token(&mut client, worktree_id).await;
    remove(&mut client, worktree_id, token).await;
    assert!(!path.exists());
    daemon.stop().await;
}

#[tokio::test]
async fn a_live_session_blocks_removal_and_old_tokens_cannot_override_changed_ownership() {
    let (fixture, daemon, mut client) = Fixture::new("").await;
    let created = fixture.create(&mut client, None).await;
    let worktree_id = created.worktree_id.unwrap();
    let path = created.worktree_path.unwrap();
    let token = ready_token(&mut client, worktree_id).await;
    let _: Session = call(
        &mut client,
        method::SESSION_START,
        json!({"sessionId": created.id}),
    )
    .await;
    wait_for_file(&fixture.observed_cwd).await;
    assert_eq!(
        failure(
            &mut client,
            method::WORKTREE_REMOVE,
            json!({
                "worktreeId": worktree_id, "confirmationToken": token
            })
        )
        .await,
        "worktree_confirmation_invalid"
    );
    let blocked = prepare(&mut client, worktree_id).await;
    let WorktreePrepareRemoveResponse::Blocked {
        is_dirty, blockers, ..
    } = blocked
    else {
        panic!("the live session must block removal: {blocked:?}");
    };
    assert!(!is_dirty);
    assert!(
        blockers.contains(&WorktreeRemovalBlocker::Running)
            || blockers.contains(&WorktreeRemovalBlocker::InUse)
    );
    assert_eq!(
        failure(
            &mut client,
            method::SESSION_DELETE,
            json!({"sessionId": created.id})
        )
        .await,
        "session_still_running"
    );
    assert!(path.is_dir());

    let _: Session = call(
        &mut client,
        method::SESSION_STOP,
        json!({"sessionId": created.id}),
    )
    .await;
    let token = ready_token(&mut client, worktree_id).await;
    let _: Value = call(
        &mut client,
        method::SESSION_DELETE,
        json!({"sessionId": created.id}),
    )
    .await;
    assert_eq!(
        failure(
            &mut client,
            method::WORKTREE_REMOVE,
            json!({
                "worktreeId": worktree_id, "confirmationToken": token
            })
        )
        .await,
        "worktree_confirmation_invalid"
    );
    assert!(path.is_dir());
    let token = ready_token(&mut client, worktree_id).await;
    remove(&mut client, worktree_id, token).await;
    daemon.stop().await;
}

#[tokio::test]
async fn an_unsubscribed_session_can_be_deleted_after_its_process_exits_on_its_own() {
    let (fixture, daemon, mut client) = Fixture::new("").await;
    fs::write(
        fixture.root.path().join("observe-cwd.sh"),
        "pwd -P > \"$1\"\nIFS= read -r line\n",
    )
    .unwrap();
    let created = fixture.create(&mut client, None).await;
    let worktree_id = created.worktree_id.unwrap();
    let path = created.worktree_path.unwrap();
    let started: Session = call(
        &mut client,
        method::SESSION_START,
        json!({"sessionId": created.id}),
    )
    .await;
    assert!(started.status.is_live());
    wait_for_file(&fixture.observed_cwd).await;

    // Let the command return normally, with no subscription driving durable
    // status updates and no explicit stop operation that could hide the bug.
    let _: Value = call(
        &mut client,
        method::SESSION_WRITE,
        json!({"sessionId": created.id, "base64": "ZG9uZQo="}),
    )
    .await;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match exchange(
                &mut client,
                method::SESSION_DELETE,
                json!({"sessionId": created.id}),
            )
            .await
            .payload
            {
                ResponsePayload::Success { .. } => break,
                ResponsePayload::Error { error } => {
                    assert_eq!(error.code, "session_still_running");
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("metadata should become deletable after the child exits");

    assert!(path.join("apps/api/tracked.txt").is_file());
    let snapshot: StateSnapshotResponse =
        call(&mut client, method::STATE_SNAPSHOT, json!({})).await;
    assert!(snapshot.sessions.is_empty());
    assert_eq!(snapshot.worktrees[0].id, worktree_id);
    assert_eq!(snapshot.worktrees[0].session_id, None);
    daemon.stop().await;
}

#[tokio::test]
async fn another_session_using_the_checkout_blocks_removal_without_a_worktree_association() {
    let (fixture, daemon, mut client) = Fixture::new("").await;
    let isolated = fixture.create(&mut client, None).await;
    let worktree_id = isolated.worktree_id.unwrap();
    let path = isolated.worktree_path.unwrap();
    let token = ready_token(&mut client, worktree_id).await;
    let project: Project = call(
        &mut client,
        method::PROJECT_ADD,
        json!({"path": path.join("apps/api")}),
    )
    .await;
    let other: Session = call(
        &mut client,
        method::SESSION_CREATE,
        json!({
            "projectId": project.id,
            "agentId": fixture.agent_id,
            "name": "Independent terminal",
            "isolation": "current"
        }),
    )
    .await;
    assert_eq!(other.worktree_id, None);
    let _: Session = call(
        &mut client,
        method::SESSION_START,
        json!({"sessionId": other.id}),
    )
    .await;
    wait_for_file(&fixture.observed_cwd).await;
    let error = failure(
        &mut client,
        method::WORKTREE_REMOVE,
        json!({"worktreeId": worktree_id, "confirmationToken": token}),
    )
    .await;
    assert!(
        ["worktree_confirmation_invalid", "worktree_in_use"].contains(&error.as_str()),
        "removal should reject changed usage: {error}"
    );
    let blocked = prepare(&mut client, worktree_id).await;
    let WorktreePrepareRemoveResponse::Blocked { blockers, .. } = blocked else {
        panic!("another session using this checkout must block removal: {blocked:?}");
    };
    assert!(blockers.contains(&WorktreeRemovalBlocker::InUse));
    assert!(path.is_dir());

    let _: Session = call(
        &mut client,
        method::SESSION_STOP,
        json!({"sessionId": other.id}),
    )
    .await;
    let token = ready_token(&mut client, worktree_id).await;
    remove(&mut client, worktree_id, token).await;
    assert!(!path.exists());
    daemon.stop().await;
}

#[tokio::test]
async fn prepared_worktrees_survive_restart_with_the_project_subdirectory_preserved() {
    let (fixture, first, mut client) = Fixture::new("apps").await;
    let created = fixture.create(&mut client, Some("api")).await;
    let worktree_id = created.worktree_id.unwrap();
    let path = created.worktree_path.as_ref().unwrap();
    assert_eq!(created.cwd, path.join("apps/api"));
    drop(client);
    first.stop().await;

    let second = RunningDaemon::start(fixture.root.path());
    let mut client = second.connect().await;
    let snapshot: StateSnapshotResponse =
        call(&mut client, method::STATE_SNAPSHOT, json!({})).await;
    assert_eq!(snapshot.sessions.len(), 1);
    assert_eq!(snapshot.sessions[0].id, created.id);
    assert_eq!(snapshot.sessions[0].cwd, created.cwd);
    assert_eq!(snapshot.sessions[0].worktree_id, Some(worktree_id));
    assert_eq!(snapshot.sessions[0].status, SessionStatus::Unknown);
    assert_eq!(snapshot.sessions[0].pid, None);
    assert_eq!(snapshot.worktrees.len(), 1);
    assert_eq!(snapshot.worktrees[0].state, WorktreeState::Active);
    assert_eq!(snapshot.worktrees[0].path, *path);
    assert_eq!(snapshot.worktrees, worktrees(&mut client, None).await);
    assert!(!fixture.observed_cwd.exists());

    let started: Session = call(
        &mut client,
        method::SESSION_START,
        json!({"sessionId": created.id}),
    )
    .await;
    assert_eq!(started.cwd, created.cwd);
    assert_eq!(
        PathBuf::from(wait_for_file(&fixture.observed_cwd).await.trim()),
        created.cwd
    );
    let _: Session = call(
        &mut client,
        method::SESSION_STOP,
        json!({"sessionId": created.id}),
    )
    .await;
    second.stop().await;
}

#[tokio::test]
async fn restart_reconciles_a_worktree_session_without_signalling_an_unowned_stale_pid() {
    let (fixture, first, mut client) = Fixture::new("").await;
    let created = fixture.create(&mut client, None).await;
    let database = first.config.database_path().to_path_buf();
    drop(client);
    first.stop().await;

    // This separate manager owns the canary process. Its PID simulates an old
    // persisted PID that now belongs to an unrelated process after a crash.
    let canary_script = fixture.root.path().join("canary.sh");
    let canary_reply = fixture.root.path().join("canary-reply");
    fs::write(
        &canary_script,
        "IFS= read -r line\nprintf '%s' \"$line\" > \"$1\"\n",
    )
    .unwrap();
    let canary_manager = SessionManager::new(SessionManagerConfig::default()).unwrap();
    let command = CommandSpec::try_from_parts(
        "/bin/sh",
        [
            canary_script.to_string_lossy().into_owned(),
            canary_reply.to_string_lossy().into_owned(),
        ],
        fixture.root.path(),
        BTreeMap::new(),
    )
    .unwrap();
    let canary = canary_manager
        .spawn(&command, TerminalSize::default())
        .unwrap();
    let canary_pid = canary_manager.snapshot(canary.id).unwrap().pid.unwrap();
    Storage::open(&database)
        .unwrap()
        .update_session_runtime(
            created.id,
            &SessionRuntimeUpdate {
                status: SessionStatus::Running,
                runtime_pid: Some(canary_pid),
                daemon_instance_id: Some("crashed-daemon".to_owned()),
                exit_code: None,
                error_code: None,
                last_activity_at_ms: None,
                updated_at_ms: created.updated_at_ms,
            },
        )
        .unwrap();

    let second = RunningDaemon::start(fixture.root.path());
    let mut client = second.connect().await;
    let snapshot: StateSnapshotResponse =
        call(&mut client, method::STATE_SNAPSHOT, json!({})).await;
    let recovered = &snapshot.sessions[0];
    assert_eq!(recovered.id, created.id);
    assert_eq!(recovered.status, SessionStatus::Unknown);
    assert_eq!(recovered.pid, None);
    assert_eq!(recovered.pty_id, None);
    assert_eq!(recovered.worktree_id, created.worktree_id);
    assert_eq!(recovered.worktree_path, created.worktree_path);
    assert_eq!(snapshot.worktrees[0].state, WorktreeState::Active);
    assert_eq!(
        failure(
            &mut client,
            method::SESSION_STOP,
            json!({"sessionId": created.id})
        )
        .await,
        "session_not_found"
    );
    canary_manager.write(canary.id, b"still-alive\n").unwrap();
    assert_eq!(wait_for_file(&canary_reply).await, "still-alive");
    let durable = Storage::open(&database)
        .unwrap()
        .get_session(created.id)
        .unwrap()
        .unwrap();
    assert_eq!(durable.status, SessionStatus::Unknown);
    assert_eq!(durable.runtime_pid, None);
    assert_eq!(durable.daemon_instance_id, None);
    canary_manager.shutdown().unwrap();
    second.stop().await;
}
