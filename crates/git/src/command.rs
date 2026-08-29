use std::{
    env,
    ffi::{OsStr, OsString},
    fs,
    io::Read,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
};

use crate::error::GitError;

const STDERR_LIMIT: usize = 2_048;
const DEFAULT_DIFF_LIMIT: usize = 2 * 1024 * 1024;

/// Output captured from a Git invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GitOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: Option<i32>,
    pub truncated: bool,
}

impl GitOutput {
    pub(crate) fn success(&self) -> bool {
        self.exit_code == Some(0)
    }

    pub(crate) fn stdout_str(&self) -> Result<&str, GitError> {
        std::str::from_utf8(&self.stdout).map_err(|_| GitError::InvalidOutput {
            reason: "stdout was not valid UTF-8".to_owned(),
        })
    }

    pub(crate) fn stdout_trimmed(&self) -> Result<&str, GitError> {
        Ok(self.stdout_str()?.trim())
    }

    pub(crate) fn require_success(&self, args: &[OsString]) -> Result<(), GitError> {
        if self.success() {
            Ok(())
        } else {
            Err(command_failed(args, self.exit_code, &self.stderr))
        }
    }
}

pub(crate) struct GitCommand<'a> {
    executable: &'a Path,
    args: Vec<OsString>,
}

impl<'a> GitCommand<'a> {
    pub(crate) fn new(executable: &'a Path) -> Self {
        Self {
            executable,
            args: vec![
                "--no-pager".into(),
                "-c".into(),
                "core.quotepath=false".into(),
            ],
        }
    }

    pub(crate) fn read_only(mut self) -> Self {
        self.args.insert(0, "--no-optional-locks".into());
        self
    }

    pub(crate) fn repo(mut self, path: impl AsRef<Path>) -> Self {
        self.args.push("-C".into());
        self.args.push(path.as_ref().as_os_str().to_owned());
        self
    }

    pub(crate) fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
        self.args.push(arg.as_ref().to_owned());
        self
    }

    pub(crate) fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.args
            .extend(args.into_iter().map(|arg| arg.as_ref().to_owned()));
        self
    }

    pub(crate) fn run(self) -> Result<GitOutput, GitError> {
        self.run_capped(None)
    }

    pub(crate) fn run_checked(self) -> Result<GitOutput, GitError> {
        let args = self.args.clone();
        let output = self.run()?;
        output.require_success(&args)?;
        Ok(output)
    }

    pub(crate) fn run_capped(self, stdout_limit: Option<usize>) -> Result<GitOutput, GitError> {
        let mut command = Command::new(self.executable);
        command
            .args(&self.args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_PAGER", "cat")
            .env("PAGER", "cat")
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command.spawn().map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                GitError::ExecutableNotFound
            } else {
                GitError::SpawnFailed {
                    message: error.to_string(),
                }
            }
        })?;

        let mut stdout_pipe = child.stdout.take().ok_or_else(|| GitError::Internal {
            message: "Git stdout pipe was missing".to_owned(),
        })?;
        let mut stderr_pipe = child.stderr.take().ok_or_else(|| GitError::Internal {
            message: "Git stderr pipe was missing".to_owned(),
        })?;

        let stderr_thread = thread::spawn(move || {
            let mut stderr = Vec::new();
            let mut buffer = [0_u8; 8_192];
            loop {
                match stderr_pipe.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => {
                        let remaining = STDERR_LIMIT.saturating_sub(stderr.len());
                        if remaining == 0 {
                            continue;
                        }
                        stderr.extend_from_slice(&buffer[..read.min(remaining)]);
                    }
                }
            }
            stderr
        });

        let limit = stdout_limit.unwrap_or(usize::MAX);
        let mut stdout = Vec::new();
        let mut buffer = [0_u8; 8_192];
        let mut truncated = false;
        loop {
            match stdout_pipe.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    if stdout.len() >= limit {
                        truncated = true;
                        break;
                    }
                    let allowed = limit - stdout.len();
                    let take = read.min(allowed);
                    stdout.extend_from_slice(&buffer[..take]);
                    if take < read {
                        truncated = true;
                        break;
                    }
                }
                Err(error) => {
                    return Err(GitError::SpawnFailed {
                        message: error.to_string(),
                    });
                }
            }
        }

        if truncated {
            let _ = child.kill();
            let _ = stdout_pipe.read_to_end(&mut Vec::new());
        }

        let status = child.wait().map_err(|error| GitError::SpawnFailed {
            message: error.to_string(),
        })?;
        let stderr = stderr_thread.join().unwrap_or_default();

        Ok(GitOutput {
            stdout,
            stderr,
            exit_code: status.code(),
            truncated,
        })
    }
}

pub(crate) fn command_failed(args: &[OsString], exit_code: Option<i32>, stderr: &[u8]) -> GitError {
    GitError::CommandFailed {
        args: args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect(),
        exit_code,
        stderr: String::from_utf8_lossy(stderr).trim().to_owned(),
    }
}

pub(crate) fn default_diff_limit() -> usize {
    DEFAULT_DIFF_LIMIT
}

pub(crate) fn resolve_git_executable() -> Result<PathBuf, GitError> {
    match env::var_os("PATH") {
        Some(path) => resolve_git_in_path(&path),
        None => Err(GitError::ExecutableNotFound),
    }
}

pub(crate) fn resolve_git_in_path(path: &OsStr) -> Result<PathBuf, GitError> {
    for directory in env::split_paths(path) {
        let candidate = directory.join("git");
        if is_executable_file(&candidate) {
            return Ok(candidate);
        }
    }
    Err(GitError::ExecutableNotFound)
}

pub(crate) fn is_executable_file(path: &Path) -> bool {
    fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

pub(crate) fn inspect_path(path: &Path) -> Result<(), GitError> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(GitError::NotADirectory {
            path: path.to_path_buf(),
        }),
        Err(_) => Err(GitError::PathNotFound {
            path: path.to_path_buf(),
        }),
    }
}
