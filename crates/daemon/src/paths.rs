//! Platform data, log, and runtime socket locations.

use std::env;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Resolved per-user directories for CLI Master.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppPaths {
    /// SQLite database file.
    pub database: PathBuf,
    /// Structured log directory.
    pub log_dir: PathBuf,
    /// Unix socket for the daemon.
    pub socket: PathBuf,
    /// Managed Git worktree root.
    pub worktrees: PathBuf,
}

impl AppPaths {
    /// Resolves standard user directories, creating them with mode `0o700`.
    ///
    /// # Errors
    ///
    /// Returns an error when a directory cannot be created.
    pub fn for_user() -> io::Result<Self> {
        let data = data_dir();
        let runtime = runtime_dir();
        Self::from_roots(data, runtime)
    }

    /// Resolves paths under explicit data and runtime roots.
    ///
    /// # Errors
    ///
    /// Returns an error when a directory cannot be created.
    pub fn from_roots(data: impl AsRef<Path>, runtime: impl AsRef<Path>) -> io::Result<Self> {
        let data = data.as_ref();
        let runtime = runtime.as_ref();
        let log_dir = data.join("logs");
        let worktrees = data.join("worktrees");
        ensure_private_dir(data)?;
        ensure_private_dir(&log_dir)?;
        ensure_private_dir(&worktrees)?;
        ensure_private_dir(runtime)?;
        Ok(Self {
            database: data.join("cli-master.db"),
            log_dir,
            socket: runtime.join("cli-masterd.sock"),
            worktrees,
        })
    }
}

fn data_dir() -> PathBuf {
    if let Some(root) = env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(root).join("cli-master");
    }
    env::var_os("HOME").map_or_else(
        || PathBuf::from("/tmp/cli-master/data"),
        |home| PathBuf::from(home).join(".local/share/cli-master"),
    )
}

fn runtime_dir() -> PathBuf {
    if let Some(root) = env::var_os("XDG_RUNTIME_DIR") {
        let path = PathBuf::from(root).join("cli-master");
        if path.as_os_str().len() < 90 {
            return path;
        }
    }
    let user = env::var("USER").unwrap_or_else(|_| "user".to_owned());
    PathBuf::from(format!("/tmp/cli-master-{user}"))
}

fn ensure_private_dir(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::AppPaths;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn from_roots_creates_private_directories() {
        let temp = tempfile::tempdir().expect("tempdir");
        let data = temp.path().join("data");
        let runtime = temp.path().join("run");
        let paths = AppPaths::from_roots(data.clone(), runtime.clone()).expect("paths");
        assert!(paths.database.ends_with("cli-master.db"));
        assert_eq!(
            std::fs::metadata(&data)
                .expect("data meta")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(runtime.join("cli-masterd.sock"), paths.socket);
    }
}
