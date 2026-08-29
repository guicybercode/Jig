mod support;

use std::{
    fs,
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant},
};

use cli_master_git::{ChangeKind, Git, GitErrorKind, slugify};
use support::{RepositoryFixture, command};
use tempfile::TempDir;

#[test]
fn discovers_repository_from_subdirectory_and_reports_branch() {
    let fixture = RepositoryFixture::new();
    let nested = fixture.repository.join("src/nested");
    fs::create_dir_all(&nested).expect("nested directory should be created");
    let git = Git::discover().expect("Git should be installed for integration tests");

    let inspection = git
        .inspect_repository(&nested)
        .expect("repository should be inspected");

    assert!(inspection.is_repository());
    assert_eq!(
        inspection.repository_root,
        Some(
            fixture
                .repository
                .canonicalize()
                .expect("repository should canonicalize")
        )
    );
    assert_eq!(inspection.branch.as_deref(), Some("main"));
}

#[test]
fn status_reports_staged_tracked_deleted_and_untracked_paths() {
    let fixture = RepositoryFixture::new();
    fs::write(fixture.repository.join("tracked.txt"), "modified\n")
        .expect("tracked file should be modified");
    fs::remove_file(fixture.repository.join("delete-me.txt"))
        .expect("tracked file should be deleted");
    fs::write(fixture.repository.join("added.txt"), "added\n")
        .expect("added file should be written");
    command(&fixture.repository, ["add", "added.txt"]);
    fs::write(fixture.repository.join("untracked.txt"), "untracked\n")
        .expect("untracked file should be written");
    let git = Git::discover().expect("Git should be discovered");

    let status = git
        .status(&fixture.repository)
        .expect("status should parse");

    assert_eq!(status.branch.as_deref(), Some("main"));
    assert_eq!(status.counts.modified, 1);
    assert_eq!(status.counts.added, 1);
    assert_eq!(status.counts.deleted, 1);
    assert_eq!(status.counts.untracked, 1);
    assert!(status.has_staged);
    assert!(status.has_tracked_changes);
    assert!(status.has_untracked);
    assert!(status.files.iter().any(|file| {
        file.path == Path::new("untracked.txt") && file.kind == ChangeKind::Untracked
    }));
}

#[test]
fn branch_generation_is_ascii_and_adds_collision_suffix() {
    let fixture = RepositoryFixture::new();
    let git = Git::discover().expect("Git should be discovered");
    let first = git
        .generate_branch_name(&fixture.repository, "Olá / Auth API", "abc123")
        .expect("branch should be generated");
    assert!(first.is_ascii());
    assert_eq!(first, "agent/ola-auth-api-abc123");
    command(&fixture.repository, ["branch", &first]);

    let second = git
        .generate_branch_name(&fixture.repository, "Olá / Auth API", "abc123")
        .expect("colliding branch should be generated");
    assert_eq!(second, format!("{first}-2"));
    assert_eq!(slugify("Olá / Auth API"), "ola-auth-api");
}

#[test]
fn diff_is_bounded_and_reports_truncation() {
    let fixture = RepositoryFixture::new();
    fs::write(fixture.repository.join("tracked.txt"), "changed line\n")
        .expect("tracked file should be modified");
    let git = Git::discover().expect("Git should be discovered");
    let diff = git
        .diff(&fixture.repository, 16)
        .expect("diff should be generated");
    assert!(diff.text.len() <= 16);
    assert!(diff.truncated);
    assert!(!diff.text.contains("\u{1b}["));
}

#[test]
fn unborn_repository_diff_reports_staged_initial_content() {
    let temp = TempDir::new().expect("temporary directory should be created");
    command(temp.path(), ["init", "-b", "main"]);
    fs::write(temp.path().join("initial.txt"), "first content\n")
        .expect("initial file should be written");
    command(temp.path(), ["add", "initial.txt"]);
    let git = Git::discover().expect("Git should be discovered");

    let diff = git
        .diff(temp.path(), 64 * 1024)
        .expect("unborn repository diff should be generated");

    assert!(!diff.truncated);
    assert!(diff.text.contains("initial.txt"));
    assert!(diff.text.contains("+first content"));
}

#[cfg(unix)]
#[test]
fn timeout_kills_process_group_without_waiting_for_descendant_pipe() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().expect("temporary directory should be created");
    let executable = temp.path().join("fake-git");
    let process_group_leader_pid = temp.path().join("leader.pid");
    let child_pid = temp.path().join("child.pid");
    let script = format!(
        "#!/bin/sh\n\
         if [ \"$1\" = \"--version\" ]; then\n\
           echo 'git version 99.0.0'\n\
           exit 0\n\
         fi\n\
         echo $$ > '{}'\n\
         sleep 30 &\n\
         echo $! > '{}'\n\
         wait\n",
        process_group_leader_pid.display(),
        child_pid.display()
    );
    fs::write(&executable, script).expect("fake Git should be written");
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
        .expect("fake Git should be executable");
    let git = Git::with_executable(&executable)
        .expect("fake Git should validate")
        .with_timeout(Duration::from_millis(250))
        .expect("timeout should be valid");

    let started = Instant::now();
    let error = git
        .inspect_repository(temp.path())
        .expect_err("fake Git should time out");

    assert_eq!(error.kind(), GitErrorKind::Timeout);
    assert!(started.elapsed() < Duration::from_secs(2));
    let pid = fs::read_to_string(&child_pid)
        .expect("descendant should report its pid")
        .trim()
        .to_owned()
}

#[cfg(unix)]
fn assert_process_stopped(pid: &str, process: &str) {
    let stopped = (0..50).any(|_| {
        if process_is_live(pid) {
            thread::sleep(Duration::from_millis(10));
            false
        } else {
            true
        }
    });
    assert!(stopped, "{process} {pid} survived timeout");
}

#[cfg(unix)]
fn process_is_live(pid: &str) -> bool {
    let output = Command::new("/bin/ps")
        .args(["-p", pid, "-o", "stat="])
        .output()
        .expect("ps should execute");
    if !output.status.success() {
        return false;
    }
    let state = String::from_utf8_lossy(&output.stdout);
    !state.trim_start().starts_with('Z') && !state.trim().is_empty()
}
