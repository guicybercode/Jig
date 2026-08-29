//! Locate the `cli-masterd` executable without interpolating a shell.

use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::DAEMON_SIDECAR_NAME;

const ENV_BINARY: &str = "CLI_MASTERD";

/// How the desktop process found a `cli-masterd` executable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DaemonBinarySource {
    /// Absolute path from the `CLI_MASTERD` environment variable.
    Environment,
    /// File named `cli-masterd` next to the running desktop executable.
    ExecutableDirectory,
    /// First executable `cli-masterd` on `PATH`.
    Path,
    /// Workspace `target/debug` or `target/release` candidate used in development.
    WorkspaceTarget,
}

/// An executable daemon binary the sidecar may spawn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocatedDaemon {
    /// Absolute or process-resolved filesystem path.
    pub path: PathBuf,
    /// Discovery rule that produced `path`.
    pub source: DaemonBinarySource,
}

/// Inputs for daemon-binary discovery. Production reads the process environment;
/// tests inject paths so they do not depend on `PATH` or `CLI_MASTERD`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocateEnv {
    /// Optional override from `CLI_MASTERD`.
    pub cli_masterd: Option<PathBuf>,
    /// Result of `current_exe()`.
    pub current_exe: PathBuf,
    /// Directories from `PATH`, in search order.
    pub path_dirs: Vec<PathBuf>,
    /// Extra development candidates such as workspace `target/*/cli-masterd`.
    pub workspace_candidates: Vec<PathBuf>,
}

impl LocateEnv {
    /// Reads discovery inputs from the current process.
    ///
    /// The `CLI_MASTERD` value is stored as a path only. Callers must not log
    /// environment contents.
    #[must_use]
    pub fn from_process() -> Self {
        Self {
            cli_masterd: env::var_os(ENV_BINARY)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
            current_exe: env::current_exe().unwrap_or_default(),
            path_dirs: env::var_os("PATH")
                .map(|value| env::split_paths(&value).collect())
                .unwrap_or_default(),
            workspace_candidates: workspace_candidates(),
        }
    }
}

/// Failure while resolving an explicit `CLI_MASTERD` override or finding any binary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LocateError {
    /// `CLI_MASTERD` was set but is not an absolute executable file.
    InvalidEnvironment,
    /// No candidate existed or was executable.
    NotFound,
}

/// Resolves `cli-masterd` using the documented Linux and macOS search order.
///
/// # Errors
///
/// Returns [`LocateError::InvalidEnvironment`] when `CLI_MASTERD` is set and
/// unusable, and [`LocateError::NotFound`] when no candidate is executable.
pub fn locate_daemon_binary(env: &LocateEnv) -> Result<LocatedDaemon, LocateError> {
    if let Some(path) = &env.cli_masterd {
        return require_absolute_executable(path).map(|path| LocatedDaemon {
            path,
            source: DaemonBinarySource::Environment,
        });
    }

    if let Some(path) = env
        .current_exe
        .parent()
        .map(|dir| dir.join(DAEMON_SIDECAR_NAME))
    {
        if is_executable_file(&path) {
            return Ok(LocatedDaemon {
                path,
                source: DaemonBinarySource::ExecutableDirectory,
            });
        }
    }

    for dir in &env.path_dirs {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let path = dir.join(DAEMON_SIDECAR_NAME);
        if is_executable_file(&path) {
            return Ok(LocatedDaemon {
                path,
                source: DaemonBinarySource::Path,
            });
        }
    }

    for path in &env.workspace_candidates {
        if is_executable_file(path) {
            return Ok(LocatedDaemon {
                path: path.clone(),
                source: DaemonBinarySource::WorkspaceTarget,
            });
        }
    }

    Err(LocateError::NotFound)
}

fn workspace_candidates() -> Vec<PathBuf> {
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../target");
    vec![
        target.join("debug").join(DAEMON_SIDECAR_NAME),
        target.join("release").join(DAEMON_SIDECAR_NAME),
    ]
}

fn require_absolute_executable(path: &Path) -> Result<PathBuf, LocateError> {
    if !path.is_absolute() || !is_executable_file(path) {
        return Err(LocateError::InvalidEnvironment);
    }
    Ok(path.to_path_buf())
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    use tempfile::TempDir;

    use super::{
        DaemonBinarySource, LocateEnv, LocateError, locate_daemon_binary,
        require_absolute_executable,
    };

    fn executable_file(root: &TempDir, name: &str) -> PathBuf {
        let path = root.path().join(name);
        fs::write(&path, b"#!/bin/true\n").expect("write stub");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
        path
    }

    fn env_with(current_exe: PathBuf) -> LocateEnv {
        LocateEnv {
            cli_masterd: None,
            current_exe,
            path_dirs: Vec::new(),
            workspace_candidates: Vec::new(),
        }
    }

    #[test]
    fn environment_override_wins_when_absolute_and_executable() {
        let root = TempDir::new().expect("tempdir");
        let binary = executable_file(&root, "cli-masterd");
        let mut env = env_with(root.path().join("desktop"));
        env.cli_masterd = Some(binary.clone());
        env.path_dirs = vec![root.path().to_path_buf()];

        let located = locate_daemon_binary(&env).expect("override should win");
        assert_eq!(located.path, binary);
        assert_eq!(located.source, DaemonBinarySource::Environment);
    }

    #[test]
    fn relative_environment_override_is_rejected() {
        let mut env = env_with(PathBuf::from("/tmp/desktop"));
        env.cli_masterd = Some(PathBuf::from("cli-masterd"));
        assert_eq!(
            locate_daemon_binary(&env).expect_err("relative override"),
            LocateError::InvalidEnvironment
        );
    }

    #[test]
    fn sibling_of_current_exe_is_used_before_path() {
        let root = TempDir::new().expect("tempdir");
        let sibling = executable_file(&root, "cli-masterd");
        let path_dir = TempDir::new().expect("path dir");
        let on_path = executable_file(&path_dir, "cli-masterd");
        let mut env = env_with(root.path().join("CLI Master"));
        env.path_dirs = vec![path_dir.path().to_path_buf()];
        env.workspace_candidates = vec![on_path];

        let located = locate_daemon_binary(&env).expect("sibling");
        assert_eq!(located.path, sibling);
        assert_eq!(located.source, DaemonBinarySource::ExecutableDirectory);
    }

    #[test]
    fn path_is_used_before_workspace_candidates() {
        let path_dir = TempDir::new().expect("path dir");
        let on_path = executable_file(&path_dir, "cli-masterd");
        let workspace = TempDir::new().expect("workspace");
        let workspace_bin = executable_file(&workspace, "cli-masterd");
        let mut env = env_with(PathBuf::from("/tmp/missing/desktop"));
        env.path_dirs = vec![path_dir.path().to_path_buf()];
        env.workspace_candidates = vec![workspace_bin];

        let located = locate_daemon_binary(&env).expect("path");
        assert_eq!(located.path, on_path);
        assert_eq!(located.source, DaemonBinarySource::Path);
    }

    #[test]
    fn missing_binary_is_not_found() {
        let env = env_with(PathBuf::from("/tmp/missing/desktop"));
        assert_eq!(
            locate_daemon_binary(&env).expect_err("none"),
            LocateError::NotFound
        );
    }

    #[test]
    fn non_executable_file_is_ignored() {
        let root = TempDir::new().expect("tempdir");
        let path = root.path().join("cli-masterd");
        fs::write(&path, b"not executable").expect("write");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod");
        assert!(require_absolute_executable(&path).is_err());
    }
}
