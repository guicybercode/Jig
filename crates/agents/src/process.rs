use std::{
    io::{self, Read},
    process::{Child, Command, Stdio},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub(crate) const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(2);
pub(crate) const DEFAULT_PROBE_OUTPUT_LIMIT: usize = 4096;
pub(crate) const PATH_IMPORT_OUTPUT_LIMIT: usize = 64 * 1024;

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

    let mut child = spawn_with_retry(&mut command)?;
    let started = run_spawned_child(&mut child, timeout, max_output);
    if started.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    started
}

fn run_spawned_child(
    child: &mut Child,
    timeout: Duration,
    max_output: usize,
) -> io::Result<LimitedOutput> {
    let start = Instant::now();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("child stdout pipe was not available"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("child stderr pipe was not available"))?;
    let stdout_reader = spawn_limited_reader("agent-probe-stdout", stdout, max_output)?;
    let stderr_reader = spawn_limited_reader("agent-probe-stderr", stderr, max_output)?;

    let wait_result = wait_with_timeout(child, start, timeout);
    if wait_result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }

    let stdout = join_limited_reader(stdout_reader);
    let stderr = join_limited_reader(stderr_reader);
    let (timed_out, exit_code) = wait_result?;

    Ok(LimitedOutput {
        timed_out,
        exit_code,
        stdout: stdout?,
        stderr: stderr?,
    })
}

fn spawn_with_retry(command: &mut Command) -> io::Result<Child> {
    let mut last_error = None;
    for attempt in 0..8 {
        match command.spawn() {
            Ok(child) => return Ok(child),
            Err(error) if is_transient_spawn_error(&error) => {
                last_error = Some(error);
                thread::sleep(Duration::from_millis(15 * (attempt + 1)));
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error.unwrap_or_else(|| io::Error::other("process spawn retries exhausted")))
}

fn is_transient_spawn_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
    ) || matches!(error.raw_os_error(), Some(11 | 35))
}

fn wait_with_timeout(
    child: &mut Child,
    start: Instant,
    timeout: Duration,
) -> io::Result<(bool, Option<i32>)> {
    loop {
        match child.try_wait()? {
            Some(status) => return Ok((false, status.code())),
            None if start.elapsed() >= timeout => {
                match child.kill() {
                    Ok(()) => {
                        let _ = child.wait();
                    }
                    Err(_) => {
                        let _ = child.try_wait();
                    }
                }
                return Ok((true, None));
            }
            None => {
                let remaining = timeout.saturating_sub(start.elapsed());
                thread::sleep(remaining.min(Duration::from_millis(10)));
            }
        }
    }
}

fn spawn_limited_reader<R>(
    name: &str,
    reader: R,
    max_output: usize,
) -> io::Result<JoinHandle<io::Result<Vec<u8>>>>
where
    R: Read + Send + 'static,
{
    thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || drain_limited(reader, max_output))
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

fn join_limited_reader(reader: JoinHandle<io::Result<Vec<u8>>>) -> io::Result<Vec<u8>> {
    reader
        .join()
        .map_err(|_| io::Error::other("child output reader thread panicked"))?
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
    fn times_out_a_sleeping_child_without_busy_looping() {
        let mut command = Command::new("/bin/sleep");
        command.arg("30");
        let output =
            run_limited(command, Duration::from_millis(200), 64).expect("timeout should return");
        assert!(output.timed_out);
        assert!(output.exit_code.is_none());
    }
}
