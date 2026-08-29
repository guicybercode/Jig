use std::{
    ffi::OsString,
    io::{self, Read},
    path::Path,
    process::{Command, ExitStatus, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

use crate::{GitError, GitErrorKind};

const STDERR_LIMIT: usize = 256 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const TERMINATION_GRACE: Duration = Duration::from_millis(250);

type ReaderResult = io::Result<(Vec<u8>, bool)>;

pub(crate) struct CommandOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_truncated: bool,
}

impl CommandOutput {
    pub fn success(&self) -> bool {
        self.status.success()
    }
}

pub(crate) fn run(
    executable: &Path,
    cwd: Option<&Path>,
    args: Vec<OsString>,
    timeout: Duration,
    max_stdout: usize,
) -> Result<CommandOutput, GitError> {
    let mut command = Command::new(executable);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_PAGER", "cat")
        .env("LC_ALL", "C");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let started = Instant::now();
    let deadline = started + timeout;
    let mut child = command
        .spawn()
        .map_err(|error| GitError::io("start Git", error))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| GitError::io("capture Git stdout", io::Error::other("missing pipe")))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| GitError::io("capture Git stderr", io::Error::other("missing pipe")))?;

    let stdout_reader = spawn_reader(stdout, max_stdout);
    let stderr_reader = spawn_reader(stderr, STDERR_LIMIT);

    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| GitError::io("wait for Git", error))?
        {
            break status;
        }
        if Instant::now() >= deadline {
            terminate_process_group(&mut child);
            return Err(timeout_error(timeout));
        }
        thread::sleep(POLL_INTERVAL);
    };

    let Some((stdout, stdout_truncated)) =
        receive_reader(&stdout_reader, deadline, "read Git stdout")?
    else {
        terminate_process_group(&mut child);
        return Err(timeout_error(timeout));
    };
    let Some((stderr, _)) = receive_reader(&stderr_reader, deadline, "read Git stderr")? else {
        terminate_process_group(&mut child);
        return Err(timeout_error(timeout));
    };
    Ok(CommandOutput {
        status,
        stdout,
        stderr,
        stdout_truncated,
    })
}

fn read_capped(mut reader: impl Read, limit: usize) -> io::Result<(Vec<u8>, bool)> {
    let mut retained = Vec::with_capacity(limit.min(64 * 1024));
    let mut buffer = [0_u8; 16 * 1024];
    let mut truncated = false;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(retained.len());
        let keep = remaining.min(count);
        retained.extend_from_slice(&buffer[..keep]);
        truncated |= keep < count;
    }
    Ok((retained, truncated))
}

fn spawn_reader(reader: impl Read + Send + 'static, limit: usize) -> Receiver<ReaderResult> {
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let _ = sender.send(read_capped(reader, limit));
    });
    receiver
}

fn receive_reader(
    reader: &Receiver<ReaderResult>,
    deadline: Instant,
    operation: &'static str,
) -> Result<Option<(Vec<u8>, bool)>, GitError> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    match reader.recv_timeout(remaining) {
        Ok(result) => result
            .map(Some)
            .map_err(|error| GitError::io(operation, error)),
        Err(RecvTimeoutError::Timeout) => Ok(None),
        Err(RecvTimeoutError::Disconnected) => Err(GitError::io(
            operation,
            io::Error::other("reader thread stopped unexpectedly"),
        )),
    }
}

fn terminate_process_group(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        // The child was placed in a process group whose ID is its PID. Signaling
        // the negative ID terminates Git helpers and descendants that inherited
        // stdout/stderr, preventing pipe-reader hangs. `/bin/kill` is available
        // on both supported platforms and is invoked directly, never by a shell.
        let group = format!("-{}", child.id());
        let _ = Command::new("/bin/kill")
            .args(["-KILL", group.as_str()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let deadline = Instant::now() + TERMINATION_GRACE;
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => return,
            Ok(None) => thread::sleep(POLL_INTERVAL),
        }
    }
}

fn timeout_error(timeout: Duration) -> GitError {
    GitError::new(
        GitErrorKind::Timeout,
        format!(
            "Git command timed out after {} seconds",
            timeout.as_secs_f64()
        ),
        "Retry the operation; if it repeatedly times out, inspect repository locks and filesystem health",
    )
}
