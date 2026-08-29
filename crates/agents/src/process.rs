use std::{
    io::{self, Read},
    process::{Command, Stdio},
    thread,
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
pub(crate) fn run_limited(
    mut command: Command,
    timeout: Duration,
    max_output: usize,
) -> io::Result<LimitedOutput> {
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = command.spawn()?;
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let start = Instant::now();

    let (timed_out, exit_code) = loop {
        match child.try_wait()? {
            Some(status) => break (false, status.code()),
            None if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                break (true, None);
            }
            None => thread::sleep(Duration::from_millis(10)),
        }
    };

    let mut stdout_buf = Vec::new();
    let mut stderr_buf = Vec::new();
    if let Some(reader) = stdout.as_mut() {
        let _ = reader.read_to_end(&mut stdout_buf);
    }
    if let Some(reader) = stderr.as_mut() {
        let _ = reader.read_to_end(&mut stderr_buf);
    }
    stdout_buf.truncate(max_output);
    stderr_buf.truncate(max_output);

    Ok(LimitedOutput {
        timed_out,
        exit_code,
        stdout: stdout_buf,
        stderr: stderr_buf,
    })
}
