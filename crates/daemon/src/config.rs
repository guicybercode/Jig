use std::env;
use std::path::{Path, PathBuf};

use crate::DaemonError;

#[cfg(target_os = "linux")]
const APP_DIRECTORY: &str = "cli-master";
const SOCKET_FILE: &str = "daemon.sock";

/// Fully resolved filesystem locations used by one daemon instance.
///
/// Use [`Self::discover`] in production. [`Self::from_paths`] intentionally
/// bypasses environment discovery so tests and embedders can isolate state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DaemonConfig {
    data_directory: PathBuf,
    runtime_directory: PathBuf,
    database_path: PathBuf,
    socket_path: PathBuf,
    lock_path: PathBuf,
}

impl DaemonConfig {
    /// Resolves the platform-standard per-user data and runtime locations.
    ///
    /// Linux honors `XDG_DATA_HOME` and `XDG_RUNTIME_DIR`, with a private
    /// directory under the data location as the runtime fallback. macOS stores
    /// durable data below `~/Library/Application Support` and uses a short,
    /// home-specific path below the system temporary directory for its socket.
    ///
    /// # Errors
    ///
    /// Returns [`DaemonError::MissingHomeDirectory`] when no absolute home or
    /// XDG data directory can be resolved.
    pub fn discover() -> Result<Self, DaemonError> {
        let home = absolute_environment_path("HOME");

        #[cfg(target_os = "linux")]
        {
            let data_directory = absolute_environment_path("XDG_DATA_HOME")
                .map(|path| path.join(APP_DIRECTORY))
                .or_else(|| {
                    home.as_ref()
                        .map(|path| path.join(".local/share").join(APP_DIRECTORY))
                })
                .ok_or(DaemonError::MissingHomeDirectory)?;
            let runtime_directory = absolute_environment_path("XDG_RUNTIME_DIR")
                .map(|path| path.join(APP_DIRECTORY))
                .unwrap_or_else(|| data_directory.join("runtime"));
            Ok(Self::from_paths(data_directory, runtime_directory))
        }

        #[cfg(target_os = "macos")]
        {
            let home = home.ok_or(DaemonError::MissingHomeDirectory)?;
            let data_directory = home.join("Library/Application Support/CLI Master");
            let runtime_directory =
                PathBuf::from("/tmp").join(format!("cli-master-{:016x}", stable_path_hash(&home)));
            Ok(Self::from_paths(data_directory, runtime_directory))
        }
    }

    /// Builds a configuration from explicit data and runtime directories.
    ///
    /// The database and lock live in `data_directory`; the Unix socket lives in
    /// `runtime_directory`. Relative paths are accepted for controlled test
    /// environments but callers should normally supply absolute paths.
    #[must_use]
    pub fn from_paths(
        data_directory: impl Into<PathBuf>,
        runtime_directory: impl Into<PathBuf>,
    ) -> Self {
        let data_directory = data_directory.into();
        let runtime_directory = runtime_directory.into();
        Self {
            database_path: data_directory.join("cli-master.db"),
            lock_path: data_directory.join("daemon.lock"),
            socket_path: runtime_directory.join(SOCKET_FILE),
            data_directory,
            runtime_directory,
        }
    }

    /// Directory containing durable daemon state.
    #[must_use]
    pub fn data_directory(&self) -> &Path {
        &self.data_directory
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

#[cfg(target_os = "macos")]
fn stable_path_hash(path: &Path) -> u64 {
    // FNV-1a is sufficient here: the value only namespaces a local socket and
    // is not used for authentication or any cryptographic purpose.
    path.as_os_str()
        .as_encoded_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

#[cfg(test)]
mod tests {
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
    }
}
