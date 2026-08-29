use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use cli_master_core::{
    ApplicationError, COMMAND_OUTPUT_MAX_BYTES, CommandSpec, ErrorCode, GIT_COMMAND_TIMEOUT,
};

/// Structured process invocation. The executable and arguments are never joined
/// into a shell string.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpawnRequest {
    executable: PathBuf,
    args: Vec<String>,
    cwd: Option<PathBuf>,
    extra_env: Vec<(String, String)>,
    clear_env: bool,
    timeout: Duration,
    max_output: usize,
    allow_login_shell: bool,
}

impl SpawnRequest {
    /// Creates a request for `executable` with no arguments.
    #[must_use]
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            args: Vec::new(),
            cwd: None,
            extra_env: Vec::new(),
            clear_env: false,
            timeout: GIT_COMMAND_TIMEOUT,
            max_output: COMMAND_OUTPUT_MAX_BYTES,
            allow_login_shell: false,
        }
    }

    /// Appends a single argument. Callers must not concatenate user input.
    #[must_use]
    pub fn arg(mut self, argument: impl Into<String>) -> Self {
        self.args.push(argument.into());
        self
    }

    /// Appends several arguments.
    #[must_use]
    pub fn args<I, S>(mut self, arguments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(arguments.into_iter().map(Into::into));
        self
    }

    /// Sets the child working directory.
    #[must_use]
    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Adds one environment override. Sensitive names are still passed to the
    /// child but are never logged.
    #[must_use]
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_env.push((key.into(), value.into()));
        self
    }

    /// Drops the inherited environment except for overrides added with [`Self::env`].
    #[must_use]
    pub const fn isolated(mut self) -> Self {
        self.clear_env = true;
        self
    }

    /// Overrides the wait timeout.
    #[must_use]
    pub const fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Overrides the captured output cap.
    #[must_use]
    pub const fn max_output(mut self, max_output: usize) -> Self {
        self.max_output = max_output;
        self
    }

    /// Returns the executable path.
    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// Returns the argument array.
    #[must_use]
    pub fn args_slice(&self) -> &[String] {
        &self.args
    }

    /// Marks this request as the documented login-shell PATH import exception.
    #[must_use]
    pub(crate) const fn allow_login_shell(mut self) -> Self {
        self.allow_login_shell = true;
        self
    }
}

/// Captured result of a bounded subprocess.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessOutput {
    /// Process exit code, if it exited before the timeout.
    pub exit_code: Option<i32>,
    /// Captured standard output, truncated when necessary.
    pub stdout: Vec<u8>,
    /// Captured standard error, truncated when necessary.
    pub stderr: Vec<u8>,
    /// Whether stdout or stderr hit the byte cap.
    pub truncated: bool,
    /// Whether the process was killed because it exceeded the timeout.
    pub timed_out: bool,
}

impl ProcessOutput {
    /// Interprets stdout as lossy UTF-8.
    #[must_use]
    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).trim().to_owned()
    }

    /// Interprets stderr as lossy UTF-8.
    #[must_use]
    pub fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).trim().to_owned()
    }

    /// Returns whether the process exited with status zero.
    #[must_use]
    pub fn success(&self) -> bool {
        self.exit_code == Some(0) && !self.timed_out
    }
}

/// Refuses `sh -c`, `bash -c`, `zsh -c`, and similar shell invocations.
///
/// # Errors
///
/// Returns [`ErrorCode::ShellInvocationRefused`] when the command would be
/// interpreted by a shell.
pub fn assert_structured_command(
    executable: &Path,
    args: &[String],
) -> Result<(), ApplicationError> {
    let file_name = executable
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default();
    let is_shell = matches!(
        file_name,
        "sh" | "bash" | "zsh" | "dash" | "ksh" | "fish" | "csh" | "tcsh"
    );
    let uses_command_string = args
        .iter()
        .any(|argument| argument == "-c" || argument == "-lc");
    if is_shell && uses_command_string {
        return Err(ApplicationError::new(
            ErrorCode::ShellInvocationRefused,
            "Refusing to start a shell with a command string.",
        )
        .not_recoverable()
        .with_action("Pass an executable and an argument array instead of `sh -c`.")
        .with_context("executable", file_name));
    }
    Ok(())
}

/// Runs a [`CommandSpec`] without a shell.
///
/// # Errors
///
/// Returns an application error when the command is a shell invocation, cannot
/// be spawned, times out, or fails.
pub fn run_command_spec(spec: &CommandSpec) -> Result<ProcessOutput, ApplicationError> {
    let mut request = SpawnRequest::new(spec.executable()).args(spec.args().iter().cloned());
    request = request.cwd(spec.cwd().clone());
    for (key, value) in spec.env() {
        request = request.env(key, value);
    }
    run_command(&request)
}

/// Spawns `executable` with `args[]`, a controlled environment, and a timeout.
///
/// # Errors
///
/// Returns an application error when the command is refused, cannot be spawned,
/// times out, or exits unsuccessfully. Callers that need the raw output of a
/// failing command should inspect [`ProcessOutput`] through
/// [`run_command_unchecked`].
pub fn run_command(request: &SpawnRequest) -> Result<ProcessOutput, ApplicationError> {
    let output = run_command_unchecked(request)?;
    if !output.success() {
        return Err(ApplicationError::new(
            ErrorCode::GitCommandFailed,
            "The command exited unsuccessfully.",
        )
        .with_action("Check that Git is installed and the selected path is a repository.")
        .with_context("exitCode", i64::from(output.exit_code.unwrap_or(-1))));
    }
    Ok(output)
}

/// Runs a structured command and returns output even when the status is non-zero.
///
/// # Errors
///
/// Returns an error for shell invocations, spawn failures, and timeouts.
pub fn run_command_unchecked(request: &SpawnRequest) -> Result<ProcessOutput, ApplicationError> {
    if request.allow_login_shell {
        // Documented exception: import_login_path uses a constant command string.
    } else {
        assert_structured_command(&request.executable, &request.args)?;
    }
    if request.executable.as_os_str().is_empty() {
        return Err(
            ApplicationError::new(ErrorCode::InvalidPath, "Executable must not be empty.")
                .with_action("Configure an absolute executable path or a PATH-resolved name."),
        );
    }

    let mut command = Command::new(&request.executable);
    command.args(&request.args);
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    if let Some(cwd) = &request.cwd {
        command.current_dir(cwd);
    }
    if request.clear_env {
        command.env_clear();
        command.env("LC_ALL", "C");
        command.env("GIT_TERMINAL_PROMPT", "0");
        command.env("GIT_OPTIONAL_LOCKS", "0");
        command.env("PAGER", "cat");
        command.env("GIT_PAGER", "cat");
    }
    for (key, value) in &request.extra_env {
        command.env(key, value);
    }

    let mut child = command.spawn().map_err(|error| {
        let code = if error.kind() == std::io::ErrorKind::PermissionDenied {
            ErrorCode::PermissionDenied
        } else {
            ErrorCode::PtySpawnFailed
        };
        ApplicationError::new(
            code,
            format!("Could not start {}.", request.executable.display()),
        )
        .with_action("Confirm that the executable exists and is executable.")
        .with_context("executable", request.executable.display().to_string())
        .with_source(&error)
    })?;

    let mut stdout = child.stdout.take().ok_or_else(|| {
        ApplicationError::new(
            ErrorCode::PtySpawnFailed,
            "Process stdout was not captured.",
        )
        .with_action("Retry the operation.")
    })?;
    let mut stderr = child.stderr.take().ok_or_else(|| {
        ApplicationError::new(
            ErrorCode::PtySpawnFailed,
            "Process stderr was not captured.",
        )
        .with_action("Retry the operation.")
    })?;

    let max_output = request.max_output;
    let stdout_handle = thread::spawn(move || read_capped(&mut stdout, max_output));
    let stderr_handle = thread::spawn(move || read_capped(&mut stderr, max_output));

    let wait_outcome = wait_with_timeout(&mut child, request.timeout)?;
    let (timed_out, exit_code) = match wait_outcome {
        WaitOutcome::Exited(status) => (false, status.code()),
        WaitOutcome::TimedOut => {
            let _ = child.kill();
            let _ = child.wait();
            (true, None)
        }
    };

    let (stdout, stdout_truncated) = stdout_handle.join().unwrap_or_else(|_| (Vec::new(), false));
    let (stderr, stderr_truncated) = stderr_handle.join().unwrap_or_else(|_| (Vec::new(), false));

    let output = ProcessOutput {
        exit_code,
        stdout,
        stderr,
        truncated: stdout_truncated || stderr_truncated,
        timed_out,
    };

    if timed_out {
        return Err(ApplicationError::new(
            ErrorCode::CommandTimeout,
            format!(
                "{} did not finish within {} seconds.",
                request.executable.display(),
                request.timeout.as_secs().max(1)
            ),
        )
        .with_action("Retry the operation. If it keeps hanging, inspect Git processes manually.")
        .with_context("executable", request.executable.display().to_string())
        .with_context(
            "timeoutSecs",
            i64::try_from(request.timeout.as_secs()).unwrap_or(i64::MAX),
        ));
    }

    Ok(output)
}

enum WaitOutcome {
    Exited(ExitStatus),
    TimedOut,
}

fn wait_with_timeout(
    child: &mut std::process::Child,
    timeout: Duration,
) -> Result<WaitOutcome, ApplicationError> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(WaitOutcome::Exited(status)),
            Ok(None) if Instant::now() >= deadline => return Ok(WaitOutcome::TimedOut),
            Ok(None) => thread::sleep(Duration::from_millis(15)),
            Err(error) => {
                return Err(ApplicationError::new(
                    ErrorCode::PtySpawnFailed,
                    "Could not wait for the child process.",
                )
                .with_action("Retry the operation.")
                .with_source(&error));
            }
        }
    }
}

fn read_capped(reader: &mut dyn Read, max_output: usize) -> (Vec<u8>, bool) {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
    let mut truncated = false;
    loop {
        match reader.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                let remaining = max_output.saturating_sub(buffer.len());
                if remaining == 0 {
                    truncated = true;
                    let mut sink = std::io::sink();
                    let _ = std::io::copy(reader, &mut sink);
                    break;
                }
                let take = read.min(remaining);
                buffer.extend_from_slice(&chunk[..take]);
                if take < read {
                    truncated = true;
                    let mut sink = std::io::sink();
                    let _ = std::io::copy(reader, &mut sink);
                    break;
                }
            }
        }
    }
    (buffer, truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_with_metacharacters_are_not_interpreted_by_a_shell() {
        let output = run_command(
            &SpawnRequest::new("/bin/echo")
                .arg("hello; rm -rf /")
                .arg("$(touch /tmp/should-not-exist)")
                .timeout(Duration::from_secs(3)),
        )
        .expect("echo should run");
        let text = output.stdout_text();
        assert!(text.contains("hello; rm -rf /"));
        assert!(text.contains("$(touch /tmp/should-not-exist)"));
    }

    #[test]
    fn refuses_shell_command_strings() {
        let error = assert_structured_command(
            Path::new("/bin/bash"),
            &["-c".to_owned(), "echo injected".to_owned()],
        )
        .expect_err("shell -c should be refused");
        assert_eq!(error.code(), ErrorCode::ShellInvocationRefused);
        assert!(error.suggested_action().is_some());
    }

    #[test]
    fn timeout_kills_a_hanging_process() {
        let error = run_command(
            &SpawnRequest::new("/bin/sleep")
                .arg("30")
                .timeout(Duration::from_millis(200)),
        )
        .expect_err("sleep should time out");
        assert_eq!(error.code(), ErrorCode::CommandTimeout);
        assert!(error.suggested_action().is_some());
    }
}
