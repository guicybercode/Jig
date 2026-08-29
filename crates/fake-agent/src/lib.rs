//! Deterministic interactive coding-agent used by Beta acceptance tests.
//!
//! The binary speaks a tiny line protocol over a PTY. It is interactive, echoes
//! commands as `ack:` lines, writes output in small flushed fragments, reports
//! Ctrl+C without exiting, and can stay alive after stdin EOF when `--hold` is
//! set. It never prints environment values.

#![cfg_attr(not(unix), allow(unused))]
#![warn(missing_docs)]

#[cfg(not(unix))]
compile_error!("cli-master-fake-agent supports Linux and macOS only");

use std::env;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use rustix::termios::{Winsize, tcgetwinsize};
use signal_hook::consts::signal::{SIGHUP, SIGINT, SIGTERM, SIGWINCH};
use signal_hook::flag;
use signal_hook::iterator::Signals;

/// Line emitted once the process is ready to accept input.
pub const READY: &str = "FAKE_AGENT_READY";
/// Prefix of echoed command acknowledgements.
pub const ACK_PREFIX: &str = "ack:";
/// Marker written when SIGINT is delivered (for example Ctrl+C on a PTY).
pub const INTERRUPT: &str = "FAKE_AGENT_INTERRUPT";
/// Prefix of explicit size reports from the `size` command.
pub const SIZE_PREFIX: &str = "SIZE";
/// Prefix of SIGWINCH resize reports.
pub const RESIZE_PREFIX: &str = "FAKE_AGENT_RESIZE";
/// Reply used instead of dumping process environment.
pub const REDACTED: &str = "FAKE_AGENT_REDACTED";
/// Prefix of the reported process identifier.
pub const PID_PREFIX: &str = "FAKE_AGENT_PID=";
/// Prefix of the reported working directory.
pub const CWD_PREFIX: &str = "FAKE_AGENT_CWD=";
/// Marker written when `--hold` parks after stdin EOF.
pub const HOLDING: &str = "FAKE_AGENT_HOLDING";

const DEFAULT_FRAGMENT_SIZE: usize = 1;

/// Parsed launch options for the fake agent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Args {
    /// Stay alive after stdin EOF instead of exiting.
    pub hold: bool,
    /// Maximum bytes flushed together when emitting protocol output.
    pub fragment_size: usize,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            hold: false,
            fragment_size: DEFAULT_FRAGMENT_SIZE,
        }
    }
}

/// Parses argv without a shell. Unknown flags are errors.
///
/// # Errors
///
/// Returns a user-facing message when a flag is unknown or `--fragment-size`
/// is missing or not a positive integer.
pub fn parse_args<I, S>(args: I) -> Result<Args, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut parsed = Args::default();
    let mut items = args.into_iter();
    let _program = items.next();
    while let Some(argument) = items.next() {
        match argument.as_ref() {
            "--hold" => parsed.hold = true,
            "--fragment-size" => {
                let value = items
                    .next()
                    .ok_or_else(|| "missing value for --fragment-size".to_owned())?;
                let size = value
                    .as_ref()
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --fragment-size {}", value.as_ref()))?;
                if size == 0 {
                    return Err("--fragment-size must be at least 1".to_owned());
                }
                parsed.fragment_size = size;
            }
            "--help" => return Err(usage().to_owned()),
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(parsed)
}

/// Locates the compiled `cli-master-fake-agent` binary for tests.
///
/// # Panics
///
/// Panics when the binary cannot be found. Integration tests that live in a
/// different package should run `cargo build -p cli-master-fake-agent` first,
/// or `cargo test --workspace` after that package's tests have compiled it.
#[must_use]
pub fn compiled_executable() -> PathBuf {
    if let Some(path) = cargo_bin_from_env() {
        return path;
    }

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        for candidate in binary_candidates() {
            if candidate.is_file() {
                return candidate;
            }
        }
        assert!(
            Instant::now() < deadline,
            "cli-master-fake-agent was not found; run `cargo build -p cli-master-fake-agent` first"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Runs the interactive protocol until exit, EOF, or a terminating signal.
#[must_use]
pub fn run() -> i32 {
    match parse_args(env::args()) {
        Ok(args) => match run_with_args(&args) {
            Ok(code) => code,
            Err(error) => {
                let mut stderr = io::stderr();
                let _ = writeln!(stderr, "{error}");
                1
            }
        },
        Err(error) => {
            let mut stderr = io::stderr();
            let _ = writeln!(stderr, "{error}");
            2
        }
    }
}

fn run_with_args(args: &Args) -> io::Result<i32> {
    let interrupted = Arc::new(AtomicBool::new(false));
    let resized = Arc::new(AtomicBool::new(false));
    flag::register(SIGINT, Arc::clone(&interrupted))?;
    flag::register(SIGWINCH, Arc::clone(&resized))?;
    if args.hold {
        flag::register(SIGHUP, Arc::new(AtomicBool::new(false)))?;
    }

    let mut stdout_handle = io::stdout();
    write_banner(&mut stdout_handle, args.fragment_size)?;

    let mut reader = BufReader::new(io::stdin());
    let mut line = String::new();

    loop {
        if interrupted.swap(false, Ordering::SeqCst) {
            write_fragmented(&mut stdout_handle, INTERRUPT.as_bytes(), args.fragment_size)?;
            write_fragmented(&mut stdout_handle, b"\n", args.fragment_size)?;
        }
        if resized.swap(false, Ordering::SeqCst) {
            write_winsize_line(&mut stdout_handle, RESIZE_PREFIX, args.fragment_size)?;
        }

        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => {
                if args.hold {
                    write_fragmented(&mut stdout_handle, HOLDING.as_bytes(), args.fragment_size)?;
                    write_fragmented(&mut stdout_handle, b"\n", args.fragment_size)?;
                    return park_until_terminate();
                }
                return Ok(0);
            }
            Ok(_) => {
                if let Some(code) = handle_line(&mut stdout_handle, &line, args)? {
                    return Ok(code);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}

fn handle_line(stdout: &mut io::Stdout, raw: &str, args: &Args) -> io::Result<Option<i32>> {
    let line = raw.trim_end_matches(['\r', '\n']);
    if line.is_empty() {
        return Ok(None);
    }
    if line == "fail" {
        return Ok(Some(17));
    }
    if let Some(rest) = line.strip_prefix("exit") {
        let rest = rest.trim();
        if rest.is_empty() {
            return Ok(Some(0));
        }
        let code = rest.parse::<u8>().unwrap_or(1);
        return Ok(Some(i32::from(code)));
    }
    if line == "size" {
        write_winsize_line(stdout, SIZE_PREFIX, args.fragment_size)?;
        return Ok(None);
    }
    if line == "cwd" {
        write_fragmented(stdout, CWD_PREFIX.as_bytes(), args.fragment_size)?;
        write_fragmented(stdout, cwd_display().as_bytes(), args.fragment_size)?;
        write_fragmented(stdout, b"\n", args.fragment_size)?;
        return Ok(None);
    }
    if line == "env" || line == "dump-env" {
        write_fragmented(stdout, REDACTED.as_bytes(), args.fragment_size)?;
        write_fragmented(stdout, b"\n", args.fragment_size)?;
        return Ok(None);
    }

    write_fragmented(stdout, ACK_PREFIX.as_bytes(), args.fragment_size)?;
    write_fragmented(stdout, line.as_bytes(), args.fragment_size)?;
    write_fragmented(stdout, b"\n", args.fragment_size)?;
    Ok(None)
}

fn write_banner(stdout: &mut io::Stdout, fragment_size: usize) -> io::Result<()> {
    write_fragmented(stdout, READY.as_bytes(), fragment_size)?;
    write_fragmented(stdout, b" ", fragment_size)?;
    write_winsize_fields(stdout, fragment_size)?;
    write_fragmented(stdout, b"\n", fragment_size)?;

    write_fragmented(stdout, PID_PREFIX.as_bytes(), fragment_size)?;
    write_fragmented(
        stdout,
        std::process::id().to_string().as_bytes(),
        fragment_size,
    )?;
    write_fragmented(stdout, b"\n", fragment_size)?;

    write_fragmented(stdout, CWD_PREFIX.as_bytes(), fragment_size)?;
    write_fragmented(stdout, cwd_display().as_bytes(), fragment_size)?;
    write_fragmented(stdout, b"\n", fragment_size)?;
    Ok(())
}

fn write_winsize_line(
    stdout: &mut io::Stdout,
    prefix: &str,
    fragment_size: usize,
) -> io::Result<()> {
    write_fragmented(stdout, prefix.as_bytes(), fragment_size)?;
    write_fragmented(stdout, b" ", fragment_size)?;
    write_winsize_fields(stdout, fragment_size)?;
    write_fragmented(stdout, b"\n", fragment_size)
}

fn write_winsize_fields(stdout: &mut io::Stdout, fragment_size: usize) -> io::Result<()> {
    let (cols, rows) = current_winsize();
    write_fragmented(
        stdout,
        format!("cols={cols} rows={rows}").as_bytes(),
        fragment_size,
    )
}

fn current_winsize() -> (u16, u16) {
    tcgetwinsize(io::stdout()).map_or((0, 0), |size: Winsize| (size.ws_col, size.ws_row))
}

fn cwd_display() -> String {
    env::current_dir().map_or_else(|_| String::from("."), |path| path.display().to_string())
}

fn write_fragmented(stdout: &mut io::Stdout, bytes: &[u8], fragment_size: usize) -> io::Result<()> {
    let size = fragment_size.max(1);
    for chunk in bytes.chunks(size) {
        stdout.write_all(chunk)?;
        stdout.flush()?;
    }
    Ok(())
}

fn park_until_terminate() -> io::Result<i32> {
    let mut signals = Signals::new([SIGTERM, SIGINT])?;
    for signal in &mut signals {
        if signal == SIGTERM {
            return Ok(143);
        }
        if signal == SIGINT {
            let mut stdout_handle = io::stdout();
            write_fragmented(
                &mut stdout_handle,
                INTERRUPT.as_bytes(),
                DEFAULT_FRAGMENT_SIZE,
            )?;
            write_fragmented(&mut stdout_handle, b"\n", DEFAULT_FRAGMENT_SIZE)?;
        }
    }
    Ok(0)
}

fn cargo_bin_from_env() -> Option<PathBuf> {
    option_env!("CARGO_BIN_EXE_cli-master-fake-agent")
        .or(option_env!("CARGO_BIN_EXE_cli_master_fake_agent"))
        .map(PathBuf::from)
        .filter(|path| path.is_file())
}

fn binary_candidates() -> Vec<PathBuf> {
    let name = executable_name();
    let mut directories = Vec::new();
    if let Ok(target) = env::var("CARGO_TARGET_DIR") {
        directories.push(PathBuf::from(target));
    }
    if let Ok(manifest) = env::var("CARGO_MANIFEST_DIR") {
        let manifest = PathBuf::from(manifest);
        directories.push(manifest.join("target"));
        if let Some(workspace) = manifest.parent().and_then(Path::parent) {
            directories.push(workspace.join("target"));
        }
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            directories.push(dir.to_path_buf());
            if dir.ends_with("deps") {
                if let Some(parent) = dir.parent() {
                    directories.push(parent.to_path_buf());
                }
            }
        }
    }

    let mut candidates = Vec::new();
    for directory in directories {
        candidates.push(directory.join(name));
        candidates.push(directory.join("debug").join(name));
        candidates.push(directory.join("release").join(name));
    }
    candidates
}

fn executable_name() -> &'static str {
    "cli-master-fake-agent"
}

fn usage() -> &'static str {
    "cli-master-fake-agent [--hold] [--fragment-size N]"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args_accepts_hold_and_fragment_size() {
        let args = parse_args(["fake", "--hold", "--fragment-size", "4"]).expect("args");
        assert!(args.hold);
        assert_eq!(args.fragment_size, 4);
    }

    #[test]
    fn parse_args_rejects_unknown_flags() {
        let error = parse_args(["fake", "--shell"]).expect_err("unknown flag");
        assert!(error.contains("unknown argument"));
    }

    #[test]
    fn parse_args_rejects_zero_fragment_size() {
        let error = parse_args(["fake", "--fragment-size", "0"]).expect_err("zero");
        assert!(error.contains("at least 1"));
    }
}
