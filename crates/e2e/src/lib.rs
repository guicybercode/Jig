//! Shared readiness helpers for Beta acceptance tests.
//!
//! These helpers wait on observable session status and PTY bytes. They do not
//! insert fixed sleeps, and they do not add test hooks to production crates.

#![cfg_attr(not(unix), allow(unused))]
#![warn(missing_docs)]

#[cfg(not(unix))]
compile_error!("cli-master-e2e supports Linux and macOS only");

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use cli_master_core::{AgentId, CommandSpec, ProjectId, Session, SessionId, SessionStatus};
use cli_master_fake_agent::compiled_executable;
use cli_master_session::{
    SessionError, SessionLaunchRequest, SessionManager, SessionManagerConfig, SessionSubscription,
};
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

/// Session manager plus an isolated working directory.
pub struct SessionFixture {
    _tempdir: TempDir,
    /// Scratch directory used when a test does not attach a worktree cwd.
    pub cwd: PathBuf,
    /// Production session manager.
    pub manager: SessionManager,
}

impl SessionFixture {
    /// Creates a manager with the same test timeouts used by PTY lifecycle tests.
    ///
    /// # Panics
    ///
    /// Panics if a temporary directory cannot be created. Also panics if called
    /// outside a Tokio runtime, because [`SessionManager::new`] requires one.
    #[must_use]
    pub fn new() -> Self {
        let tempdir = TempDir::new().expect("temporary directory");
        let cwd = tempdir.path().to_path_buf();
        let manager = SessionManager::new(SessionManagerConfig::for_tests());
        Self {
            _tempdir: tempdir,
            cwd,
            manager,
        }
    }

    /// Starts the fake agent in `cwd`.
    ///
    /// # Errors
    ///
    /// Returns a session error when spawn fails.
    ///
    /// # Panics
    ///
    /// Panics if the fake-agent binary cannot be located or the command spec
    /// is invalid.
    pub fn start_fake_agent(
        &self,
        project_id: ProjectId,
        agent_id: AgentId,
        name: &str,
        cwd: &Path,
        extra_args: &[&str],
    ) -> Result<Session, SessionError> {
        self.manager.create(SessionLaunchRequest {
            project_id,
            agent_id,
            name: name.to_owned(),
            command: fake_agent_command(cwd, extra_args),
            cols: 80,
            rows: 24,
        })
    }
}

impl Default for SessionFixture {
    fn default() -> Self {
        Self::new()
    }
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
        compiled_executable().to_string_lossy().into_owned(),
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
pub async fn wait_live(manager: &SessionManager, id: SessionId) -> Session {
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
) -> Session {
    wait_until_session(manager, id, |session| session.status == expected).await
}

async fn wait_until_session(
    manager: &SessionManager,
    id: SessionId,
    predicate: impl Fn(&Session) -> bool,
) -> Session {
    let deadline = tokio::time::Instant::now() + PROBE_TIMEOUT;
    loop {
        if let Some(session) = manager.get(id) {
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
            manager.get(id)
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
        collected.extend(subscription.snapshot.concatenated());
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
        match tokio::time::timeout(remaining, subscription.next_chunk()).await {
            Ok(Ok(chunk)) => {
                collected.extend_from_slice(&chunk.data);
                if contains(collected, needle) {
                    return;
                }
            }
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
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Locates a Unix executable used by leftover-process fixtures.
///
/// # Panics
///
/// Panics if `name` is not present in the usual Unix binary directories.
#[must_use]
pub fn which(name: &str) -> PathBuf {
    ["/usr/bin", "/bin", "/usr/sbin", "/sbin"]
        .into_iter()
        .map(|directory| Path::new(directory).join(name))
        .find(|path| path.is_file())
        .unwrap_or_else(|| panic!("{name} not found"))
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
