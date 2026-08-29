use std::{
    ffi::OsStr,
    fs,
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant},
};

use cli_master_git::{ChangeKind, Git, GitErrorKind, RemovalBlocker, WorktreeUse, slugify};
use tempfile::TempDir;

struct RepositoryFixture {
    _temp: TempDir,
    repository: std::path::PathBuf,
    managed: std::path::PathBuf,
}

impl RepositoryFixture {
    fn new() -> Self {
        let temp = TempDir::new().expect("temporary directory should be created");
        let repository = temp.path().join("repository");
        let managed = temp.path().join("managed");
        fs::create_dir(&repository).expect("repository directory should be created");
        command(&repository, ["init", "-b", "main"]);
        command(
            &repository,
            ["config", "user.email", "tests@example.invalid"],
        );
        command(&repository, ["config", "user.name", "CLI Master Tests"]);
        fs::write(repository.join("tracked.txt"), "initial\n")
            .expect("tracked fixture should be written");
        fs::write(repository.join("delete-me.txt"), "delete me\n")
            .expect("deletion fixture should be written");
        command(&repository, ["add", "."]);
        command(&repository, ["commit", "-m", "initial"]);
        Self {
            _temp: temp,
            repository,
            managed,
        }
    }
}

fn command<I, S>(cwd: &Path, args: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("Git fixture command should start");
    assert!(
        output.status.success(),
        "Git fixture command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

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
    assert!(first.starts_with("agent/"));
    command(&fixture.repository, ["branch", &first]);

    let second = git
        .generate_branch_name(&fixture.repository, "Olá / Auth API", "abc123")
        .expect("colliding branch should be generated");
    assert_eq!(second, format!("{first}-2"));
    assert_eq!(slugify("Olá / Auth API"), "ol-auth-api");
}

#[test]
fn create_and_list_worktree_then_clean_remove_preserves_branch() {
    let fixture = RepositoryFixture::new();
    let git = Git::discover().expect("Git should be discovered");
    let created = git
        .create_worktree(
            &fixture.repository,
            &fixture.managed,
            "Implement Auth",
            "01234567",
        )
        .expect("worktree should be created");
    let branch = created
        .branch
        .clone()
        .expect("worktree should have a branch");
    assert!(
        created.path.starts_with(
            fixture
                .managed
                .canonicalize()
                .expect("managed root should canonicalize")
        )
    );
    assert!(
        git.list_worktrees(&fixture.repository)
            .expect("worktrees should list")
            .iter()
            .any(|worktree| worktree.path == created.path)
    );

    git.remove_worktree(
        &fixture.repository,
        &fixture.managed,
        &created.path,
        WorktreeUse::default(),
    )
    .expect("clean worktree should be removed");

    assert!(!created.path.exists());
    let output = Command::new("git")
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .current_dir(&fixture.repository)
        .output()
        .expect("show-ref should start");
    assert!(output.status.success(), "worktree branch must be preserved");
}

#[test]
fn dirty_tracked_and_untracked_worktree_is_never_removed() {
    let fixture = RepositoryFixture::new();
    let git = Git::discover().expect("Git should be discovered");
    let created = git
        .create_worktree(
            &fixture.repository,
            &fixture.managed,
            "Dirty Worktree",
            "deadbeef",
        )
        .expect("worktree should be created");
    fs::write(created.path.join("tracked.txt"), "dirty\n")
        .expect("tracked file should be modified");
    fs::write(created.path.join("untracked.txt"), "dirty\n")
        .expect("untracked file should be written");

    let preparation = git
        .prepare_remove(
            &fixture.repository,
            &fixture.managed,
            &created.path,
            WorktreeUse::default(),
        )
        .expect("removal should be inspected");
    assert!(!preparation.can_remove);
    assert!(preparation.status.has_tracked_changes);
    assert!(preparation.status.has_untracked);

    let error = git
        .remove_worktree(
            &fixture.repository,
            &fixture.managed,
            &created.path,
            WorktreeUse::default(),
        )
        .expect_err("dirty worktree removal must be refused");
    assert_eq!(error.kind(), GitErrorKind::DirtyWorktree);
    assert!(created.path.exists());
}

#[test]
fn ignored_only_worktree_is_preserved() {
    let fixture = RepositoryFixture::new();
    let git = Git::discover().expect("Git should be discovered");
    let created = git
        .create_worktree(
            &fixture.repository,
            &fixture.managed,
            "Ignored Worktree",
            "19a0be5",
        )
        .expect("worktree should be created");
    fs::write(
        fixture.repository.join(".git/info/exclude"),
        "ignored-output.log\n",
    )
    .expect("repository exclude file should be written");
    fs::write(created.path.join("ignored-output.log"), "valuable output\n")
        .expect("ignored file should be written");

    let preparation = git
        .prepare_remove(
            &fixture.repository,
            &fixture.managed,
            &created.path,
            WorktreeUse::default(),
        )
        .expect("removal should be inspected");
    assert!(!preparation.can_remove);
    assert_eq!(
        preparation.ignored_paths,
        [std::path::PathBuf::from("ignored-output.log")]
    );
    assert!(preparation.blockers.contains(&RemovalBlocker::IgnoredFiles));

    let error = git
        .remove_worktree(
            &fixture.repository,
            &fixture.managed,
            &created.path,
            WorktreeUse::default(),
        )
        .expect_err("ignored-only worktree must not be removed");
    assert_eq!(error.kind(), GitErrorKind::DirtyWorktree);
    assert!(created.path.join("ignored-output.log").exists());
}

#[test]
fn assume_unchanged_modified_worktree_is_preserved() {
    let fixture = RepositoryFixture::new();
    let git = Git::discover().expect("Git should be discovered");
    let created = git
        .create_worktree(
            &fixture.repository,
            &fixture.managed,
            "Assume Unchanged",
            "a55a0e0",
        )
        .expect("worktree should be created");
    command(
        &created.path,
        ["update-index", "--assume-unchanged", "tracked.txt"],
    );
    fs::write(created.path.join("tracked.txt"), "hidden modification\n")
        .expect("assume-unchanged file should be modified");

    let preparation = git
        .prepare_remove(
            &fixture.repository,
            &fixture.managed,
            &created.path,
            WorktreeUse::default(),
        )
        .expect("removal should be inspected");
    assert!(!preparation.can_remove);
    assert_eq!(
        preparation.assume_unchanged_paths,
        [std::path::PathBuf::from("tracked.txt")]
    );
    assert!(
        preparation
            .blockers
            .contains(&RemovalBlocker::AssumeUnchanged)
    );
    let error = git
        .remove_worktree(
            &fixture.repository,
            &fixture.managed,
            &created.path,
            WorktreeUse::default(),
        )
        .expect_err("assume-unchanged worktree must not be removed");
    assert_eq!(error.kind(), GitErrorKind::DirtyWorktree);
    assert!(created.path.exists());
}

#[test]
fn skip_worktree_modified_worktree_is_preserved() {
    let fixture = RepositoryFixture::new();
    let git = Git::discover().expect("Git should be discovered");
    let created = git
        .create_worktree(
            &fixture.repository,
            &fixture.managed,
            "Skip Worktree",
            "5c1f1e5",
        )
        .expect("worktree should be created");
    command(
        &created.path,
        ["update-index", "--skip-worktree", "tracked.txt"],
    );
    fs::write(created.path.join("tracked.txt"), "hidden modification\n")
        .expect("skip-worktree file should be modified");

    let preparation = git
        .prepare_remove(
            &fixture.repository,
            &fixture.managed,
            &created.path,
            WorktreeUse::default(),
        )
        .expect("removal should be inspected");
    assert!(!preparation.can_remove);
    assert_eq!(
        preparation.skip_worktree_paths,
        [std::path::PathBuf::from("tracked.txt")]
    );
    assert!(preparation.blockers.contains(&RemovalBlocker::SkipWorktree));
    let error = git
        .remove_worktree(
            &fixture.repository,
            &fixture.managed,
            &created.path,
            WorktreeUse::default(),
        )
        .expect_err("skip-worktree worktree must not be removed");
    assert_eq!(error.kind(), GitErrorKind::DirtyWorktree);
    assert!(created.path.exists());
}

#[test]
fn running_worktree_is_never_removed() {
    let fixture = RepositoryFixture::new();
    let git = Git::discover().expect("Git should be discovered");
    let created = git
        .create_worktree(
            &fixture.repository,
            &fixture.managed,
            "Running Worktree",
            "feedface",
        )
        .expect("worktree should be created");
    let usage = WorktreeUse {
        running: true,
        in_use: true,
    };
    let preparation = git
        .prepare_remove(&fixture.repository, &fixture.managed, &created.path, usage)
        .expect("removal should be inspected");
    assert!(!preparation.can_remove);
    let error = git
        .remove_worktree(&fixture.repository, &fixture.managed, &created.path, usage)
        .expect_err("running worktree removal must be refused");
    assert_eq!(error.kind(), GitErrorKind::WorktreeInUse);
}

#[test]
fn locked_worktree_is_never_removable() {
    let fixture = RepositoryFixture::new();
    let git = Git::discover().expect("Git should be discovered");
    let created = git
        .create_worktree(
            &fixture.repository,
            &fixture.managed,
            "Locked Worktree",
            "10c4ed0",
        )
        .expect("worktree should be created");
    command(
        &fixture.repository,
        [
            OsStr::new("worktree"),
            OsStr::new("lock"),
            created.path.as_os_str(),
        ],
    );

    let preparation = git
        .prepare_remove(
            &fixture.repository,
            &fixture.managed,
            &created.path,
            WorktreeUse::default(),
        )
        .expect("removal should be inspected");
    assert!(!preparation.can_remove);
    assert!(preparation.blockers.contains(&RemovalBlocker::Locked));
    let error = git
        .remove_worktree(
            &fixture.repository,
            &fixture.managed,
            &created.path,
            WorktreeUse::default(),
        )
        .expect_err("locked worktree must not be removed");
    assert_eq!(error.kind(), GitErrorKind::WorktreeInUse);
    assert!(created.path.exists());
}

#[test]
fn staged_only_untracked_only_and_in_use_are_independent_blockers() {
    let fixture = RepositoryFixture::new();
    let git = Git::discover().expect("Git should be discovered");
    let staged = git
        .create_worktree(
            &fixture.repository,
            &fixture.managed,
            "Staged Only",
            "57a6ed0",
        )
        .expect("staged worktree should be created");
    fs::write(staged.path.join("staged.txt"), "staged\n").expect("staged file should be written");
    command(&staged.path, ["add", "staged.txt"]);
    let staged_preparation = git
        .prepare_remove(
            &fixture.repository,
            &fixture.managed,
            &staged.path,
            WorktreeUse::default(),
        )
        .expect("staged removal should be inspected");
    assert_eq!(staged_preparation.blockers, [RemovalBlocker::StagedChanges]);

    let untracked = git
        .create_worktree(
            &fixture.repository,
            &fixture.managed,
            "Untracked Only",
            "a17ac0d",
        )
        .expect("untracked worktree should be created");
    fs::write(untracked.path.join("untracked.txt"), "untracked\n")
        .expect("untracked file should be written");
    let untracked_preparation = git
        .prepare_remove(
            &fixture.repository,
            &fixture.managed,
            &untracked.path,
            WorktreeUse::default(),
        )
        .expect("untracked removal should be inspected");
    assert_eq!(
        untracked_preparation.blockers,
        [RemovalBlocker::UntrackedFiles]
    );

    let in_use = git
        .create_worktree(
            &fixture.repository,
            &fixture.managed,
            "In Use Only",
            "10a5e00",
        )
        .expect("in-use worktree should be created");
    let in_use_preparation = git
        .prepare_remove(
            &fixture.repository,
            &fixture.managed,
            &in_use.path,
            WorktreeUse {
                running: false,
                in_use: true,
            },
        )
        .expect("in-use removal should be inspected");
    assert_eq!(in_use_preparation.blockers, [RemovalBlocker::InUse]);
}

#[test]
fn removal_rejects_registered_worktree_outside_managed_root() {
    let fixture = RepositoryFixture::new();
    let git = Git::discover().expect("Git should be discovered");
    let other_root = fixture
        .managed
        .parent()
        .expect("managed root should have a parent")
        .join("other-managed-root");
    let created = git
        .create_worktree(
            &fixture.repository,
            &other_root,
            "Outside Worktree",
            "cab005e",
        )
        .expect("outside worktree should be created");
    fs::create_dir_all(&fixture.managed).expect("managed root should be created");

    let error = git
        .prepare_remove(
            &fixture.repository,
            &fixture.managed,
            &created.path,
            WorktreeUse::default(),
        )
        .expect_err("outside worktree must be rejected");

    assert_eq!(error.kind(), GitErrorKind::UnsafePath);
    assert!(created.path.exists());
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
    assert!(
        !diff.text.contains("\u{1b}["),
        "diff must not contain ANSI color"
    );
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
    let elapsed = started.elapsed();

    assert_eq!(error.kind(), GitErrorKind::Timeout);
    assert!(
        elapsed < Duration::from_secs(2),
        "timeout exceeded deadline budget: {elapsed:?}"
    );
    let leader_pid = read_pid(&process_group_leader_pid, "leader");
    let descendant_pid = read_pid(&child_pid, "descendant");
    assert_process_stopped(&leader_pid, "Git process-group leader");
    assert_process_stopped(&descendant_pid, "Git descendant");
}

#[cfg(unix)]
fn read_pid(path: &Path, process: &str) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{process} should report its pid: {error}"))
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
