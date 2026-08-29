//! Binary-level protocol tests for the fake coding-agent.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use cli_master_fake_agent::{ACK_PREFIX, HOLDING, INTERRUPT, READY, REDACTED, compiled_executable};

#[test]
fn ready_line_is_emitted_before_any_command() {
    let mut child = spawn_agent(&[]);
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let line = read_until(&mut stdout, READY);
    assert!(line.contains(READY), "first protocol line: {line}");
    child.kill().expect("kill");
    let _ = child.wait();
}

#[test]
fn commands_are_echoed_as_ack_lines() {
    let mut child = spawn_agent(&[]);
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut stdin = child.stdin.take().expect("stdin");
    wait_ready(&mut stdout);
    writeln!(stdin, "alpha-one").expect("write");
    stdin.flush().expect("flush");
    let line = read_until(&mut stdout, "ack:alpha-one");
    assert!(line.contains(&format!("{ACK_PREFIX}alpha-one")));
    writeln!(stdin, "exit 0").expect("exit");
    let status = child.wait().expect("wait");
    assert_eq!(status.code(), Some(0));
}

#[test]
fn fail_and_exit_codes_are_observable() {
    let status = Command::new(compiled_executable())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .and_then(|mut child| {
            {
                let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
                let mut stdin = child.stdin.take().expect("stdin");
                wait_ready(&mut stdout);
                writeln!(stdin, "fail").expect("fail");
            }
            child.wait()
        })
        .expect("fail command");
    assert_eq!(status.code(), Some(17));
}

#[test]
fn environment_is_never_dumped() {
    let mut child = Command::new(compiled_executable())
        .env("FAKE_AGENT_SECRET", "super-secret-token")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut stdin = child.stdin.take().expect("stdin");
    wait_ready(&mut stdout);
    writeln!(stdin, "dump-env").expect("write");
    let line = read_until(&mut stdout, REDACTED);
    assert!(line.contains(REDACTED));
    writeln!(stdin, "exit 0").expect("exit");
    let output = child.wait_with_output().expect("wait");
    let combined = [
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    ]
    .join("");
    assert!(
        !combined.contains("super-secret-token"),
        "secret leaked: {combined}"
    );
}

#[test]
fn hold_keeps_the_process_alive_after_stdin_eof() {
    let mut child = spawn_agent(&["--hold"]);
    let stdout = child.stdout.take().expect("stdout");
    let stdin = child.stdin.take().expect("stdin");
    let mut reader = BufReader::new(stdout);
    wait_ready(&mut reader);
    drop(stdin);
    let _ = read_until(&mut reader, HOLDING);
    assert!(
        child.try_wait().expect("try_wait").is_none(),
        "hold process must remain after stdin EOF"
    );
    child.kill().expect("kill");
    let _ = child.wait();
}

fn spawn_agent(args: &[&str]) -> std::process::Child {
    Command::new(compiled_executable())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("cli-master-fake-agent should spawn")
}

fn wait_ready(stdout: &mut BufReader<std::process::ChildStdout>) {
    let _ = read_until(stdout, READY);
}

fn read_until(stdout: &mut BufReader<std::process::ChildStdout>, needle: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut collected = String::new();
    let mut line = String::new();
    while Instant::now() < deadline {
        line.clear();
        let bytes = stdout.read_line(&mut line).expect("read");
        if bytes == 0 {
            break;
        }
        collected.push_str(&line);
        if collected.contains(needle) {
            return collected;
        }
    }
    panic!("timed out waiting for {needle:?}, got {collected:?}");
}

#[test]
fn interrupt_constant_is_stable() {
    assert_eq!(INTERRUPT, "FAKE_AGENT_INTERRUPT");
}
