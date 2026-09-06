use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use cli_master_core::{ApiError, RequestEnvelope, ResponseEnvelope, ResponsePayload};
use cli_master_daemon::{Daemon, DaemonConfig, DaemonError, MAX_FRAME_LENGTH};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::net::UnixStream;
use tokio::task::JoinHandle;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tokio_util::sync::CancellationToken;

const MAX_TEXT_BYTES: usize = 128 * 1024;
const MAX_LIST_BYTES: usize = 512 * 1024;
const INITIAL: &str = "\u{feff}first\r\nsecond\r\n";

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
    selected: PathBuf,
    target: Value,
    project_id: Value,
    agent_id: Value,
}

impl Fixture {
    async fn new() -> (Self, RunningDaemon, Client) {
        let root = TempDir::new().unwrap();
        let repository = root.path().join("repository");
        let selected = repository.join("apps");
        fs::create_dir_all(selected.join("api")).unwrap();
        fs::write(selected.join("api/notes.txt"), INITIAL).unwrap();
        fs::write(repository.join("outside.txt"), "outside selected project\n").unwrap();
        git(&repository, &["init", "-b", "main"]);
        git(
            &repository,
            &["config", "user.email", "tests@example.invalid"],
        );
        git(&repository, &["config", "user.name", "CLI Master Tests"]);
        git(&repository, &["add", "."]);
        git(&repository, &["commit", "-m", "initial"]);
        let daemon = RunningDaemon::start(root.path());
        let mut client = daemon.connect().await;
        let project = call(&mut client, "project.add", json!({"path": selected})).await;
        let agent = call(
            &mut client,
            "agent.custom.create",
            json!({
                "displayName": "File fixture",
                "command": {"executable": "/bin/cat", "args": [], "env": {}}
            }),
        )
        .await;
        (
            Self {
                root,
                repository,
                selected,
                target: json!({"kind": "project", "projectId": project["id"]}),
                project_id: project["id"].clone(),
                agent_id: agent["id"].clone(),
            },
            daemon,
            client,
        )
    }

    async fn session(&self, client: &mut Client, isolation: &str) -> Value {
        call(
            client,
            "session.create",
            json!({
                "projectId": self.project_id,
                "agentId": self.agent_id,
                "name": "File session",
                "isolation": isolation,
                "relativeDirectory": "api"
            }),
        )
        .await
    }
}

fn git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn path(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

fn request(target: &Value, bytes: &[u8]) -> Value {
    json!({"target": target, "pathBase64": path(bytes)})
}

fn write_request(target: &Value, bytes: &[u8], text: &str, revision: &str) -> Value {
    json!({"target": target, "pathBase64": path(bytes), "text": text, "expectedRevision": revision})
}

async fn exchange(client: &mut Client, method: &str, payload: Value) -> ResponseEnvelope<Value> {
    let request = RequestEnvelope::v1(method, payload);
    let encoded = serde_json::to_vec(&request).unwrap();
    assert!(
        encoded.len() <= MAX_FRAME_LENGTH,
        "test request must fit the wire frame"
    );
    client.send(encoded.into()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let frame = client
                .next()
                .await
                .expect("response should arrive")
                .unwrap();
            assert!(frame.len() <= MAX_FRAME_LENGTH);
            let envelope: Value = serde_json::from_slice(&frame).unwrap();
            if envelope["kind"] == "event" {
                continue;
            }
            let response: ResponseEnvelope<Value> = serde_json::from_value(envelope).unwrap();
            assert_eq!(response.request_id, request.request_id);
            return response;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{method} must not hang"))
}

async fn call(client: &mut Client, method: &str, payload: Value) -> Value {
    match exchange(client, method, payload).await.payload {
        ResponsePayload::Success { data } => data,
        ResponsePayload::Error { error } => panic!("{method} failed: {error:?}"),
    }
}

async fn failure(client: &mut Client, method: &str, payload: Value) -> ApiError {
    match exchange(client, method, payload).await.payload {
        ResponsePayload::Error { error } => error,
        ResponsePayload::Success { data } => panic!("{method} unexpectedly succeeded: {data}"),
    }
}

async fn read(client: &mut Client, target: &Value, bytes: &[u8]) -> Value {
    call(client, "file.read", request(target, bytes)).await
}

fn revision(response: &Value) -> &str {
    let revision = response["revision"]
        .as_str()
        .expect("opaque revision should be present");
    assert_eq!(revision.len(), 67);
    assert!(revision.starts_with("v1:"));
    assert!(
        revision.as_bytes()[3..]
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    );
    revision
}

fn entry_paths(response: &Value) -> Vec<Vec<u8>> {
    response["entries"]
        .as_array()
        .expect("directory entries")
        .iter()
        .map(|entry| {
            STANDARD
                .decode(entry["pathBase64"].as_str().unwrap())
                .unwrap()
        })
        .collect()
}

#[tokio::test]
async fn registered_project_session_and_worktree_roots_list_read_and_save_real_bytes() {
    let (fixture, daemon, mut client) = Fixture::new().await;
    let session = fixture.session(&mut client, "current").await;
    let isolated = fixture.session(&mut client, "new_worktree").await;
    let session_target = json!({"kind": "session", "sessionId": session["id"]});
    let isolated_target = json!({"kind": "worktree", "worktreeId": isolated["worktreeId"]});
    let isolated_root = PathBuf::from(isolated["worktreePath"].as_str().unwrap());
    let cases = [
        (
            fixture.target.clone(),
            b"api/notes.txt".as_slice(),
            fixture.selected.join("api/notes.txt"),
        ),
        (
            session_target,
            b"notes.txt".as_slice(),
            fixture.selected.join("api/notes.txt"),
        ),
        (
            isolated_target,
            b"apps/api/notes.txt".as_slice(),
            isolated_root.join("apps/api/notes.txt"),
        ),
    ];
    for (index, (target, name, disk)) in cases.iter().enumerate() {
        let listed = call(&mut client, "file.list", request(target, b"")).await;
        assert!(!entry_paths(&listed).is_empty());
        assert!(listed["observedAtMs"].as_i64().unwrap() > 1_700_000_000_000);
        let before = read(&mut client, target, name).await;
        assert_eq!(before["text"], fs::read_to_string(disk).unwrap());
        let text = format!("\u{feff}olá {index} 🦀\r\nkeep CRLF\r\n");
        let saved = call(
            &mut client,
            "file.write",
            write_request(target, name, &text, revision(&before)),
        )
        .await;
        assert_eq!(saved["pathBase64"], path(name));
        assert_eq!(saved["sizeBytes"], text.len());
        assert_ne!(revision(&before), revision(&saved));
        assert_eq!(fs::read(disk).unwrap(), text.as_bytes());
        let after = read(&mut client, target, name).await;
        assert_eq!(after["text"], text);
        assert_eq!(revision(&after), revision(&saved));
    }
    let listed = call(&mut client, "file.list", request(&fixture.target, b"")).await;
    assert_eq!(entry_paths(&listed), [b"api".to_vec()]);
    assert_eq!(
        failure(
            &mut client,
            "file.read",
            request(&fixture.target, b"outside.txt")
        )
        .await
        .code,
        "file_not_found"
    );
    assert_eq!(
        fs::read_to_string(fixture.repository.join("outside.txt")).unwrap(),
        "outside selected project\n"
    );
    daemon.stop().await;
}

#[tokio::test]
async fn unix_filename_bytes_round_trip_through_listing_and_followup_identifiers() {
    let (fixture, daemon, mut client) = Fixture::new().await;
    let mut names: Vec<Vec<u8>> = vec![
        b"space name.txt".to_vec(),
        b"-leading.txt".to_vec(),
        b"literal\\name:part".to_vec(),
        "ação.txt".as_bytes().to_vec(),
        b"line\n\x01.txt".to_vec(),
    ];
    let non_utf8_name = b"bad-\xff.txt".to_vec();
    match fs::write(
        fixture
            .selected
            .join(OsString::from_vec(non_utf8_name.clone())),
        "before",
    ) {
        Ok(()) => names.push(non_utf8_name),
        // APFS rejects invalid UTF-8 before it reaches the file service. Linux
        // must exercise this case; the pure wire contract covers it on both OSes.
        Err(error)
            if cfg!(target_os = "macos")
                && error.raw_os_error() == Some(rustix::io::Errno::ILSEQ.raw_os_error()) => {}
        Err(error) => panic!("non-UTF-8 fixture failed unexpectedly: {error}"),
    }
    for name in &names {
        fs::write(
            fixture.selected.join(OsString::from_vec(name.clone())),
            "before",
        )
        .unwrap();
    }
    let listing = call(&mut client, "file.list", request(&fixture.target, b"")).await;
    let paths = entry_paths(&listing);
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted, "sorting must follow raw Unix bytes");
    for name in &names {
        let entry = listing["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| {
                STANDARD
                    .decode(entry["pathBase64"].as_str().unwrap())
                    .unwrap()
                    == *name
            })
            .expect("every byte-exact filename should be listable");
        assert!(
            !entry["displayName"]
                .as_str()
                .unwrap()
                .chars()
                .any(char::is_control)
        );
        let payload = json!({"target": fixture.target, "pathBase64": entry["pathBase64"]});
        let before = call(&mut client, "file.read", payload.clone()).await;
        let mut write = payload;
        write["text"] = json!("after 🦀\r\n");
        write["expectedRevision"] = before["revision"].clone();
        call(&mut client, "file.write", write).await;
        assert_eq!(
            fs::read(fixture.selected.join(OsString::from_vec(name.clone()))).unwrap(),
            "after 🦀\r\n".as_bytes()
        );
    }
    daemon.stop().await;
}

#[tokio::test]
async fn invalid_paths_revision_and_unregistered_targets_are_rejected_at_the_wire_boundary() {
    let (fixture, daemon, mut client) = Fixture::new().await;
    let invalid: &[&[u8]] = &[
        b"",
        b"/absolute.txt",
        b"..",
        b"api/../notes.txt",
        b"api//notes.txt",
        b"api/./notes.txt",
        b"api/",
        b"api\0notes.txt",
    ];
    for bytes in invalid {
        assert_eq!(
            failure(&mut client, "file.read", request(&fixture.target, bytes))
                .await
                .code,
            "invalid_input",
            "{bytes:?}"
        );
    }
    for encoded in ["%%%", "YQ", "YR=="] {
        assert_eq!(
            failure(
                &mut client,
                "file.read",
                json!({"target": fixture.target, "pathBase64": encoded})
            )
            .await
            .code,
            "invalid_input"
        );
    }
    assert_eq!(
        failure(
            &mut client,
            "file.read",
            request(&fixture.target, &vec![b'a'; 4097])
        )
        .await
        .code,
        "invalid_input"
    );
    assert_eq!(
        failure(
            &mut client,
            "file.write",
            write_request(
                &fixture.target,
                b"api/notes.txt",
                "changed",
                "not-a-revision"
            )
        )
        .await
        .code,
        "invalid_input"
    );
    let missing_target = json!({"kind": "project", "projectId": cli_master_core::ProjectId::new()});
    assert_eq!(
        failure(
            &mut client,
            "file.read",
            request(&missing_target, b"notes.txt")
        )
        .await
        .code,
        "file_target_not_found"
    );
    let mut extra_path = request(&fixture.target, b"api/notes.txt");
    extra_path["path"] = json!(fixture.repository.join("outside.txt"));
    assert_eq!(
        failure(&mut client, "file.read", extra_path).await.code,
        "invalid_input"
    );
    assert_eq!(
        fs::read_to_string(fixture.selected.join("api/notes.txt")).unwrap(),
        INITIAL
    );
    daemon.stop().await;
}

#[tokio::test]
async fn symlinks_directories_and_fifo_leaves_are_never_opened_as_regular_text() {
    let (fixture, daemon, mut client) = Fixture::new().await;
    symlink(
        fixture.selected.join("api/notes.txt"),
        fixture.selected.join("link.txt"),
    )
    .unwrap();
    symlink(
        fixture.selected.join("api"),
        fixture.selected.join("linked-dir"),
    )
    .unwrap();
    let before = read(&mut client, &fixture.target, b"api/notes.txt").await;
    for bytes in [b"link.txt".as_slice(), b"linked-dir/notes.txt".as_slice()] {
        assert_eq!(
            failure(&mut client, "file.read", request(&fixture.target, bytes))
                .await
                .code,
            "file_symlink_not_allowed"
        );
        assert_eq!(
            failure(
                &mut client,
                "file.write",
                write_request(&fixture.target, bytes, "changed", revision(&before))
            )
            .await
            .code,
            "file_symlink_not_allowed"
        );
    }
    assert_eq!(
        failure(&mut client, "file.read", request(&fixture.target, b"api"))
            .await
            .code,
        "file_not_regular"
    );
    assert_eq!(
        failure(
            &mut client,
            "file.list",
            request(&fixture.target, b"api/notes.txt")
        )
        .await
        .code,
        "file_not_directory"
    );

    let fifo = fixture.selected.join("api/notes.txt");
    fs::remove_file(&fifo).unwrap();
    let output = Command::new("mkfifo").arg(&fifo).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        failure(
            &mut client,
            "file.read",
            request(&fixture.target, b"api/notes.txt")
        )
        .await
        .code,
        "file_not_regular"
    );
    assert_eq!(
        failure(
            &mut client,
            "file.write",
            write_request(
                &fixture.target,
                b"api/notes.txt",
                "changed",
                revision(&before)
            )
        )
        .await
        .code,
        "file_not_regular"
    );
    let listing = call(&mut client, "file.list", request(&fixture.target, b"api")).await;
    assert_eq!(listing["entries"][0]["kind"], "other");
    let root_listing = call(&mut client, "file.list", request(&fixture.target, b"")).await;
    assert_eq!(
        root_listing["entries"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| entry["kind"] == "symlink")
            .count(),
        2
    );
    daemon.stop().await;
}

#[tokio::test]
async fn a_replaced_registered_root_and_pending_worktree_removal_reject_writes() {
    let (fixture, daemon, mut client) = Fixture::new().await;
    let before = read(&mut client, &fixture.target, b"api/notes.txt").await;
    let moved = fixture.root.path().join("moved-apps");
    fs::rename(&fixture.selected, &moved).unwrap();
    symlink(&moved, &fixture.selected).unwrap();
    let read_error = failure(
        &mut client,
        "file.read",
        request(&fixture.target, b"api/notes.txt"),
    )
    .await;
    let write_error = failure(
        &mut client,
        "file.write",
        write_request(
            &fixture.target,
            b"api/notes.txt",
            "changed",
            revision(&before),
        ),
    )
    .await;
    fs::remove_file(&fixture.selected).unwrap();
    fs::rename(&moved, &fixture.selected).unwrap();
    assert_eq!(read_error.code, "file_target_changed");
    assert_eq!(write_error.code, "file_target_changed");
    assert_eq!(
        fs::read_to_string(fixture.selected.join("api/notes.txt")).unwrap(),
        INITIAL
    );

    let isolated = fixture.session(&mut client, "new_worktree").await;
    let target = json!({"kind": "worktree", "worktreeId": isolated["worktreeId"]});
    let before = read(&mut client, &target, b"apps/api/notes.txt").await;
    let prepared = call(
        &mut client,
        "worktree.prepare_remove",
        json!({"worktreeId": isolated["worktreeId"]}),
    )
    .await;
    assert_eq!(prepared["status"], "ready");
    assert_eq!(
        failure(
            &mut client,
            "file.write",
            write_request(&target, b"apps/api/notes.txt", "changed", revision(&before))
        )
        .await
        .code,
        "file_target_changed"
    );
    assert_eq!(
        fs::read_to_string(
            Path::new(isolated["worktreePath"].as_str().unwrap()).join("apps/api/notes.txt")
        )
        .unwrap(),
        INITIAL
    );
    daemon.stop().await;
}

#[tokio::test]
async fn binary_oversize_and_unsupported_hardlinks_are_rejected_without_data_loss() {
    let (fixture, daemon, mut client) = Fixture::new().await;
    for (name, bytes, code) in [
        ("nul.bin", vec![b'a', 0, b'b'], "file_not_text"),
        ("invalid.bin", vec![0xff, 0xfe], "file_not_text"),
        (
            "large.txt",
            vec![b'x'; MAX_TEXT_BYTES + 1],
            "file_too_large",
        ),
    ] {
        fs::write(fixture.selected.join(name), &bytes).unwrap();
        assert_eq!(
            failure(
                &mut client,
                "file.read",
                request(&fixture.target, name.as_bytes())
            )
            .await
            .code,
            code
        );
        assert_eq!(fs::read(fixture.selected.join(name)).unwrap(), bytes);
    }
    let before = read(&mut client, &fixture.target, b"api/notes.txt").await;
    let too_large = "é".repeat(MAX_TEXT_BYTES / 2 + 1);
    assert_eq!(
        failure(
            &mut client,
            "file.write",
            write_request(
                &fixture.target,
                b"api/notes.txt",
                &too_large,
                revision(&before)
            )
        )
        .await
        .code,
        "invalid_input"
    );
    assert_eq!(
        failure(
            &mut client,
            "file.write",
            write_request(&fixture.target, b"missing.txt", "new", revision(&before))
        )
        .await
        .code,
        "file_not_found"
    );
    assert!(!fixture.selected.join("missing.txt").exists());

    let original = fixture.selected.join("api/notes.txt");
    let linked = fixture.root.path().join("hardlink.txt");
    fs::hard_link(&original, &linked).unwrap();
    let linked_read = read(&mut client, &fixture.target, b"api/notes.txt").await;
    assert_eq!(
        failure(
            &mut client,
            "file.write",
            write_request(
                &fixture.target,
                b"api/notes.txt",
                "changed",
                revision(&linked_read)
            )
        )
        .await
        .code,
        "file_metadata_unsupported"
    );
    assert_eq!(fs::read_to_string(&original).unwrap(), INITIAL);
    assert_eq!(fs::read_to_string(&linked).unwrap(), INITIAL);
    assert_eq!(
        fs::metadata(original).unwrap().ino(),
        fs::metadata(linked).unwrap().ino()
    );
    daemon.stop().await;
}

#[tokio::test]
async fn two_clients_cannot_both_save_one_revision_even_through_different_registered_targets() {
    let (fixture, daemon, mut project_client) = Fixture::new().await;
    let session = fixture.session(&mut project_client, "current").await;
    let session_target = json!({"kind": "session", "sessionId": session["id"]});
    let mut session_client = daemon.connect().await;
    let project_read = read(&mut project_client, &fixture.target, b"api/notes.txt").await;
    let session_read = read(&mut session_client, &session_target, b"notes.txt").await;
    assert_eq!(revision(&project_read), revision(&session_read));
    let writes = ["project wins\n", "session wins\n"];
    let (first, second) = tokio::join!(
        exchange(
            &mut project_client,
            "file.write",
            write_request(
                &fixture.target,
                b"api/notes.txt",
                writes[0],
                revision(&project_read)
            )
        ),
        exchange(
            &mut session_client,
            "file.write",
            write_request(
                &session_target,
                b"notes.txt",
                writes[1],
                revision(&session_read)
            )
        ),
    );
    let mut winner = None;
    let mut conflicts = 0;
    for (response, candidate) in [first, second].into_iter().zip(writes) {
        match response.payload {
            ResponsePayload::Success { data } => {
                assert!(
                    winner.replace(candidate).is_none(),
                    "one revision must have exactly one successful writer"
                );
                revision(&data);
            }
            ResponsePayload::Error { error } => {
                assert_eq!(error.code, "file_conflict");
                assert!(!format!("{error:?}").contains(INITIAL));
                conflicts += 1;
            }
        }
    }
    assert_eq!(conflicts, 1);
    assert_eq!(
        fs::read_to_string(fixture.selected.join("api/notes.txt")).unwrap(),
        winner.unwrap()
    );
    daemon.stop().await;
}

#[tokio::test]
async fn external_content_and_inode_changes_conflict_and_saves_preserve_unix_permissions() {
    let (fixture, daemon, mut client) = Fixture::new().await;
    let disk = fixture.selected.join("api/notes.txt");
    fs::set_permissions(&disk, fs::Permissions::from_mode(0o640)).unwrap();
    let before = read(&mut client, &fixture.target, b"api/notes.txt").await;
    fs::write(&disk, "external content\n").unwrap();
    assert_eq!(
        failure(
            &mut client,
            "file.write",
            write_request(
                &fixture.target,
                b"api/notes.txt",
                "stale",
                revision(&before)
            )
        )
        .await
        .code,
        "file_conflict"
    );
    assert_eq!(fs::read_to_string(&disk).unwrap(), "external content\n");
    let before = read(&mut client, &fixture.target, b"api/notes.txt").await;
    let replacement = fixture.selected.join("api/replacement.txt");
    fs::write(&replacement, "external content\n").unwrap();
    fs::set_permissions(&replacement, fs::Permissions::from_mode(0o640)).unwrap();
    fs::rename(&replacement, &disk).unwrap();
    assert_eq!(
        failure(
            &mut client,
            "file.write",
            write_request(
                &fixture.target,
                b"api/notes.txt",
                "stale",
                revision(&before)
            )
        )
        .await
        .code,
        "file_conflict"
    );

    let metadata = fs::metadata(&disk).unwrap();
    let before = read(&mut client, &fixture.target, b"api/notes.txt").await;
    call(
        &mut client,
        "file.write",
        write_request(
            &fixture.target,
            b"api/notes.txt",
            "current\r\n",
            revision(&before),
        ),
    )
    .await;
    let saved = fs::metadata(&disk).unwrap();
    assert_eq!(saved.permissions().mode() & 0o777, 0o640);
    assert_eq!(saved.uid(), metadata.uid());
    assert_eq!(saved.gid(), metadata.gid());
    assert_eq!(fs::read(&disk).unwrap(), b"current\r\n");
    daemon.stop().await;
}

#[tokio::test]
async fn saved_text_and_revision_survive_daemon_restart_and_remain_writable() {
    let (fixture, first, mut client) = Fixture::new().await;
    let before = read(&mut client, &fixture.target, b"api/notes.txt").await;
    let saved = call(
        &mut client,
        "file.write",
        write_request(
            &fixture.target,
            b"api/notes.txt",
            "persisted\r\n",
            revision(&before),
        ),
    )
    .await;
    drop(client);
    first.stop().await;
    let second = RunningDaemon::start(fixture.root.path());
    let mut client = second.connect().await;
    let recovered = read(&mut client, &fixture.target, b"api/notes.txt").await;
    assert_eq!(recovered["text"], "persisted\r\n");
    assert_eq!(revision(&recovered), revision(&saved));
    call(
        &mut client,
        "file.write",
        write_request(
            &fixture.target,
            b"api/notes.txt",
            "saved after restart\n",
            revision(&saved),
        ),
    )
    .await;
    assert_eq!(
        fs::read_to_string(fixture.selected.join("api/notes.txt")).unwrap(),
        "saved after restart\n"
    );
    second.stop().await;
}

#[tokio::test]
async fn pagination_is_byte_ordered_and_rejects_unbounded_enumerations() {
    let (fixture, daemon, mut client) = Fixture::new().await;
    let pages = fixture.selected.join("pages");
    fs::create_dir(&pages).unwrap();
    for index in 0..205 {
        fs::write(pages.join(format!("{index:03}.txt")), []).unwrap();
    }
    let first = call(&mut client, "file.list", request(&fixture.target, b"pages")).await;
    assert_eq!(first["entries"].as_array().unwrap().len(), 100);
    assert_eq!(first["nextAfterNameBase64"], path(b"099.txt"));
    let second = call(
        &mut client,
        "file.list",
        json!({
            "target": fixture.target, "pathBase64": path(b"pages"), "limit": 200,
            "afterNameBase64": first["nextAfterNameBase64"]
        }),
    )
    .await;
    assert_eq!(second["entries"].as_array().unwrap().len(), 105);
    assert!(second.get("nextAfterNameBase64").is_none());
    let combined = [entry_paths(&first), entry_paths(&second)].concat();
    assert_eq!(
        combined,
        (0..205)
            .map(|index| format!("pages/{index:03}.txt").into_bytes())
            .collect::<Vec<_>>()
    );
    for limit in [0, 201] {
        assert_eq!(
            failure(
                &mut client,
                "file.list",
                json!({"target": fixture.target, "pathBase64": path(b"pages"), "limit": limit})
            )
            .await
            .code,
            "invalid_input"
        );
    }
    let large = fixture.selected.join("many");
    fs::create_dir(&large).unwrap();
    for index in 0..10_001 {
        fs::write(large.join(format!("{index:05}")), []).unwrap();
    }
    assert_eq!(
        failure(&mut client, "file.list", request(&fixture.target, b"many"))
            .await
            .code,
        "file_listing_too_large"
    );
    daemon.stop().await;
}

#[tokio::test]
async fn maximum_text_with_worst_case_json_escaping_round_trips_inside_one_frame() {
    let (fixture, daemon, mut client) = Fixture::new().await;
    let text = "\u{0001}".repeat(MAX_TEXT_BYTES);
    fs::write(fixture.selected.join("escaped.txt"), &text).unwrap();
    let before = read(&mut client, &fixture.target, b"escaped.txt").await;
    assert_eq!(before["text"], text);
    assert_eq!(before["sizeBytes"], MAX_TEXT_BYTES);
    assert!(serde_json::to_vec(&before).unwrap().len() > 6 * MAX_TEXT_BYTES);
    let updated = "\u{0002}".repeat(MAX_TEXT_BYTES);
    let saved = call(
        &mut client,
        "file.write",
        write_request(&fixture.target, b"escaped.txt", &updated, revision(&before)),
    )
    .await;
    assert_eq!(
        fs::read(fixture.selected.join("escaped.txt")).unwrap(),
        updated.as_bytes()
    );
    assert_eq!(
        revision(&read(&mut client, &fixture.target, b"escaped.txt").await),
        revision(&saved)
    );
    daemon.stop().await;
}

#[tokio::test]
async fn directory_pages_respect_the_encoded_response_budget_without_losing_names() {
    let (fixture, daemon, mut client) = Fixture::new().await;
    let mut directory = fs::File::open(&fixture.selected).unwrap();
    let mut relative = Vec::new();
    for index in 0..20 {
        let component = format!("d{index:02}{}", "a".repeat(117));
        rustix::fs::mkdirat(&directory, &component, rustix::fs::Mode::RWXU).unwrap();
        directory = rustix::fs::openat(
            &directory,
            &component,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::DIRECTORY
                | rustix::fs::OFlags::NOFOLLOW,
            rustix::fs::Mode::empty(),
        )
        .unwrap()
        .into();
        if !relative.is_empty() {
            relative.push(b'/');
        }
        relative.extend_from_slice(component.as_bytes());
    }
    for index in 0..200 {
        let name = format!("n{index:03}{}", "b".repeat(216));
        rustix::fs::openat(
            &directory,
            &name,
            rustix::fs::OFlags::WRONLY | rustix::fs::OFlags::CREATE | rustix::fs::OFlags::EXCL,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        )
        .unwrap();
    }
    let mut cursor: Option<Value> = None;
    let mut all_paths = Vec::new();
    loop {
        let mut payload =
            json!({"target": fixture.target, "pathBase64": path(&relative), "limit": 200});
        if let Some(after) = &cursor {
            payload["afterNameBase64"] = after.clone();
        }
        let page = call(&mut client, "file.list", payload).await;
        assert!(serde_json::to_vec(&page).unwrap().len() <= MAX_LIST_BYTES);
        let names = entry_paths(&page);
        assert!(!names.is_empty());
        if cursor.is_none() {
            assert!(
                names.len() < 200,
                "encoded budget must paginate before entry limit"
            );
        }
        all_paths.extend(names);
        assert!(
            all_paths.len() <= 200,
            "pagination must not repeat previous names"
        );
        cursor = page.get("nextAfterNameBase64").cloned();
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(all_paths.len(), 200);
    let mut unique = all_paths.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(all_paths, unique);
    daemon.stop().await;
}
