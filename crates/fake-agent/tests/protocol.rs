//! Binary-level protocol tests for the fake coding-agent.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use cli_master_fake_agent::{ACK_PREFIX, HOLDING, INTERRUPT, READY, REDACTED, compiled_executable};

const IO_TIMEOUT: Duration = Duration::from_secs(8);

#[test]
fn ready_line_is_emitted_before_any_command() {
    let mut agent = AgentProcess::spawn(&[], &[]);
    let output = agent.read_until(READY);
    assert_eq!(
        output.lines().next(),
        Some("FAKE_AGENT_READY cols=0 rows=0")
    );
}

#[test]
fn commands_are_echoed_as_ack_lines() {
    let mut agent = AgentProcess::spawn(&[], &[]);
    agent.read_until(READY);
    agent.write_line("alpha-one");
    let output = agent.read_until("ack:alpha-one");
    assert!(output.contains(&format!("{ACK_PREFIX}alpha-one")));
    agent.write_line("exit 0");
    assert_eq!(agent.wait_for_exit().code(), Some(0));
}

#[test]
fn fail_and_exit_codes_are_observable() {
    let mut agent = AgentProcess::spawn(&[], &[]);
    agent.read_until(READY);
    agent.write_line("fail");
    assert_eq!(agent.wait_for_exit().code(), Some(17));
}

#[test]
fn environment_is_never_dumped() {
    let mut agent = AgentProcess::spawn(&[], &[("FAKE_AGENT_SECRET", "super-secret-token")]);
    agent.read_until(READY);
    agent.write_line("dump-env");
    assert!(agent.read_until(REDACTED).contains(REDACTED));
    agent.write_line("exit 0");
    assert_eq!(agent.wait_for_exit().code(), Some(0));
    let output = agent.output().to_owned();
    let stderr = agent.read_stderr();
    let combined = format!("{output}{stderr}");
    assert!(
        !combined.contains("super-secret-token"),
        "secret leaked: {combined}"
    );
}

#[test]
fn hold_keeps_the_process_alive_after_stdin_eof() {
    let mut agent = AgentProcess::spawn(&["--hold"], &[]);
    agent.read_until(READY);
    agent.close_stdin();
    agent.read_until(HOLDING);
    assert!(
        agent.try_wait().is_none(),
        "hold process must remain after stdin EOF"
    );
}

#[test]
fn interrupt_constant_is_stable() {
    assert_eq!(INTERRUPT, "FAKE_AGENT_INTERRUPT");
}

struct AgentProcess {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<io::Result<String>>,
    reader: Option<JoinHandle<()>>,
    collected: String,
}

impl AgentProcess {
    fn spawn(args: &[&str], env: &[(&str, &str)]) -> Self {
        let executable = compiled_executable().expect("fake-agent binary should be available");
        assert!(executable.is_absolute());
        let mut child = Command::new(executable)
            .args(args)
            .envs(env.iter().copied())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("cli-master-fake-agent should spawn");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        let (sender, lines) = mpsc::channel();
        let reader = thread::Builder::new()
            .name("fake-agent-test-output".to_owned())
            .spawn(move || {
                let mut stdout = BufReader::new(stdout);
                loop {
                    let mut line = String::new();
                    match stdout.read_line(&mut line) {
                        Ok(0) => break,
                        Ok(_) => {
                            if sender.send(Ok(line)).is_err() {
                                break;
                            }
                        }
                        Err(error) => {
                            let _ = sender.send(Err(error));
                            break;
                        }
                    }
                }
            })
            .expect("output reader should start");
        Self {
            child,
            stdin: Some(stdin),
            lines,
            reader: Some(reader),
            collected: String::new(),
        }
    }

    fn write_line(&mut self, line: &str) {
        let stdin = self.stdin.as_mut().expect("stdin should remain open");
        writeln!(stdin, "{line}").expect("protocol input should be written");
        stdin.flush().expect("protocol input should flush");
    }

    fn close_stdin(&mut self) {
        self.stdin.take();
    }

    fn read_until(&mut self, needle: &str) -> String {
        if self.collected.contains(needle) {
            return self.collected.clone();
        }
        let deadline = Instant::now() + IO_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "timed out waiting for {needle:?}, got {:?}",
                self.collected
            );
            match self.lines.recv_timeout(remaining) {
                Ok(Ok(line)) => self.collected.push_str(&line),
                Ok(Err(error)) => panic!("fake-agent output failed: {error}"),
                Err(RecvTimeoutError::Timeout) => {
                    panic!("timed out waiting for {needle:?}, got {:?}", self.collected)
                }
                Err(RecvTimeoutError::Disconnected) => panic!(
                    "fake-agent output ended before {needle:?}, got {:?}",
                    self.collected
                ),
            }
            if self.collected.contains(needle) {
                return self.collected.clone();
            }
        }
    }

    fn wait_for_exit(&mut self) -> ExitStatus {
        let deadline = Instant::now() + IO_TIMEOUT;
        loop {
            if let Some(status) = self.child.try_wait().expect("child status") {
                return status;
            }
            assert!(Instant::now() < deadline, "fake agent did not exit in time");
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn try_wait(&mut self) -> Option<ExitStatus> {
        self.child.try_wait().expect("child status")
    }

    fn output(&self) -> &str {
        &self.collected
    }

    fn read_stderr(&mut self) -> String {
        let mut stderr = String::new();
        self.child
            .stderr
            .take()
            .expect("stderr")
            .read_to_string(&mut stderr)
            .expect("stderr should be readable after exit");
        stderr
    }
}

impl Drop for AgentProcess {
    fn drop(&mut self) {
        self.close_stdin();
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
