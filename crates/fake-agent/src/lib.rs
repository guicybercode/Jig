//! Deterministic interactive CLI used by tests in place of vendor agents.
//!
//! The binary speaks a small scripted protocol: optional banner, cwd/args
//! reporting, a single prompt, optional streaming or bulk output, and a
//! configurable exit. It never prints the process environment.

use std::io::{self, ErrorKind, Read, Write};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use signal_hook::consts::SIGTERM;
use signal_hook::flag;

/// Parsed fake-agent invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Options {
    /// Banner printed at startup. `None` skips the banner.
    pub banner: Option<String>,
    /// When true, write `cwd=<absolute path>`.
    pub report_cwd: bool,
    /// When true, write `args=` followed by the process arguments after argv0.
    pub report_args: bool,
    /// When true, wait for one line of stdin after printing `READY>`.
    pub prompt: bool,
    /// Reply written after a prompt. `{input}` is replaced with the line.
    pub reply: String,
    /// Optional delay after the banner and reports.
    pub sleep: Duration,
    /// Number of numbered stream lines to emit.
    pub stream_lines: u32,
    /// Delay between stream lines.
    pub stream_interval: Duration,
    /// When true, emit a known Unicode probe line.
    pub unicode: bool,
    /// Optional bulk output of ASCII `x` bytes.
    pub bytes: Option<usize>,
    /// Process exit code.
    pub exit_code: i32,
    /// When true, SIGTERM is ignored so callers can exercise force-kill.
    pub ignore_sigterm: bool,
    /// When true, keep running after scripted actions until stdin EOF or SIGKILL.
    pub hold: bool,
    /// Process arguments after argv0, used only for `--report-args`.
    pub reported_args: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            banner: Some("cli-master-fake-agent".to_owned()),
            report_cwd: true,
            report_args: true,
            prompt: true,
            reply: "ack:{input}".to_owned(),
            sleep: Duration::ZERO,
            stream_lines: 0,
            stream_interval: Duration::from_millis(25),
            unicode: false,
            bytes: None,
            exit_code: 0,
            ignore_sigterm: false,
            hold: false,
            reported_args: Vec::new(),
        }
    }
}

impl Options {
    /// Parses argv-style arguments. `argv0` is skipped by the caller.
    ///
    /// # Errors
    ///
    /// Returns an error when a flag is unknown or a value cannot be parsed.
    pub fn parse<I, S>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let collected: Vec<String> = args
            .into_iter()
            .map(|arg| arg.as_ref().to_owned())
            .collect();
        let mut options = Self::default();
        options.reported_args.clone_from(&collected);
        let mut iter = collected.into_iter().peekable();
        let mut passthrough = false;

        while let Some(arg) = iter.next() {
            if passthrough {
                continue;
            }
            if arg == "--" {
                passthrough = true;
                continue;
            }
            match arg.as_str() {
                "--no-banner" => options.banner = None,
                "--no-input" => options.prompt = false,
                "--prompt" => options.prompt = true,
                "--report-cwd" => options.report_cwd = true,
                "--no-report-cwd" => options.report_cwd = false,
                "--report-args" => options.report_args = true,
                "--no-report-args" => options.report_args = false,
                "--unicode" => options.unicode = true,
                "--ignore-sigterm" => options.ignore_sigterm = true,
                "--hold" => options.hold = true,
                "--banner" => options.banner = Some(require_value("--banner", iter.next())?),
                "--reply" => options.reply = require_value("--reply", iter.next())?,
                "--sleep-ms" => {
                    options.sleep = Duration::from_millis(parse_int("--sleep-ms", iter.next())?);
                }
                "--stream-lines" => {
                    options.stream_lines = parse_int("--stream-lines", iter.next())?;
                }
                "--stream-interval-ms" => {
                    options.stream_interval =
                        Duration::from_millis(parse_int("--stream-interval-ms", iter.next())?);
                }
                "--bytes" => options.bytes = Some(parse_int("--bytes", iter.next())?),
                "--exit-code" => options.exit_code = parse_int("--exit-code", iter.next())?,
                other if other.starts_with("--banner=") => {
                    options.banner = Some(other[9..].to_owned());
                }
                other if other.starts_with("--reply=") => {
                    other[8..].clone_into(&mut options.reply);
                }
                other if other.starts_with("--sleep-ms=") => {
                    options.sleep = Duration::from_millis(parse_int(
                        "--sleep-ms",
                        Some(other[11..].to_owned()),
                    )?);
                }
                other if other.starts_with("--exit-code=") => {
                    options.exit_code = parse_int("--exit-code", Some(other[12..].to_owned()))?;
                }
                other if other.starts_with("--bytes=") => {
                    options.bytes = Some(parse_int("--bytes", Some(other[8..].to_owned()))?);
                }
                other if other.starts_with('-') => {
                    return Err(format!("unknown flag: {other}"));
                }
                _ => {}
            }
        }

        Ok(options)
    }
}

fn require_value(flag: &str, value: Option<impl AsRef<str>>) -> Result<String, String> {
    value
        .map(|value| value.as_ref().to_owned())
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_int<T>(flag: &str, value: Option<impl AsRef<str>>) -> Result<T, String>
where
    T: std::str::FromStr,
{
    let value = require_value(flag, value)?;
    value
        .parse()
        .map_err(|_| format!("{flag} expected an integer, got {value:?}"))
}

/// Locates the compiled fake-agent binary for workspace tests.
///
/// # Panics
///
/// Panics when the binary has not been built. `scripts/check.sh` and CI build
/// this package before running tests that spawn it.
#[must_use]
pub fn compiled_executable() -> PathBuf {
    let name = format!("cli-master-fake-agent{}", std::env::consts::EXE_SUFFIX);
    let mut candidates = Vec::new();

    if let Ok(target) = std::env::var("CARGO_TARGET_DIR") {
        let target = PathBuf::from(target);
        candidates.push(target.join("debug").join(&name));
        candidates.push(target.join("release").join(&name));
    }

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if let Some(workspace) = manifest.parent().and_then(std::path::Path::parent) {
        candidates.push(workspace.join("target/debug").join(&name));
        candidates.push(workspace.join("target/release").join(&name));
    }

    if let Ok(current) = std::env::current_exe()
        && let Some(profile_dir) = current.parent().and_then(std::path::Path::parent)
    {
        candidates.push(profile_dir.join(&name));
    }

    candidates.into_iter().find(|path| path.is_file()).unwrap_or_else(|| {
        panic!(
            "cli-master-fake-agent was not built; run `cargo build -p cli-master-fake-agent --locked`"
        )
    })
}

/// Runs the fake agent against explicit IO streams.
///
/// # Errors
///
/// Returns an IO error when writing output or reading the prompt fails for a
/// reason other than EOF or interruption.
pub fn run_with_io(
    options: &Options,
    stdin: &mut impl Read,
    stdout: &mut impl Write,
) -> io::Result<i32> {
    if options.ignore_sigterm {
        let ignored = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        flag::register(SIGTERM, ignored)
            .map_err(|error| io::Error::other(format!("failed to ignore SIGTERM: {error}")))?;
    }

    if let Some(banner) = &options.banner {
        writeln!(stdout, "{banner}")?;
        stdout.flush()?;
    }

    if options.report_cwd {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        writeln!(stdout, "cwd={}", cwd.display())?;
        stdout.flush()?;
    }

    if options.report_args {
        writeln!(stdout, "args={}", options.reported_args.join("\t"))?;
        stdout.flush()?;
    }

    if !options.sleep.is_zero() {
        thread::sleep(options.sleep);
    }

    if options.unicode {
        writeln!(stdout, "unicode: café 日本語 🦀")?;
        stdout.flush()?;
    }

    if options.stream_lines > 0 {
        for index in 1..=options.stream_lines {
            writeln!(stdout, "stream:{index}")?;
            stdout.flush()?;
            if index < options.stream_lines && !options.stream_interval.is_zero() {
                thread::sleep(options.stream_interval);
            }
        }
    }

    if let Some(count) = options.bytes {
        const CHUNK: usize = 4096;
        let chunk = vec![b'x'; CHUNK.min(count.max(1))];
        let mut remaining = count;
        while remaining > 0 {
            let n = remaining.min(chunk.len());
            stdout.write_all(&chunk[..n])?;
            remaining -= n;
        }
        stdout.write_all(b"\n")?;
        stdout.flush()?;
    }

    if options.prompt {
        write!(stdout, "READY>")?;
        stdout.flush()?;
        let mut line = String::new();
        read_line_retry(stdin, &mut line)?;
        let input = line.trim_end_matches(['\n', '\r']);
        let reply = options.reply.replace("{input}", input);
        writeln!(stdout, "{reply}")?;
        stdout.flush()?;
    }

    if options.hold {
        let mut sink = Vec::new();
        loop {
            match stdin.read_to_end(&mut sink) {
                Ok(_) => break,
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) => return Err(error),
            }
        }
    }

    Ok(options.exit_code)
}

fn read_line_retry(stdin: &mut impl Read, line: &mut String) -> io::Result<()> {
    let mut buffer = Vec::new();
    let mut byte = [0_u8; 1];
    loop {
        match stdin.read(&mut byte) {
            Ok(0) => break,
            Ok(_) => {
                buffer.push(byte[0]);
                if byte[0] == b'\n' {
                    break;
                }
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    *line = String::from_utf8_lossy(&buffer).into_owned();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn parse_supports_interactive_and_exit_flags() {
        let options = Options::parse([
            "--banner",
            "hello",
            "--reply",
            "got:{input}",
            "--exit-code",
            "7",
            "--unicode",
            "--",
            "--looks-like-flag",
            "file with spaces",
        ])
        .expect("options should parse");

        assert_eq!(options.banner.as_deref(), Some("hello"));
        assert_eq!(options.reply, "got:{input}");
        assert_eq!(options.exit_code, 7);
        assert!(options.unicode);
        assert!(
            options
                .reported_args
                .iter()
                .any(|arg| arg == "file with spaces")
        );
    }

    #[test]
    fn run_reports_cwd_and_args_without_environment() {
        let options = Options::parse(["--no-input", "--banner", "hello-agent", "alpha"])
            .expect("options should parse");

        let mut stdin = Cursor::new(Vec::new());
        let mut stdout = Vec::new();
        let code = run_with_io(&options, &mut stdin, &mut stdout).expect("run should succeed");

        let output = String::from_utf8(stdout).expect("utf8");
        assert_eq!(code, 0);
        assert!(output.contains("hello-agent"));
        assert!(output.contains("cwd="));
        assert!(output.contains("args="));
        assert!(output.contains("alpha"));
        assert!(!output.contains("PATH="));
        assert!(!output.contains("HOME="));
        assert!(!output.to_ascii_uppercase().contains("SECRET="));
    }

    #[test]
    fn prompt_echoes_configured_reply() {
        let options = Options::parse(["--no-banner", "--no-report-cwd", "--no-report-args"])
            .expect("options should parse");
        let mut stdin = Cursor::new(b"ping\n".to_vec());
        let mut stdout = Vec::new();
        run_with_io(&options, &mut stdin, &mut stdout).expect("run should succeed");
        let output = String::from_utf8(stdout).expect("utf8");
        assert!(output.contains("READY>"));
        assert!(output.contains("ack:ping"));
    }

    #[test]
    fn unknown_flag_is_rejected() {
        let error = Options::parse(["--explode"]).expect_err("unknown flag");
        assert!(error.contains("unknown flag"));
    }
}
