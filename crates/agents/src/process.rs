use std::{
    io::{self, Read},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

pub(crate) const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(2);
pub(crate) const DEFAULT_PROBE_OUTPUT_LIMIT: usize = 4096;
pub(crate) const PATH_IMPORT_OUTPUT_LIMIT: usize = 64 * 1024;

const POLL_INTERVAL: Duration = Duration::from_millis(10);
const TERMINATION_GRACE: Duration = Duration::from_millis(250);

type ReaderResult = io::Result<Vec<u8>>;

pub(crate) struct LimitedOutput {
    pub timed_out: bool,
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Runs `command` with stdin closed, bounded output, and a hard timeout.
///
/// The child is started directly. Callers must not wrap the program in `sh -c`.
/// Stdout and stderr are drained concurrently to prevent child writes from
/// blocking on full pipes. At most `max_output` bytes from each stream are
/// retained; additional bytes are discarded while the stream is still drained.
pub(crate) fn run_limited(
    mut command: Command,
    timeout: Duration,
    max_output: usize,
) -> io::Result<LimitedOutput> {
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let start = Instant::now();
    let deadline = start + timeout;
    let mut child = command.spawn()?;
    let Some(stdout) = child.stdout.take() else {
        terminate_process_group(&mut child);
        return Err(io::Error::other("child stdout pipe was not available"));
    };
    let Some(stderr) = child.stderr.take() else {
        terminate_process_group(&mut child);
        return Err(io::Error::other("child stderr pipe was not available"));
    };
    let stdout_reader = match spawn_limited_reader("agent-probe-stdout", stdout, max_output) {
        Ok(reader) => reader,
        Err(error) => {
            terminate_process_group(&mut child);
            return Err(error);
        }
    };
    let stderr_reader = match spawn_limited_reader("agent-probe-stderr", stderr, max_output) {
        Ok(reader) => reader,
        Err(error) => {
            terminate_process_group(&mut child);
            return Err(error);
        }
    };

    let wait_result = wait_with_timeout(&mut child, deadline);
    let (timed_out, exit_code) = match wait_result {
        Ok(outcome) => outcome,
        Err(error) => {
            terminate_process_group(&mut child);
            return Err(error);
        }
    };
    if timed_out {
        return Ok(timeout_output());
    }

    let Some(stdout) = receive_limited_reader(&stdout_reader, deadline)? else {
        terminate_process_group(&mut child);
        return Ok(timeout_output());
    };
    let Some(stderr) = receive_limited_reader(&stderr_reader, deadline)? else {
        terminate_process_group(&mut child);
        return Ok(timeout_output());
    };

    Ok(LimitedOutput {
        timed_out: false,
        exit_code,
        stdout,
        stderr,
    })
}

fn wait_with_timeout(child: &mut Child, deadline: Instant) -> io::Result<(bool, Option<i32>)> {
    loop {
        match child.try_wait()? {
            Some(status) => return Ok((false, status.code())),
            None if Instant::now() >= deadline => {
                terminate_process_group(child);
                return Ok((true, None));
            }
            None => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                thread::sleep(remaining.min(POLL_INTERVAL));
            }
        }
    }
}

fn spawn_limited_reader<R>(
    name: &str,
    reader: R,
    max_output: usize,
) -> io::Result<Receiver<ReaderResult>>
where
    R: Read + Send + 'static,
{
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || {
            let _ = sender.send(drain_limited(reader, max_output));
        })?;
    Ok(receiver)
}

fn drain_limited(mut reader: impl Read, max_output: usize) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let bytes_read = match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(bytes_read) => bytes_read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        let retained = bytes_read.min(max_output.saturating_sub(output.len()));
        output.extend_from_slice(&chunk[..retained]);
    }
    Ok(output)
}

fn receive_limited_reader(
    reader: &Receiver<ReaderResult>,
    deadline: Instant,
) -> io::Result<Option<Vec<u8>>> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    match reader.recv_timeout(remaining) {
        Ok(result) => result.map(Some),
        Err(RecvTimeoutError::Timeout) => Ok(None),
        Err(RecvTimeoutError::Disconnected) => Err(io::Error::other(
            "child output reader thread stopped unexpectedly",
        )),
    }
}

fn terminate_process_group(child: &mut Child) {
    #[cfg(unix)]
    {
        let group = format!("-{}", child.id());
        let _ = Command::new("/bin/kill")
            .args(["-KILL", "--", group.as_str()])
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

fn timeout_output() -> LimitedOutput {
    LimitedOutput {
        timed_out: true,
        exit_code: None,
        stdout: Vec::new(),
        stderr: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drains_full_pipes_concurrently_without_exceeding_capture_limit() {
        let mut command = Command::new("/bin/sh");
        command.args([
            "-c",
            "i=0; while [ \"$i\" -lt 4096 ]; do \
             printf 'stdout-0123456789abcdef0123456789abcdef\\n'; \
             printf 'stderr-0123456789abcdef0123456789abcdef\\n' >&2; \
             i=$((i + 1)); done",
        ]);

        let output =
            run_limited(command, Duration::from_secs(5), 257).expect("bounded child should run");

        assert!(
            !output.timed_out,
            "draining output must prevent backpressure"
        );
        assert_eq!(output.exit_code, Some(0));
        assert_eq!(output.stdout.len(), 257);
        assert_eq!(output.stderr.len(), 257);
        assert!(output.stdout.starts_with(b"stdout-"));
        assert!(output.stderr.starts_with(b"stderr-"));
    }

    #[test]
    fn descendant_retaining_output_pipes_cannot_hold_the_runner() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "sleep 10 &"]);
        let started = Instant::now();

        let output = run_limited(command, Duration::from_millis(250), 64)
            .expect("runner should bound inherited pipes");

        assert!(output.timed_out);
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
