//! Daemon-level Beta v0.1 acceptance and failure coverage.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use cli_master_core::ApiError;
use cli_master_core::ipc::{self, codes};
use cli_master_daemon::{AppPaths, Daemon};
use serde_json::{Value, json};
use tempfile::TempDir;

fn open_daemon(root: &Path) -> Daemon {
    let paths = AppPaths::from_roots(root.join("data"), root.join("run")).expect("paths");
    Daemon::open(paths).expect("daemon")
}

fn init_repo(root: &Path) -> PathBuf {
    let repo = root.join("repo");
    fs::create_dir_all(&repo).expect("repo");
    run(&repo, &["git", "init", "-b", "main"]);
    run(&repo, &["git", "config", "user.email", "dev@example.com"]);
    run(&repo, &["git", "config", "user.name", "Dev"]);
    fs::write(repo.join("README.md"), "hello\n").expect("readme");
    run(&repo, &["git", "add", "README.md"]);
    run(&repo, &["git", "commit", "-m", "init"]);
    fs::canonicalize(&repo).expect("canon")
}

fn run(cwd: &Path, args: &[&str]) {
    let status = Command::new(args[0])
        .args(&args[1..])
        .current_dir(cwd)
        .status()
        .expect("spawn");
    assert!(status.success(), "{args:?} failed");
}

fn call(daemon: &Daemon, method: &str, payload: Value) -> Result<Value, ApiError> {
    daemon.dispatch(method, payload)
}

fn ok(daemon: &Daemon, method: &str, payload: Value) -> Value {
    call(daemon, method, payload).unwrap_or_else(|error| {
        panic!("{method} failed: {error}");
    })
}

fn err_code(daemon: &Daemon, method: &str, payload: Value) -> String {
    call(daemon, method, payload)
        .expect_err("expected failure")
        .code
}

fn add_project(daemon: &Daemon, path: &Path) -> Value {
    ok(
        daemon,
        ipc::PROJECT_ADD,
        json!({ "path": path, "name": "Demo" }),
    )
}

fn custom_agent(daemon: &Daemon, key: &str, executable: &str, args: &[&str]) -> Value {
    ok(
        daemon,
        ipc::AGENT_CUSTOM_CREATE,
        json!({
            "key": key,
            "displayName": key,
            "executable": executable,
            "args": args,
        }),
    )
}

fn create_session(daemon: &Daemon, project_id: &str, agent_id: &str, worktree: bool) -> Value {
    ok(
        daemon,
        ipc::SESSION_CREATE,
        json!({
            "projectId": project_id,
            "agentId": agent_id,
            "name": agent_id,
            "createWorktree": worktree,
            "cols": 80,
            "rows": 24,
        }),
    )
}

fn session_id(session: &Value) -> &str {
    session["id"].as_str().expect("session id")
}

fn wait_replay(daemon: &Daemon, id: &str, needle: &[u8]) -> Vec<u8> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let response = ok(daemon, ipc::SESSION_SUBSCRIBE, json!({ "sessionId": id }));
        let bytes = BASE64
            .decode(response["replayBase64"].as_str().unwrap_or_default())
            .unwrap_or_default();
        if bytes.windows(needle.len()).any(|window| window == needle) {
            return bytes;
        }
        assert!(
            Instant::now() <= deadline,
            "missing output {needle:?} in {bytes:?}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn wait_not_live(daemon: &Daemon, id: &str) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if !daemon
            .session_manager()
            .is_live(id.parse().expect("session id"))
        {
            return;
        }
        assert!(Instant::now() <= deadline, "session {id} still live");
        thread::sleep(Duration::from_millis(20));
    }
}

struct StopGuard<'a> {
    daemon: &'a Daemon,
    id: String,
}

impl Drop for StopGuard<'_> {
    fn drop(&mut self) {
        let _ = call(
            self.daemon,
            ipc::SESSION_STOP,
            json!({ "sessionId": self.id }),
        );
        let _ = call(
            self.daemon,
            ipc::SESSION_KILL,
            json!({ "sessionId": self.id }),
        );
    }
}

#[test]
fn hello_and_snapshot_report_beta_identity() {
    let temp = TempDir::new().expect("temp");
    let daemon = open_daemon(temp.path());
    let hello = ok(&daemon, ipc::SYSTEM_HELLO, json!({}));
    assert_eq!(hello["protocolVersion"], 1);
    assert_eq!(hello["appVersion"], "0.1.0-beta.1");
    assert_eq!(hello["platform"], "linux");
    let snapshot = ok(&daemon, ipc::STATE_SNAPSHOT, json!({}));
    assert!(snapshot["agents"].as_array().expect("agents").len() >= 4);
}

#[test]
fn project_add_exposes_name_path_and_branch() {
    let temp = TempDir::new().expect("temp");
    let repo = init_repo(temp.path());
    let daemon = open_daemon(temp.path());
    let project = add_project(&daemon, &repo);
    assert_eq!(project["name"], "Demo");
    assert_eq!(PathBuf::from(project["path"].as_str().expect("path")), repo);
    assert_eq!(project["currentBranch"], "main");
}

#[test]
fn custom_agent_and_working_tree_session_round_trip() {
    let temp = TempDir::new().expect("temp");
    let repo = init_repo(temp.path());
    let daemon = open_daemon(temp.path());
    let project = add_project(&daemon, &repo);
    let project_id = project["id"].as_str().expect("id");
    custom_agent(
        &daemon,
        "echoer",
        "python3",
        &[
            "-u",
            "-c",
            "import sys; data=sys.stdin.buffer.read(5); sys.stdout.buffer.write(data); sys.stdout.buffer.flush()",
        ],
    );
    let agents = ok(&daemon, ipc::AGENT_DETECT, json!({}));
    assert!(
        agents
            .as_array()
            .expect("agents")
            .iter()
            .any(|agent| agent["id"] == "echoer" && agent["detected"].as_bool() == Some(true))
    );
    let session = create_session(&daemon, project_id, "echoer", false);
    let id = session_id(&session);
    assert_eq!(
        session["cwd"].as_str().expect("cwd"),
        repo.to_str().unwrap()
    );
    ok(
        &daemon,
        ipc::SESSION_WRITE,
        json!({ "sessionId": id, "bytesBase64": BASE64.encode(b"alpha") }),
    );
    wait_replay(&daemon, id, b"alpha");
    ok(
        &daemon,
        ipc::SESSION_RESIZE,
        json!({ "sessionId": id, "cols": 40, "rows": 12 }),
    );
}

#[test]
fn stopping_one_session_does_not_stop_the_other() {
    let temp = TempDir::new().expect("temp");
    let repo = init_repo(temp.path());
    let daemon = open_daemon(temp.path());
    let project_id = add_project(&daemon, &repo)["id"]
        .as_str()
        .expect("id")
        .to_owned();
    custom_agent(&daemon, "sleeper", "/bin/sleep", &["30"]);
    let first = create_session(&daemon, &project_id, "sleeper", false);
    let second = create_session(&daemon, &project_id, "sleeper", false);
    let first_id = session_id(&first).to_owned();
    let second_id = session_id(&second).to_owned();
    let listed = ok(
        &daemon,
        ipc::SESSION_LIST,
        json!({ "projectId": project_id }),
    );
    assert_eq!(listed.as_array().expect("list").len(), 2);
    ok(&daemon, ipc::SESSION_STOP, json!({ "sessionId": first_id }));
    wait_not_live(&daemon, &first_id);
    let second_live = ok(&daemon, ipc::SESSION_GET, json!({ "sessionId": second_id }));
    assert_eq!(second_live["status"], "running");
    ok(
        &daemon,
        ipc::SESSION_STOP,
        json!({ "sessionId": second_id }),
    );
}

#[test]
fn dirty_worktree_removal_is_blocked_until_confirmed() {
    let temp = TempDir::new().expect("temp");
    let repo = init_repo(temp.path());
    let daemon = open_daemon(temp.path());
    let project_id = add_project(&daemon, &repo)["id"]
        .as_str()
        .expect("id")
        .to_owned();
    custom_agent(&daemon, "sleeper", "/bin/sleep", &["30"]);
    let session = create_session(&daemon, &project_id, "sleeper", true);
    let worktree_path = PathBuf::from(session["worktreePath"].as_str().expect("path"));
    let worktree_id = session["worktreeId"].as_str().expect("worktree").to_owned();
    assert!(
        session["branch"]
            .as_str()
            .expect("branch")
            .starts_with("agent/")
    );
    assert!(worktree_path.starts_with(daemon.paths().worktrees.join(&project_id)));
    fs::write(worktree_path.join("README.md"), "dirty change\n").expect("dirty");
    let status = ok(
        &daemon,
        ipc::GIT_STATUS,
        json!({ "projectId": project_id, "worktreeId": worktree_id }),
    );
    assert_eq!(status["isDirty"].as_bool(), Some(true));
    let diff = ok(
        &daemon,
        ipc::GIT_DIFF,
        json!({ "projectId": project_id, "worktreeId": worktree_id }),
    );
    assert!(
        diff["text"]
            .as_str()
            .unwrap_or_default()
            .contains("dirty change")
    );
    let running_plan = ok(
        &daemon,
        ipc::WORKTREE_PREPARE_REMOVE,
        json!({ "worktreeId": worktree_id }),
    );
    assert_eq!(
        err_code(
            &daemon,
            ipc::WORKTREE_REMOVE,
            json!({
                "worktreeId": worktree_id,
                "confirmationToken": running_plan["confirmationToken"],
                "allowDirty": false,
            }),
        ),
        codes::WORKTREE_IN_USE
    );
    let id = session_id(&session).to_owned();
    let _guard = StopGuard {
        daemon: &daemon,
        id: id.clone(),
    };
    ok(&daemon, ipc::SESSION_STOP, json!({ "sessionId": id }));
    wait_not_live(&daemon, &id);
    let dirty_plan = ok(
        &daemon,
        ipc::WORKTREE_PREPARE_REMOVE,
        json!({ "worktreeId": worktree_id }),
    );
    assert_eq!(dirty_plan["isDirty"].as_bool(), Some(true));
    assert_eq!(
        err_code(
            &daemon,
            ipc::WORKTREE_REMOVE,
            json!({
                "worktreeId": worktree_id,
                "confirmationToken": dirty_plan["confirmationToken"],
                "allowDirty": false,
            }),
        ),
        codes::WORKTREE_DIRTY
    );
}

#[test]
fn metadata_survives_restart_and_unknown_processes_are_reconciled() {
    let temp = TempDir::new().expect("temp");
    let repo = init_repo(temp.path());
    let first = open_daemon(temp.path());
    let project_id = add_project(&first, &repo)["id"]
        .as_str()
        .expect("id")
        .to_owned();
    custom_agent(&first, "sleeper", "/bin/sleep", &["30"]);
    let session = create_session(&first, &project_id, "sleeper", false);
    let session_id = session_id(&session).to_owned();
    drop(first);
    let second = open_daemon(temp.path());
    let snapshot = ok(&second, ipc::STATE_SNAPSHOT, json!({}));
    assert_eq!(snapshot["projects"][0]["name"], "Demo");
    let restored = snapshot["sessions"]
        .as_array()
        .expect("sessions")
        .iter()
        .find(|item| item["id"] == session_id)
        .expect("session");
    assert_eq!(restored["status"], "unknown");
    assert_eq!(second.session_manager().live_count(), 0);
}

#[test]
fn project_remove_does_not_delete_the_repository() {
    let temp = TempDir::new().expect("temp");
    let repo = init_repo(temp.path());
    let daemon = open_daemon(temp.path());
    let project_id = add_project(&daemon, &repo)["id"]
        .as_str()
        .expect("id")
        .to_owned();
    custom_agent(&daemon, "sleeper", "/bin/sleep", &["30"]);
    let session = create_session(&daemon, &project_id, "sleeper", true);
    let session_id = session_id(&session).to_owned();
    let worktree_id = session["worktreeId"].as_str().expect("wt").to_owned();
    let _guard = StopGuard {
        daemon: &daemon,
        id: session_id.clone(),
    };
    ok(
        &daemon,
        ipc::SESSION_STOP,
        json!({ "sessionId": session_id }),
    );
    wait_not_live(&daemon, &session_id);
    ok(
        &daemon,
        ipc::SESSION_DELETE,
        json!({ "sessionId": session_id }),
    );
    let plan = ok(
        &daemon,
        ipc::WORKTREE_PREPARE_REMOVE,
        json!({ "worktreeId": worktree_id }),
    );
    ok(
        &daemon,
        ipc::WORKTREE_REMOVE,
        json!({
            "worktreeId": worktree_id,
            "confirmationToken": plan["confirmationToken"],
            "allowDirty": false,
        }),
    );
    ok(
        &daemon,
        ipc::PROJECT_REMOVE,
        json!({ "projectId": project_id }),
    );
    assert!(repo.join("README.md").exists());
}

#[test]
fn unexpected_process_exit_is_observed() {
    let temp = TempDir::new().expect("temp");
    let repo = init_repo(temp.path());
    let daemon = open_daemon(temp.path());
    let project_id = add_project(&daemon, &repo)["id"]
        .as_str()
        .expect("id")
        .to_owned();
    custom_agent(&daemon, "true-agent", "/bin/true", &[]);
    let session = create_session(&daemon, &project_id, "true-agent", false);
    wait_not_live(&daemon, session_id(&session));
}

#[test]
fn failure_paths_return_actionable_codes() {
    let temp = TempDir::new().expect("temp");
    let repo = init_repo(temp.path());
    let daemon = open_daemon(temp.path());
    let project_id = add_project(&daemon, &repo)["id"]
        .as_str()
        .expect("id")
        .to_owned();
    assert_eq!(
        err_code(&daemon, "no.such.method", json!({})),
        codes::METHOD_NOT_FOUND
    );
    assert_eq!(
        err_code(
            &daemon,
            ipc::SESSION_CREATE,
            json!({ "projectId": project_id, "agentId": "missing" }),
        ),
        codes::AGENT_INVALID
    );
    assert_eq!(
        err_code(
            &daemon,
            ipc::AGENT_CUSTOM_CREATE,
            json!({
                "key": "evil",
                "displayName": "evil",
                "executable": "../bin/sh",
                "args": [],
            }),
        ),
        codes::AGENT_INVALID
    );
    custom_agent(&daemon, "missing-bin", "/tmp/cli-master-no-such-agent", &[]);
    assert_eq!(
        err_code(
            &daemon,
            ipc::SESSION_CREATE,
            json!({ "projectId": project_id, "agentId": "missing-bin" }),
        ),
        codes::AGENT_EXECUTABLE_NOT_FOUND
    );
}

#[test]
fn duplicate_start_and_stop_are_rejected() {
    let temp = TempDir::new().expect("temp");
    let repo = init_repo(temp.path());
    let daemon = open_daemon(temp.path());
    let project_id = add_project(&daemon, &repo)["id"]
        .as_str()
        .expect("id")
        .to_owned();
    custom_agent(&daemon, "sleeper", "/bin/sleep", &["30"]);
    let session = create_session(&daemon, &project_id, "sleeper", false);
    let id = session_id(&session).to_owned();
    assert_eq!(
        err_code(&daemon, ipc::SESSION_RESTART, json!({ "sessionId": id })),
        codes::SESSION_ALREADY_RUNNING
    );
    ok(&daemon, ipc::SESSION_STOP, json!({ "sessionId": id }));
    wait_not_live(&daemon, &id);
    assert_eq!(
        err_code(&daemon, ipc::SESSION_STOP, json!({ "sessionId": id })),
        codes::SESSION_NOT_RUNNING
    );
}

#[test]
fn ten_registered_sessions_can_be_listed_and_stopped() {
    let temp = TempDir::new().expect("temp");
    let repo = init_repo(temp.path());
    let daemon = open_daemon(temp.path());
    let project_id = add_project(&daemon, &repo)["id"]
        .as_str()
        .expect("id")
        .to_owned();
    custom_agent(&daemon, "sleeper", "/bin/sleep", &["30"]);
    let mut ids = Vec::new();
    for _ in 0..10 {
        ids.push(session_id(&create_session(&daemon, &project_id, "sleeper", false)).to_owned());
    }
    let listed = ok(
        &daemon,
        ipc::SESSION_LIST,
        json!({ "projectId": project_id }),
    );
    assert_eq!(listed.as_array().expect("list").len(), 10);
    assert!(daemon.session_manager().live_count() >= 4);
    for id in ids {
        let _ = call(&daemon, ipc::SESSION_STOP, json!({ "sessionId": id }));
    }
}

#[test]
fn moved_project_is_reported_when_starting_a_session() {
    let temp = TempDir::new().expect("temp");
    let repo = init_repo(temp.path());
    let daemon = open_daemon(temp.path());
    let project_id = add_project(&daemon, &repo)["id"]
        .as_str()
        .expect("id")
        .to_owned();
    custom_agent(&daemon, "sleeper", "/bin/sleep", &["30"]);
    let moved = temp.path().join("moved-repo");
    fs::rename(&repo, &moved).expect("rename");
    assert_eq!(
        err_code(
            &daemon,
            ipc::SESSION_CREATE,
            json!({ "projectId": project_id, "agentId": "sleeper" }),
        ),
        codes::PROJECT_MOVED
    );
}
