//! Shared readiness helpers for Beta acceptance tests.
//!
//! These helpers wait on observable session status and PTY bytes. They do not
//! insert fixed sleeps, and they do not add test hooks to production crates.

#![cfg_attr(not(unix), allow(unused))]
#![warn(missing_docs)]

#[cfg(not(unix))]
compile_error!("cli-master-e2e supports Linux and macOS only");

use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::Duration;

use cli_master_core::{CommandSpec, SessionId, SessionStatus};
use cli_master_fake_agent::compiled_executable;
use cli_master_session::{SessionEvent, SessionManager, SessionSnapshot, SessionSubscription};
use rustix::process::{Pid, test_kill_process};
use tempfile::TempDir;

/// Default deadline used by readiness probes.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(12);

/// Isolated Git repository plus a managed worktree root.
pub struct RepositoryFixture {
    _temp: TempDir,
    /// User-selected project directory, which is also the Git root.
    pub repository: PathBuf,
    /// Directory that must contain every managed worktree.
    pub managed: PathBuf,
    /// `SQLite` database used by the production session saga.
    pub database: PathBuf,
}

impl RepositoryFixture {
    /// Initializes a real Git repository with one commit.
    ///
    /// # Panics
    ///
    /// Panics if a temporary directory or Git fixture command fails.
    #[must_use]
    pub fn new() -> Self {
        let temp = TempDir::new().expect("temporary directory should be created");
        let repository = temp.path().join("repository");
        let managed = temp.path().join("managed");
        let database = temp.path().join("cli-master.db");
        std::fs::create_dir(&repository).expect("repository directory should be created");
        git(&repository, ["init", "-b", "main"]);
        git(
            &repository,
            ["config", "user.email", "tests@example.invalid"],
        );
        git(&repository, ["config", "user.name", "CLI Master Tests"]);
        std::fs::write(repository.join("README.md"), "beta e2e\n")
            .expect("tracked file should be written");
        git(&repository, ["add", "."]);
        git(&repository, ["commit", "-m", "initial"]);
        Self {
            _temp: temp,
            repository,
            managed,
            database,
        }
    }
}

impl Default for RepositoryFixture {
    fn default() -> Self {
        Self::new()
    }
}

/// Runs a Git command with a structured argv and no shell.
///
/// # Panics
///
/// Panics if Git cannot be started or exits unsuccessfully.
pub fn git<I, S>(cwd: &Path, args: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
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

/// Builds a `CommandSpec` for the compiled fake agent.
///
/// # Panics
///
/// Panics if the fake-agent binary cannot be found or the command is invalid.
#[must_use]
pub fn fake_agent_command(cwd: &Path, extra_args: &[&str]) -> CommandSpec {
    let mut env = std::collections::BTreeMap::new();
    env.insert(
        "FAKE_AGENT_SECRET".to_owned(),
        "must-not-appear-in-output".to_owned(),
    );
    CommandSpec::try_from_parts(
        compiled_executable()
            .expect("fake-agent binary should be built before runtime acceptance")
            .to_string_lossy()
            .into_owned(),
        extra_args.iter().map(ToString::to_string),
        cwd.to_path_buf(),
        env,
    )
    .expect("fake-agent command should be valid")
}

/// Waits until the session is in a live status.
///
/// # Panics
///
/// Panics if the session fails, exits, or the probe deadline elapses.
pub async fn wait_live(manager: &SessionManager, id: SessionId) -> SessionSnapshot {
    wait_until_session(manager, id, |session| session.status.is_live()).await
}

/// Waits until the session reaches `expected`.
///
/// # Panics
///
/// Panics if the session fails unexpectedly or the probe deadline elapses.
pub async fn wait_status(
    manager: &SessionManager,
    id: SessionId,
    expected: SessionStatus,
) -> SessionSnapshot {
    wait_until_session(manager, id, |session| session.status == expected).await
}

async fn wait_until_session(
    manager: &SessionManager,
    id: SessionId,
    predicate: impl Fn(&SessionSnapshot) -> bool,
) -> SessionSnapshot {
    let deadline = tokio::time::Instant::now() + PROBE_TIMEOUT;
    loop {
        if let Ok(session) = manager.snapshot(id) {
            if predicate(&session) {
                return session;
            }
            assert!(
                !matches!(
                    session.status,
                    SessionStatus::Failed | SessionStatus::Exited
                ) || predicate(&session),
                "session entered an unexpected terminal state: {session:?}"
            );
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for session {id}, last={:?}",
            manager.snapshot(id)
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// Collects PTY bytes until `needle` appears in the concatenated output.
///
/// # Panics
///
/// Panics if the subscription ends or the probe deadline elapses first.
pub async fn wait_for_bytes(
    subscription: &mut SessionSubscription,
    collected: &mut Vec<u8>,
    needle: &[u8],
) {
    if collected.is_empty() {
        collected.extend(
            subscription
                .snapshot
                .output
                .iter()
                .flat_map(|chunk| chunk.bytes.iter().copied()),
        );
    }
    if contains(collected, needle) {
        return;
    }
    let deadline = tokio::time::Instant::now() + PROBE_TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        assert!(
            !remaining.is_zero(),
            "timed out waiting for {}, got {}",
            String::from_utf8_lossy(needle),
            String::from_utf8_lossy(collected)
        );
        match tokio::time::timeout(remaining, subscription.receiver.recv()).await {
            Ok(Ok(SessionEvent::Output(chunk))) => {
                collected.extend_from_slice(&chunk.bytes);
                if contains(collected, needle) {
                    return;
                }
            }
            Ok(Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => {}
            Ok(Err(error)) => panic!("subscription ended while waiting for output: {error}"),
            Err(elapsed) => panic!(
                "timed out waiting for {}, got {} after {elapsed:?}",
                String::from_utf8_lossy(needle),
                String::from_utf8_lossy(collected)
            ),
        }
    }
}

/// Returns whether `needle` is present in `haystack`.
#[must_use]
pub fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// Unix epoch milliseconds for persistence timestamps.
#[must_use]
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or(0)
}

/// Returns whether `pid` still exists without signaling it.
#[must_use]
pub fn process_is_alive(pid: u32) -> bool {
    i32::try_from(pid)
        .ok()
        .and_then(Pid::from_raw)
        .is_some_and(|pid| test_kill_process(pid).is_ok())
}

/// Locates a Unix executable used by leftover-process fixtures in a fixed
/// system directory, without consulting `PATH`.
///
/// # Panics
///
/// Panics if `name` is not present in the usual Unix binary directories.
#[must_use]
pub fn system_executable(name: &str) -> PathBuf {
    ["/usr/bin", "/bin", "/usr/sbin", "/sbin"]
        .into_iter()
        .map(|directory| Path::new(directory).join(name))
        .find(|path| path.is_file())
        .unwrap_or_else(|| panic!("{name} not found"))
}

/// Child process that is always terminated and reaped during unwinding.
pub struct ChildGuard {
    child: Option<Child>,
}

impl ChildGuard {
    /// Spawns a fixture process from an absolute path with closed standard I/O.
    ///
    /// # Panics
    ///
    /// Panics if `executable` is relative or the process cannot be started.
    #[must_use]
    pub fn spawn(executable: &Path, args: &[&str]) -> Self {
        assert!(
            executable.is_absolute(),
            "fixture executable path must be absolute"
        );
        let child = Command::new(executable)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("fixture process should start");
        Self { child: Some(child) }
    }

    /// Returns the operating-system process identifier.
    ///
    /// # Panics
    ///
    /// Panics if the guarded child has already been terminated.
    #[must_use]
    pub fn id(&self) -> u32 {
        self.child.as_ref().expect("child should be present").id()
    }

    /// Terminates and reaps the guarded child.
    ///
    /// # Panics
    ///
    /// Panics if the status cannot be inspected or the child cannot be reaped.
    pub fn terminate(&mut self) -> ExitStatus {
        let mut child = self.child.take().expect("child should be present");
        if child
            .try_wait()
            .expect("child status should be readable")
            .is_none()
        {
            child.kill().expect("fixture process should terminate");
        }
        child.wait().expect("fixture process should be reaped")
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        if child.try_wait().ok().flatten().is_none() {
            let _ = child.kill();
        }
        let _ = child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_finds_fragmented_needles() {
        assert!(contains(b"ack:one", b"ack:one"));
        assert!(!contains(b"ack:one", b"ack:two"));
    }
}
