use std::env;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use cli_master_core::{
    ApplicationError, COMMAND_OUTPUT_MAX_BYTES, CommandSpec, ErrorCode, GIT_COMMAND_TIMEOUT,
};

/// Structured process invocation. The executable and arguments are never joined
/// into a shell string.
#[derive(Clone, Eq, PartialEq)]
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

impl fmt::Debug for SpawnRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SpawnRequest")
            .field("executable", &self.executable)
            .field("args_count", &self.args.len())
            .field("cwd", &self.cwd)
            .field(
                "env_keys",
                &self
                    .extra_env
                    .iter()
                    .map(|(key, _)| key)
                    .collect::<Vec<_>>(),
            )
            .field("clear_env", &self.clear_env)
            .field("timeout", &self.timeout)
            .field("max_output", &self.max_output)
            .field("allow_login_shell", &self.allow_login_shell)
            .finish()
    }
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
#[derive(Clone, Eq, PartialEq)]
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

impl fmt::Debug for ProcessOutput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProcessOutput")
            .field("exit_code", &self.exit_code)
            .field("stdout_bytes", &self.stdout.len())
            .field("stderr_bytes", &self.stderr.len())
            .field("truncated", &self.truncated)
            .field("timed_out", &self.timed_out)
            .finish()
    }
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
    if executable_has_name(executable, "env")
        && args
            .iter()
            .any(|argument| argument == "-S" || argument == "--split-string")
    {
        return Err(shell_invocation_error(executable));
    }
    if let Some((nested, nested_args)) = wrapped_command(executable, args) {
        return assert_structured_command(Path::new(nested), nested_args);
    }

    let shell = shell_kind(executable);
    let uses_command_string = match shell {
        Some(ShellKind::Posix) => args.iter().any(|argument| {
            let argument = argument.to_ascii_lowercase();
            argument == "--command"
                || argument.starts_with("--command=")
                || argument == "--init-command"
                || argument.starts_with("--init-command=")
                || argument
                    .strip_prefix('-')
                    .is_some_and(|flags| !flags.starts_with('-') && flags.contains('c'))
        }),
        Some(ShellKind::PowerShell) => args.iter().any(|argument| {
            let flag = argument.trim_start_matches(['-', '/']).to_ascii_lowercase();
            !flag.is_empty()
                && ("command".starts_with(&flag)
                    || "commandwithargs".starts_with(&flag)
                    || "encodedcommand".starts_with(&flag))
        }),
        Some(ShellKind::Cmd) => args.iter().any(|argument| {
            let argument = argument.to_ascii_lowercase();
            argument.starts_with("/c") || argument.starts_with("/k")
        }),
        None => false,
    };
    if shell.is_some() && uses_command_string {
        return Err(shell_invocation_error(executable));
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum ShellKind {
    Posix,
    PowerShell,
    Cmd,
}

fn shell_kind(executable: &Path) -> Option<ShellKind> {
    let direct = executable.file_name().and_then(OsStr::to_str);
    let canonical = fs::canonicalize(executable)
        .ok()
        .and_then(|path| path.file_name().map(OsStr::to_owned))
        .and_then(|name| name.into_string().ok());
    direct
        .into_iter()
        .chain(canonical.as_deref())
        .find_map(classify_shell_name)
}

fn classify_shell_name(name: &str) -> Option<ShellKind> {
    let name = name.to_ascii_lowercase();
    let name = name.strip_suffix(".exe").unwrap_or(&name);
    match name {
        "sh" | "bash" | "zsh" | "dash" | "ksh" | "fish" | "csh" | "tcsh" | "nu" | "xonsh" => {
            Some(ShellKind::Posix)
        }
        "powershell" | "pwsh" => Some(ShellKind::PowerShell),
        "cmd" => Some(ShellKind::Cmd),
        _ => None,
    }
}

fn executable_has_name(executable: &Path, expected: &str) -> bool {
    let direct = executable.file_name().and_then(OsStr::to_str);
    let canonical = fs::canonicalize(executable)
        .ok()
        .and_then(|path| path.file_name().map(OsStr::to_owned))
        .and_then(|name| name.into_string().ok());
    direct
        .into_iter()
        .chain(canonical.as_deref())
        .any(|name| name.trim_end_matches(".exe").eq_ignore_ascii_case(expected))
}

fn wrapped_command<'a>(executable: &Path, args: &'a [String]) -> Option<(&'a str, &'a [String])> {
    if executable_has_name(executable, "busybox") {
        let (nested, rest) = args.split_first()?;
        return classify_shell_name(nested).map(|_| (nested.as_str(), rest));
    }
    if !executable_has_name(executable, "env") {
        return None;
    }
    for (index, argument) in args.iter().enumerate() {
        if classify_shell_name(argument).is_some() {
            return Some((argument, &args[index + 1..]));
        }
    }
    None
}

fn shell_invocation_error(executable: &Path) -> ApplicationError {
    ApplicationError::new(
        ErrorCode::ShellInvocationRefused,
        "Refusing to start a shell with a command string.",
    )
    .not_recoverable()
    .with_action("Pass an executable and an argument array instead of `sh -c`.")
    .with_context("executable", executable.display().to_string())
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
    let executable = resolved_spawn_executable(request);
    if request.allow_login_shell {
        // Documented exception: import_login_path uses a constant command string.
    } else {
        assert_structured_command(&executable, &request.args)?;
    }
    if request.executable.as_os_str().is_empty() {
        return Err(
            ApplicationError::new(ErrorCode::InvalidPath, "Executable must not be empty.")
                .with_action("Configure an absolute executable path or a PATH-resolved name."),
        );
    }

    let mut command = Command::new(&executable);
    command.args(&request.args);
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.process_group(0);
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

    let stdout = child.stdout.take().ok_or_else(|| {
        ApplicationError::new(
            ErrorCode::PtySpawnFailed,
            "Process stdout was not captured.",
        )
        .with_action("Retry the operation.")
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        ApplicationError::new(
            ErrorCode::PtySpawnFailed,
            "Process stderr was not captured.",
        )
        .with_action("Retry the operation.")
    })?;

    let max_output = request.max_output;
    let stdout_receiver = read_capped_async(stdout, max_output);
    let stderr_receiver = read_capped_async(stderr, max_output);

    let wait_outcome = wait_with_timeout(&mut child, request.timeout).inspect_err(|_| {
        terminate_process_group(&mut child);
    })?;
    let (timed_out, exit_code) = match wait_outcome {
        WaitOutcome::Exited(status) => (false, status.code()),
        WaitOutcome::TimedOut => {
            terminate_process_group(&mut child);
            let _ = child.wait();
            (true, None)
        }
    };

    let (stdout, stdout_truncated) = receive_capped_output(&stdout_receiver, &mut child);
    let (stderr, stderr_truncated) = receive_capped_output(&stderr_receiver, &mut child);

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

fn resolved_spawn_executable(request: &SpawnRequest) -> PathBuf {
    if request.executable.is_absolute() || request.executable.components().count() != 1 {
        return fs::canonicalize(&request.executable)
            .unwrap_or_else(|_| request.executable.clone());
    }
    let path_override = request
        .extra_env
        .iter()
        .rev()
        .find(|(key, _)| key == "PATH")
        .map(|(_, value)| OsStr::new(value).to_os_string());
    let search_path = path_override.or_else(|| {
        if request.clear_env {
            None
        } else {
            env::var_os("PATH")
        }
    });
    if let Some(search_path) = search_path {
        for directory in env::split_paths(&search_path) {
            if let Ok(candidate) = fs::canonicalize(directory.join(&request.executable)) {
                return candidate;
            }
        }
    }
    request.executable.clone()
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

fn read_capped_async<R>(mut reader: R, max_output: usize) -> Receiver<(Vec<u8>, bool)>
where
    R: Read + Send + 'static,
{
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let _ = sender.send(read_capped(&mut reader, max_output));
    });
    receiver
}

fn receive_capped_output(
    receiver: &Receiver<(Vec<u8>, bool)>,
    child: &mut std::process::Child,
) -> (Vec<u8>, bool) {
    match receiver.recv_timeout(Duration::from_millis(250)) {
        Ok(output) => output,
        Err(RecvTimeoutError::Timeout) => {
            terminate_process_group(child);
            receiver
                .recv_timeout(Duration::from_secs(1))
                .unwrap_or_else(|_| (Vec::new(), true))
        }
        Err(RecvTimeoutError::Disconnected) => (Vec::new(), true),
    }
}

fn terminate_process_group(child: &mut std::process::Child) {
    let process_group = format!("-{}", child.id());
    let _ = Command::new("/bin/kill")
        .args(["-KILL", "--"])
        .arg(process_group)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = child.kill();
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use tempfile::TempDir;

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
    fn refuses_combined_shell_flags_wrappers_and_shell_symlinks() {
        let combined = assert_structured_command(
            Path::new("/bin/bash"),
            &["-xc".to_owned(), "echo injected".to_owned()],
        )
        .expect_err("combined -c must be refused");
        assert_eq!(combined.code(), ErrorCode::ShellInvocationRefused);

        let wrapped = assert_structured_command(
            Path::new("/usr/bin/env"),
            &[
                "bash".to_owned(),
                "-c".to_owned(),
                "echo injected".to_owned(),
            ],
        )
        .expect_err("env wrapper must be refused");
        assert_eq!(wrapped.code(), ErrorCode::ShellInvocationRefused);

        let wrapped_with_options = assert_structured_command(
            Path::new("/usr/bin/env"),
            &[
                "-u".to_owned(),
                "TOKEN".to_owned(),
                "bash".to_owned(),
                "-c".to_owned(),
                "echo injected".to_owned(),
            ],
        )
        .expect_err("env option wrapper must be refused");
        assert_eq!(
            wrapped_with_options.code(),
            ErrorCode::ShellInvocationRefused
        );

        let powershell_abbreviation = assert_structured_command(
            Path::new("pwsh"),
            &["-com".to_owned(), "Write-Host injected".to_owned()],
        )
        .expect_err("PowerShell command abbreviations must be refused");
        assert_eq!(
            powershell_abbreviation.code(),
            ErrorCode::ShellInvocationRefused
        );

        let temp = TempDir::new().expect("temp");
        let alias = temp.path().join("helper");
        symlink("/bin/sh", &alias).expect("shell alias");
        let aliased =
            assert_structured_command(&alias, &["-c".to_owned(), "echo injected".to_owned()])
                .expect_err("shell symlink must be refused");
        assert_eq!(aliased.code(), ErrorCode::ShellInvocationRefused);

        let bare_alias = run_command_unchecked(
            &SpawnRequest::new("helper")
                .args(["-c", "echo injected"])
                .env("PATH", temp.path().display().to_string()),
        )
        .expect_err("PATH-resolved shell alias must be refused");
        assert_eq!(bare_alias.code(), ErrorCode::ShellInvocationRefused);
    }

    #[test]
    fn debug_output_never_contains_arguments_environment_values_or_process_output() {
        let request = SpawnRequest::new("agent")
            .arg("--token=argument-secret")
            .env("TOKEN", "environment-secret");
        let request_debug = format!("{request:?}");
        assert!(!request_debug.contains("argument-secret"));
        assert!(!request_debug.contains("environment-secret"));

        let output = ProcessOutput {
            exit_code: Some(0),
            stdout: b"stdout-secret".to_vec(),
            stderr: b"stderr-secret".to_vec(),
            truncated: false,
            timed_out: false,
        };
        let output_debug = format!("{output:?}");
        assert!(!output_debug.contains("stdout-secret"));
        assert!(!output_debug.contains("stderr-secret"));
    }

    #[test]
    fn descendant_holding_output_open_cannot_bypass_the_timeout() {
        let started = Instant::now();
        let output = run_command_unchecked(
            &SpawnRequest::new(std::env::current_exe().expect("test executable"))
                .args([
                    "--exact",
                    "process::tests::spawn_output_holder_helper",
                    "--ignored",
                    "--nocapture",
                ])
                .env("CLI_MASTER_TEST_PIPE_HOLDER", "1")
                .timeout(Duration::from_secs(2)),
        )
        .expect("direct child exits successfully");
        assert!(output.success());
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    #[ignore = "subprocess helper for descendant pipe test"]
    #[allow(clippy::zombie_processes)]
    fn spawn_output_holder_helper() {
        if std::env::var_os("CLI_MASTER_TEST_PIPE_HOLDER").is_none() {
            return;
        }
        // Intentionally leave the child alive: the parent test verifies that the
        // process runner kills the isolated group and does not block on its pipe.
        let _child = std::process::Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .expect("spawn output holder");
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
