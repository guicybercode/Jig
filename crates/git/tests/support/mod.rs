// Cargo compiles each integration-test file as a separate crate, so shared
// helpers are intentionally unused in some of those crates.
#![allow(dead_code)]

use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use cli_master_git::{Git, RemovalPreparation, WorktreeUse};
use tempfile::TempDir;

pub struct RepositoryFixture {
    pub temp: TempDir,
    pub repository: PathBuf,
    pub managed: PathBuf,
}

impl RepositoryFixture {
    pub fn new() -> Self {
        let temp = TempDir::new().expect("temporary directory should be created");
        let repository = temp.path().join("repository");
        let managed = temp.path().join("managed");
        fs::create_dir(&repository).expect("repository directory should be created");
        command(&repository, ["init", "-b", "main"]);
        configure_identity(&repository);
        fs::write(repository.join("tracked.txt"), "initial\n")
            .expect("tracked fixture should be written");
        fs::write(repository.join("delete-me.txt"), "delete me\n")
            .expect("deletion fixture should be written");
        command(&repository, ["add", "."]);
        command(&repository, ["commit", "-m", "initial"]);
        Self {
            temp,
            repository,
            managed,
        }
    }
}

pub fn configure_identity(repository: &Path) {
    command(
        repository,
        ["config", "user.email", "tests@example.invalid"],
    );
    command(repository, ["config", "user.name", "CLI Master Tests"]);
}

pub fn command<I, S>(cwd: &Path, args: I)
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

pub fn command_line<I, S>(cwd: &Path, args: I) -> String
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
    assert!(output.status.success());
    String::from_utf8(output.stdout)
        .expect("Git fixture output should be UTF-8")
        .trim()
        .to_owned()
}

pub fn branch_exists(repository: &Path, branch: &str) -> bool {
    Command::new("git")
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .current_dir(repository)
        .env("GIT_TERMINAL_PROMPT", "0")
        .status()
        .expect("branch lookup should start")
        .success()
}

pub fn removal_snapshot(
    git: &Git,
    fixture: &RepositoryFixture,
    worktree: &Path,
) -> RemovalPreparation {
    git.prepare_remove(
        &fixture.repository,
        &fixture.managed,
        worktree,
        WorktreeUse::default(),
    )
    .expect("removal should be inspected")
}

#[cfg(unix)]
pub fn install_post_checkout_hook(fixture: &RepositoryFixture, body: &str) {
    let hook = fixture.repository.join(".git/hooks/post-checkout");
    write_executable(&hook, &format!("#!/bin/sh\n{body}\n"));
}

#[cfg(unix)]
pub fn write_executable(path: &Path, content: &str) {
    use std::os::unix::fs::PermissionsExt;

    fs::write(path, content).expect("executable fixture should be written");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .expect("fixture should be executable");
}

#[cfg(unix)]
pub fn shell_quote(path: &Path) -> String {
    let escaped = path.to_string_lossy().replace('\'', "'\"'\"'");
    format!("'{escaped}'")
}
