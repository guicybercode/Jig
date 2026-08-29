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
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread;

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

/// Locates the compiled `cli-master-fake-agent` binary without consulting
/// `PATH` or waiting for another build process.
///
/// # Errors
///
/// Returns [`io::ErrorKind::NotFound`] with every inspected path when no
/// executable file exists. Cross-package acceptance tests must build the
/// binary first with `cargo build -p cli-master-fake-agent`.
pub fn compiled_executable() -> io::Result<PathBuf> {
    let candidates = binary_candidates();
    if let Some(path) = candidates.iter().find(|path| is_executable(path)) {
        return path.canonicalize();
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!(
            "cli-master-fake-agent was not found; build it first; inspected: {}",
            candidates
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    ))
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
    let output = Arc::new(ProtocolOut::new(args.fragment_size));
    spawn_signal_reporter(Arc::clone(&output))?;
    if args.hold {
        flag::register(SIGHUP, Arc::new(AtomicBool::new(false)))?;
    }

    write_banner(&output)?;

    let mut reader = BufReader::new(io::stdin());
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => {
                if args.hold {
                    output.write_line(HOLDING)?;
                    return park_until_terminate(&output);
                }
                return Ok(0);
            }
            Ok(_) => {
                if let Some(code) = handle_line(&output, &line)? {
                    return Ok(code);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}

fn spawn_signal_reporter(output: Arc<ProtocolOut>) -> io::Result<()> {
    let mut signals = Signals::new([SIGINT, SIGWINCH])?;
    thread::Builder::new()
        .name("fake-agent-signals".to_owned())
        .spawn(move || {
            for signal in &mut signals {
                match signal {
                    SIGINT => {
                        let _ = output.write_line(INTERRUPT);
                    }
                    SIGWINCH => {
                        let _ = output.write_winsize(RESIZE_PREFIX);
                    }
                    _ => {}
                }
            }
        })?;
    Ok(())
}

fn handle_line(output: &ProtocolOut, raw: &str) -> io::Result<Option<i32>> {
    let line = raw.trim_end_matches(['\r', '\n']);
    if line.is_empty() {
        return Ok(None);
    }
    if line == "\u{3}" {
        output.write_line(INTERRUPT)?;
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
        output.write_winsize(SIZE_PREFIX)?;
        return Ok(None);
    }
    if line == "cwd" {
        output.write_line(&format!("{CWD_PREFIX}{}", cwd_display()))?;
        return Ok(None);
    }
    if line == "env" || line == "dump-env" {
        output.write_line(REDACTED)?;
        return Ok(None);
    }

    output.write_line(&format!("{ACK_PREFIX}{line}"))?;
    Ok(None)
}

fn write_banner(output: &ProtocolOut) -> io::Result<()> {
    output.write_winsize(READY)?;
    output.write_line(&format!("{PID_PREFIX}{}", std::process::id()))?;
    output.write_line(&format!("{CWD_PREFIX}{}", cwd_display()))
}

struct ProtocolOut {
    stdout: Mutex<io::Stdout>,
    fragment_size: usize,
}

impl ProtocolOut {
    fn new(fragment_size: usize) -> Self {
        Self {
            stdout: Mutex::new(io::stdout()),
            fragment_size: fragment_size.max(1),
        }
    }

    fn write_line(&self, line: &str) -> io::Result<()> {
        let mut stdout = self
            .stdout
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        write_fragmented(&mut stdout, line.as_bytes(), self.fragment_size)?;
        write_fragmented(&mut stdout, b"\n", self.fragment_size)
    }

    fn write_winsize(&self, prefix: &str) -> io::Result<()> {
        let (cols, rows) = current_winsize();
        self.write_line(&format!("{prefix} cols={cols} rows={rows}"))
    }
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

fn park_until_terminate(output: &ProtocolOut) -> io::Result<i32> {
    let mut signals = Signals::new([SIGTERM, SIGINT])?;
    for signal in &mut signals {
        if signal == SIGTERM {
            return Ok(143);
        }
        if signal == SIGINT {
            output.write_line(INTERRUPT)?;
        }
    }
    Ok(0)
}

fn binary_candidates() -> Vec<PathBuf> {
    let name = executable_name();
    let mut directories = Vec::new();
    let mut candidates = [
        "CLI_MASTER_FAKE_AGENT_BIN",
        "CARGO_BIN_EXE_cli-master-fake-agent",
        "CARGO_BIN_EXE_cli_master_fake_agent",
    ]
    .into_iter()
    .filter_map(env::var_os)
    .map(PathBuf::from)
    .map(absolute_path)
    .collect::<Vec<_>>();
    if let Some(path) = option_env!("CARGO_BIN_EXE_cli-master-fake-agent") {
        candidates.push(absolute_path(PathBuf::from(path)));
    }
    if let Some(path) = option_env!("CARGO_BIN_EXE_cli_master_fake_agent") {
        candidates.push(absolute_path(PathBuf::from(path)));
    }
    if let Some(target) = env::var_os("CARGO_TARGET_DIR") {
        directories.push(absolute_path(PathBuf::from(target)));
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if let Some(workspace) = manifest.parent().and_then(Path::parent) {
        directories.push(workspace.join("target"));
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

    for directory in directories {
        candidates.push(directory.join(name));
        candidates.push(directory.join("debug").join(name));
        candidates.push(directory.join("release").join(name));
    }
    candidates.dedup();
    candidates
}

fn absolute_path(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        env::current_dir().map_or(path.clone(), |cwd| cwd.join(path))
    }
}

fn is_executable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
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
