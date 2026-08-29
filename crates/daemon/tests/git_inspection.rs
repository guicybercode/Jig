use std::fs;
use std::path::Path;
use std::process::Command;

use cli_master_core::wire::{GitChangeKind, GitDiffResponse, GitStatusResponse, method};
use cli_master_core::{
    AgentId, AgentSource, Project, ProjectId, RequestEnvelope, ResponseEnvelope, ResponsePayload,
    SessionId, SessionStatus, WorktreeId,
};
use cli_master_daemon::{Daemon, DaemonConfig, DaemonError, MAX_FRAME_LENGTH, MAX_GIT_DIFF_BYTES};
use cli_master_storage::{Storage, StoredAgent, StoredSession, StoredWorktree, WorktreeState};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::net::UnixStream;
use tokio::task::JoinHandle;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tokio_util::sync::CancellationToken;

const CREATED_AT_MS: i64 = 1_787_941_200_000;

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

fn success(response: ResponseEnvelope<Value>) -> Value {
    match response.payload {
        ResponsePayload::Success { data } => data,
        ResponsePayload::Error { error } => panic!("expected success, received {error:?}"),
    }
}

fn failure_code(response: ResponseEnvelope<Value>) -> String {
    match response.payload {
        ResponsePayload::Error { error } => error.code,
        ResponsePayload::Success { data } => panic!("expected failure, received {data}"),
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
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_repository(path: &Path) {
    fs::create_dir_all(path).expect("repository directory should exist");
    git(path, &["init", "-b", "main"]);
    git(path, &["config", "user.email", "tests@example.invalid"]);
    git(path, &["config", "user.name", "CLI Master Tests"]);
    fs::write(path.join("tracked.txt"), "initial\n").expect("tracked file should be written");
    git(path, &["add", "tracked.txt"]);
    git(path, &["commit", "-m", "initial"]);
}

fn open_storage(database: &Path) -> Storage {
    Storage::open(database).expect("daemon database should open")
}

fn insert_project(database: &Path, repo: &Path) -> Project {
    let storage = open_storage(database);
    let project = Project {
        id: ProjectId::new(),
        name: "Inspect".to_owned(),
        path: repo.canonicalize().expect("repository should canonicalize"),
        repository_root: None,
        current_branch: None,
        created_at_ms: CREATED_AT_MS,
        last_opened_at_ms: CREATED_AT_MS,
    };
    storage
        .insert_project(&project)
        .expect("project should be registered");
    project
}

fn insert_worktree(database: &Path, project_id: ProjectId, repo: &Path) -> WorktreeId {
    let storage = open_storage(database);
    let worktree = StoredWorktree {
        id: WorktreeId::new(),
        project_id,
        session_id: None,
        path: repo.canonicalize().expect("repository should canonicalize"),
        branch: "main".to_owned(),
        state: WorktreeState::Active,
        is_dirty: false,
        created_at_ms: CREATED_AT_MS,
        updated_at_ms: CREATED_AT_MS,
    };
    storage
        .insert_worktree(&worktree)
        .expect("worktree should be registered");
    worktree.id
}

fn insert_session(database: &Path, project_id: ProjectId, repo: &Path) -> SessionId {
    let storage = open_storage(database);
    let agent = StoredAgent {
        id: AgentId::new(),
        source: AgentSource::Custom,
        display_name: "Fixture".to_owned(),
        executable: "codex".to_owned(),
        args: Vec::new(),
        env: std::collections::BTreeMap::new(),
        enabled: true,
        created_at_ms: CREATED_AT_MS,
        updated_at_ms: CREATED_AT_MS,
    };
    storage.insert_agent(&agent).expect("agent should insert");
    let session = StoredSession {
        id: SessionId::new(),
        project_id,
        agent_id: agent.id,
        name: "Inspect".to_owned(),
        cwd: repo.canonicalize().expect("repository should canonicalize"),
        status: SessionStatus::Exited,
        runtime_pid: None,
        daemon_instance_id: None,
        exit_code: None,
        error_code: None,
        created_at_ms: CREATED_AT_MS,
        updated_at_ms: CREATED_AT_MS,
        last_activity_at_ms: None,
    };
    storage
        .insert_session(&session)
        .expect("session should insert");
    session.id
}

fn project_target(project_id: ProjectId) -> Value {
    json!({ "kind": "project", "projectId": project_id })
}

#[tokio::test]
async fn unregistered_targets_are_rejected_without_path_details() {
    let temporary = TempDir::new().expect("temporary directory should exist");
    let daemon = RunningDaemon::start(temporary.path());
    let mut client = connect(daemon.config.socket_path()).await;
    let project_id = ProjectId::new();
    let worktree_id = WorktreeId::new();

    let status = RequestEnvelope::v1(
        method::GIT_STATUS,
        json!({ "target": project_target(project_id) }),
    );
    let response = exchange(&mut client, &status).await;
    match response.payload {
        ResponsePayload::Error { error } => {
            assert_eq!(error.code, "unregistered_git_target");
            assert!(!format!("{error:?}").contains("/tmp/"));
            assert_eq!(error.details.get("targetKind"), Some(&json!("project")));
        }
        ResponsePayload::Success { data } => panic!("expected failure, received {data}"),
    }

    let diff = RequestEnvelope::v1(
        method::GIT_DIFF,
        json!({ "target": { "kind": "worktree", "worktreeId": worktree_id } }),
    );
    assert_eq!(
        failure_code(exchange(&mut client, &diff).await),
        "unregistered_git_target"
    );
    daemon.stop().await;
}

#[tokio::test]
async fn status_lists_staged_unstaged_untracked_deleted_renamed_and_ignored() {
    let temporary = TempDir::new().expect("temporary directory should exist");
    let repo = temporary.path().join("repo");
    init_repository(&repo);
    fs::write(repo.join(".gitignore"), "noise.log\n").unwrap();
    fs::write(repo.join("gone.txt"), "gone\n").unwrap();
    git(&repo, &["add", ".gitignore", "gone.txt"]);
    git(&repo, &["commit", "-m", "ignore and gone"]);
    git(&repo, &["mv", "tracked.txt", "renamed.txt"]);
    fs::write(repo.join("added.txt"), "added\n").unwrap();
    git(&repo, &["add", "added.txt"]);
    fs::remove_file(repo.join("gone.txt")).unwrap();
    fs::write(repo.join("unstaged.txt"), "new\n").unwrap();
    git(&repo, &["add", "unstaged.txt"]);
    fs::write(repo.join("unstaged.txt"), "changed\n").unwrap();
    fs::write(repo.join("untracked.txt"), "loose\n").unwrap();
    fs::write(repo.join("noise.log"), "ignored\n").unwrap();

    let daemon = RunningDaemon::start(temporary.path());
    let project = insert_project(daemon.config.database_path(), &repo);
    let mut client = connect(daemon.config.socket_path()).await;
    let data = success(
        exchange(
            &mut client,
            &RequestEnvelope::v1(
                method::GIT_STATUS,
                json!({ "target": project_target(project.id) }),
            ),
        )
        .await,
    );
    let status: GitStatusResponse = serde_json::from_value(data).expect("status should decode");
    assert_eq!(status.branch.as_deref(), Some("main"));
    assert!(status.files.iter().any(|file| {
        file.path == "renamed.txt"
            && file.original_path.as_deref() == Some("tracked.txt")
            && file.kind == GitChangeKind::Renamed
            && file.staged
    }));
    assert!(status.files.iter().any(|file| {
        file.path == "added.txt" && file.kind == GitChangeKind::Added && file.staged
    }));
    assert!(
        status
            .files
            .iter()
            .any(|file| { file.path == "gone.txt" && file.kind == GitChangeKind::Deleted })
    );
    assert!(
        status
            .files
            .iter()
            .any(|file| { file.path == "untracked.txt" && file.kind == GitChangeKind::Untracked })
    );
    assert!(
        status
            .files
            .iter()
            .any(|file| { file.path == "noise.log" && file.kind == GitChangeKind::Ignored })
    );
    assert!(
        status
            .files
            .iter()
            .any(|file| { file.path == "unstaged.txt" && file.staged && file.unstaged })
    );
    assert!(status.is_dirty);
    daemon.stop().await;
}

#[tokio::test]
async fn worktree_and_session_targets_use_registered_paths() {
    let temporary = TempDir::new().expect("temporary directory should exist");
    let repo = temporary.path().join("repo");
    init_repository(&repo);
    fs::write(repo.join("tracked.txt"), "changed\n").unwrap();
    let daemon = RunningDaemon::start(temporary.path());
    let project = insert_project(daemon.config.database_path(), &repo);
    let worktree_id = insert_worktree(daemon.config.database_path(), project.id, &repo);
    let session_id = insert_session(daemon.config.database_path(), project.id, &repo);
    let mut client = connect(daemon.config.socket_path()).await;

    let worktree_status: GitStatusResponse = serde_json::from_value(success(
        exchange(
            &mut client,
            &RequestEnvelope::v1(
                method::GIT_STATUS,
                json!({ "target": { "kind": "worktree", "worktreeId": worktree_id } }),
            ),
        )
        .await,
    ))
    .unwrap();
    assert!(worktree_status.is_dirty);

    let session_status: GitStatusResponse = serde_json::from_value(success(
        exchange(
            &mut client,
            &RequestEnvelope::v1(
                method::GIT_STATUS,
                json!({ "target": { "kind": "session", "sessionId": session_id } }),
            ),
        )
        .await,
    ))
    .unwrap();
    assert!(session_status.is_dirty);
    daemon.stop().await;
}

#[tokio::test]
async fn diffs_cover_file_overall_binary_large_unborn_and_reject_strange_paths() {
    let temporary = TempDir::new().expect("temporary directory should exist");
    let repo = temporary.path().join("repo");
    init_repository(&repo);
    fs::write(repo.join("tracked.txt"), "changed line\n").unwrap();
    fs::write(repo.join("blob.bin"), [0_u8, 1, 2, 0, 255]).unwrap();
    git(&repo, &["add", "blob.bin"]);
    git(&repo, &["commit", "-m", "binary"]);
    fs::write(repo.join("blob.bin"), [0_u8, 9, 9, 0, 254]).unwrap();
    let original = "AAAAAAAA\n".repeat(80_000);
    fs::write(repo.join("large.txt"), &original).unwrap();
    git(&repo, &["add", "large.txt"]);
    git(&repo, &["commit", "-m", "large"]);
    fs::write(repo.join("large.txt"), "BBBBBBBB\n".repeat(80_000)).unwrap();

    let daemon = RunningDaemon::start(temporary.path());
    let project = insert_project(daemon.config.database_path(), &repo);
    let mut client = connect(daemon.config.socket_path()).await;
    let target = project_target(project.id);

    let file_diff: GitDiffResponse = serde_json::from_value(success(
        exchange(
            &mut client,
            &RequestEnvelope::v1(
                method::GIT_DIFF,
                json!({ "target": target, "path": "tracked.txt" }),
            ),
        )
        .await,
    ))
    .unwrap();
    assert!(file_diff.text.contains("+changed line"));
    assert!(!file_diff.binary);

    let overall: GitDiffResponse = serde_json::from_value(success(
        exchange(
            &mut client,
            &RequestEnvelope::v1(method::GIT_DIFF, json!({ "target": target })),
        )
        .await,
    ))
    .unwrap();
    assert!(overall.text.contains("tracked.txt") || overall.truncated);

    let binary: GitDiffResponse = serde_json::from_value(success(
        exchange(
            &mut client,
            &RequestEnvelope::v1(
                method::GIT_DIFF,
                json!({ "target": target, "path": "blob.bin" }),
            ),
        )
        .await,
    ))
    .unwrap();
    assert!(binary.binary);
    assert!(!binary.text.contains('\0'));

    let large_diff: GitDiffResponse = serde_json::from_value(success(
        exchange(
            &mut client,
            &RequestEnvelope::v1(
                method::GIT_DIFF,
                json!({ "target": target, "path": "large.txt" }),
            ),
        )
        .await,
    ))
    .unwrap();
    assert!(large_diff.truncated);
    assert!(large_diff.text.len() <= MAX_GIT_DIFF_BYTES);

    for path in ["../secret", "/tmp/not-registered", "-u", "--"] {
        let response = exchange(
            &mut client,
            &RequestEnvelope::v1(method::GIT_DIFF, json!({ "target": target, "path": path })),
        )
        .await;
        assert_eq!(failure_code(response), "invalid_payload", "{path}");
    }

    let unborn = temporary.path().join("unborn");
    fs::create_dir_all(&unborn).unwrap();
    git(&unborn, &["init", "-b", "main"]);
    git(&unborn, &["config", "user.email", "tests@example.invalid"]);
    git(&unborn, &["config", "user.name", "CLI Master Tests"]);
    fs::write(unborn.join("initial.txt"), "first content\n").unwrap();
    git(&unborn, &["add", "initial.txt"]);
    let unborn_project = insert_project(daemon.config.database_path(), &unborn);
    let unborn_diff: GitDiffResponse = serde_json::from_value(success(
        exchange(
            &mut client,
            &RequestEnvelope::v1(
                method::GIT_DIFF,
                json!({ "target": project_target(unborn_project.id) }),
            ),
        )
        .await,
    ))
    .unwrap();
    assert!(unborn_diff.text.contains("+first content"));
    daemon.stop().await;
}

#[tokio::test]
async fn git_status_rejects_arbitrary_filesystem_paths() {
    let temporary = TempDir::new().expect("temporary directory should exist");
    let daemon = RunningDaemon::start(temporary.path());
    let mut client = connect(daemon.config.socket_path()).await;
    let response = exchange(
        &mut client,
        &RequestEnvelope::v1(method::GIT_STATUS, json!({ "path": "/tmp/not-registered" })),
    )
    .await;
    assert_eq!(failure_code(response), "invalid_payload");
    daemon.stop().await;
}
