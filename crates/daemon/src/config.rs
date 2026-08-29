use std::env;
use std::path::{Path, PathBuf};

use crate::DaemonError;
use crate::paths::{
    UserDirectories, database_path, ensure_private_directory, lock_path, log_file_path, socket_path,
};

#[cfg(target_os = "linux")]
use crate::paths::linux_runtime_fallback;

/// Fully resolved filesystem locations used by one daemon instance.
///
/// Use [`Self::discover`] in production. [`Self::from_paths`] intentionally
/// bypasses environment discovery so tests and embedders can isolate state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DaemonConfig {
    data_directory: PathBuf,
    config_directory: PathBuf,
    cache_directory: PathBuf,
    log_directory: PathBuf,
    runtime_directory: PathBuf,
    database_path: PathBuf,
    socket_path: PathBuf,
    lock_path: PathBuf,
}

impl DaemonConfig {
    /// Resolves the platform-standard per-user data, config, cache, log, and
    /// runtime locations.
    ///
    /// Linux honors the XDG base directories, with a user-owned directory under
    /// the process temporary directory as the runtime fallback. macOS stores
    /// durable data below `~/Library/Application Support/CLI Master`, logs
    /// below `~/Library/Logs/CLI Master`, and uses a short home-specific path
    /// under `/tmp` for the Unix socket.
    ///
    /// # Errors
    ///
    /// Returns [`DaemonError::MissingHomeDirectory`] when no absolute home or
    /// XDG data directory can be resolved.
    pub fn discover() -> Result<Self, DaemonError> {
        let home = absolute_environment_path("HOME");

        #[cfg(target_os = "linux")]
        {
            let uid = rustix::process::geteuid().as_raw();
            let directories = UserDirectories::linux(
                home.as_deref(),
                absolute_environment_path("XDG_DATA_HOME").as_deref(),
                absolute_environment_path("XDG_CONFIG_HOME").as_deref(),
                absolute_environment_path("XDG_CACHE_HOME").as_deref(),
                absolute_environment_path("XDG_STATE_HOME").as_deref(),
                absolute_environment_path("XDG_RUNTIME_DIR").as_deref(),
                &linux_runtime_fallback(uid),
            )?;
            Ok(Self::from_directories(directories))
        }

        #[cfg(target_os = "macos")]
        {
            let home = home.ok_or(DaemonError::MissingHomeDirectory)?;
            Ok(Self::from_directories(UserDirectories::macos(&home)))
        }
    }

    /// Builds a configuration from explicit data and runtime directories.
    ///
    /// The database and lock live in `data_directory`. Config, cache, and log
    /// directories are nested under it so tests stay inside one root. The Unix
    /// socket lives in `runtime_directory`. Relative paths are accepted for
    /// controlled test environments but callers should normally supply absolute
    /// paths.
    #[must_use]
    pub fn from_paths(
        data_directory: impl Into<PathBuf>,
        runtime_directory: impl Into<PathBuf>,
    ) -> Self {
        Self::from_directories(UserDirectories::from_test_roots(
            data_directory.into(),
            runtime_directory.into(),
        ))
    }

    fn from_directories(directories: UserDirectories) -> Self {
        Self {
            database_path: database_path(&directories.data),
            lock_path: lock_path(&directories.data),
            socket_path: socket_path(&directories.runtime),
            data_directory: directories.data,
            config_directory: directories.config,
            cache_directory: directories.cache,
            log_directory: directories.logs,
            runtime_directory: directories.runtime,
        }
    }

    /// Creates every private directory with mode `0700` and current-user ownership.
    ///
    /// # Errors
    ///
    /// Returns [`DaemonError::UntrustedDirectory`] when a path is a symlink or
    /// owned by another user, or [`DaemonError::Io`] when the directory cannot
    /// be created or chmod'd.
    pub fn prepare_private_directories(&self) -> Result<(), DaemonError> {
        for path in [
            self.data_directory(),
            self.config_directory(),
            self.cache_directory(),
            self.log_directory(),
            self.runtime_directory(),
        ] {
            ensure_private_directory(path)?;
        }
        Ok(())
    }

    /// Directory containing durable daemon state.
    #[must_use]
    pub fn data_directory(&self) -> &Path {
        &self.data_directory
    }

    /// Directory containing user configuration. Unused by v0.1 except to reserve
    /// the XDG location with user-only permissions.
    #[must_use]
    pub fn config_directory(&self) -> &Path {
        &self.config_directory
    }

    /// Directory containing disposable cache files.
    #[must_use]
    pub fn cache_directory(&self) -> &Path {
        &self.cache_directory
    }

    /// Directory containing rotated structured logs.
    #[must_use]
    pub fn log_directory(&self) -> &Path {
        &self.log_directory
    }

    /// Path of the current structured daemon log file.
    #[must_use]
    pub fn log_file_path(&self) -> PathBuf {
        log_file_path(&self.log_directory)
    }

    /// Opens the structured log file after ensuring the log directory is private.
    ///
    /// # Errors
    ///
    /// Returns an error when the log directory cannot be secured or the file
    /// cannot be created with mode `0600`.
    pub fn open_structured_log(&self) -> Result<std::fs::File, DaemonError> {
        ensure_private_directory(self.log_directory())?;
        crate::paths::open_log_file(&self.log_file_path())
    }

    /// Private directory containing ephemeral runtime state.
    #[must_use]
    pub fn runtime_directory(&self) -> &Path {
        &self.runtime_directory
    }

    /// Path of the daemon's `SQLite` database.
    #[must_use]
    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    /// Path of the daemon's Unix domain socket.
    #[must_use]
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Path of the per-user single-instance lock.
    #[must_use]
    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }
}

fn absolute_environment_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::DaemonConfig;

    #[test]
    fn explicit_paths_derive_all_owned_files() {
        let config = DaemonConfig::from_paths("/data/cli-master", "/run/cli-master");

        assert_eq!(
            config.database_path(),
            std::path::Path::new("/data/cli-master/cli-master.db")
        );
        assert_eq!(
            config.lock_path(),
            std::path::Path::new("/data/cli-master/daemon.lock")
        );
        assert_eq!(
            config.socket_path(),
            std::path::Path::new("/run/cli-master/daemon.sock")
        );
        assert_eq!(
            config.config_directory(),
            std::path::Path::new("/data/cli-master/config")
        );
        assert_eq!(
            config.cache_directory(),
            std::path::Path::new("/data/cli-master/cache")
        );
        assert_eq!(
            config.log_directory(),
            std::path::Path::new("/data/cli-master/logs")
        );
        assert_eq!(
            config.log_file_path(),
            std::path::Path::new("/data/cli-master/logs/cli-masterd.json.log")
        );
    }

    #[test]
    fn structured_log_file_is_created_with_mode_600() {
        let temporary = tempfile::TempDir::new().expect("temporary directory should exist");
        let config =
            DaemonConfig::from_paths(temporary.path().join("data"), temporary.path().join("run"));
        drop(config.open_structured_log().expect("log file should open"));
        let mode = std::fs::metadata(config.log_file_path())
            .expect("log metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
}
