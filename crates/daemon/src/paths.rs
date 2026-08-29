use std::fs;
use std::io;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use rustix::process::geteuid;

use crate::DaemonError;

#[cfg(target_os = "linux")]
const APP_DIRECTORY: &str = "cli-master";
const SOCKET_FILE: &str = "daemon.sock";
const DATABASE_FILE: &str = "cli-master.db";
const LOCK_FILE: &str = "daemon.lock";
const LOG_FILE: &str = "cli-masterd.json.log";

/// Platform directories used by one daemon instance before derived files
/// (database, socket, lock, log file) are attached.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UserDirectories {
    pub data: PathBuf,
    pub config: PathBuf,
    pub cache: PathBuf,
    pub logs: PathBuf,
    pub runtime: PathBuf,
}

impl UserDirectories {
    pub(crate) fn from_test_roots(data_directory: PathBuf, runtime_directory: PathBuf) -> Self {
        Self {
            config: data_directory.join("config"),
            cache: data_directory.join("cache"),
            logs: data_directory.join("logs"),
            data: data_directory,
            runtime: runtime_directory,
        }
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn linux(
        home: Option<&Path>,
        xdg_data_home: Option<&Path>,
        xdg_config_home: Option<&Path>,
        xdg_cache_home: Option<&Path>,
        xdg_state_home: Option<&Path>,
        xdg_runtime_dir: Option<&Path>,
        runtime_fallback: &Path,
    ) -> Result<Self, DaemonError> {
        let data_directory = xdg_data_home
            .map(|path| path.join(APP_DIRECTORY))
            .or_else(|| home.map(|path| path.join(".local/share").join(APP_DIRECTORY)))
            .ok_or(DaemonError::MissingHomeDirectory)?;
        let config_directory = xdg_config_home
            .map(|path| path.join(APP_DIRECTORY))
            .or_else(|| home.map(|path| path.join(".config").join(APP_DIRECTORY)))
            .ok_or(DaemonError::MissingHomeDirectory)?;
        let cache_directory = xdg_cache_home
            .map(|path| path.join(APP_DIRECTORY))
            .or_else(|| home.map(|path| path.join(".cache").join(APP_DIRECTORY)))
            .ok_or(DaemonError::MissingHomeDirectory)?;
        let log_directory = xdg_state_home
            .map(|path| path.join(APP_DIRECTORY).join("logs"))
            .or_else(|| home.map(|path| path.join(".local/state").join(APP_DIRECTORY).join("logs")))
            .ok_or(DaemonError::MissingHomeDirectory)?;
        let runtime_directory = xdg_runtime_dir.map_or_else(
            || runtime_fallback.to_path_buf(),
            |path| path.join(APP_DIRECTORY),
        );
        Ok(Self {
            data: data_directory,
            config: config_directory,
            cache: cache_directory,
            logs: log_directory,
            runtime: runtime_directory,
        })
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn macos(home: &Path) -> Self {
        Self {
            data: home.join("Library/Application Support/CLI Master"),
            config: home.join("Library/Application Support/CLI Master"),
            cache: home.join("Library/Caches/CLI Master"),
            logs: home.join("Library/Logs/CLI Master"),
            runtime: PathBuf::from("/tmp")
                .join(format!("cli-master-{:016x}", stable_path_hash(home))),
        }
    }
}

pub(crate) fn database_path(data_directory: &Path) -> PathBuf {
    data_directory.join(DATABASE_FILE)
}

pub(crate) fn lock_path(data_directory: &Path) -> PathBuf {
    data_directory.join(LOCK_FILE)
}

pub(crate) fn socket_path(runtime_directory: &Path) -> PathBuf {
    runtime_directory.join(SOCKET_FILE)
}

pub(crate) fn log_file_path(log_directory: &Path) -> PathBuf {
    log_directory.join(LOG_FILE)
}

#[cfg(target_os = "linux")]
pub(crate) fn linux_runtime_fallback(uid: u32) -> PathBuf {
    std::env::temp_dir().join(format!("cli-master-{uid}"))
}

/// Creates `path` if needed, then requires a user-owned directory with mode `0700`.
///
/// Symlinks are rejected so a shared `/tmp` entry cannot redirect the daemon.
pub(crate) fn ensure_private_directory(path: &Path) -> Result<(), DaemonError> {
    fs::create_dir_all(path)
        .map_err(|error| DaemonError::io("create private directory", path, error))?;
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| DaemonError::io("inspect private directory", path, error))?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
        return Err(DaemonError::UntrustedDirectory {
            path: path.to_path_buf(),
            reason: "path must be a directory, not a symlink or other file",
        });
    }
    let expected_uid = geteuid().as_raw();
    if metadata.uid() != expected_uid {
        return Err(DaemonError::UntrustedDirectory {
            path: path.to_path_buf(),
            reason: "directory is not owned by the current user",
        });
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| DaemonError::io("secure private directory", path, error))?;
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| DaemonError::io("reinspect private directory", path, error))?;
    if metadata.permissions().mode() & 0o777 != 0o700 {
        return Err(DaemonError::io(
            "secure private directory",
            path,
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                "directory mode is not 0700 after chmod",
            ),
        ));
    }
    Ok(())
}

/// Opens the daemon log file with mode `0600`, rotating a previous file larger
/// than 10 MiB to `*.1`.
pub(crate) fn open_log_file(path: &Path) -> Result<fs::File, DaemonError> {
    const MAX_BYTES: u64 = 10 * 1024 * 1024;
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.is_file() && metadata.len() > MAX_BYTES {
            let rotated = path.with_extension("json.log.1");
            fs::rename(path, &rotated)
                .map_err(|error| DaemonError::io("rotate daemon log", path, error))?;
            fs::set_permissions(&rotated, fs::Permissions::from_mode(0o600)).ok();
        }
    }

    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
        .map_err(|error| DaemonError::io("open daemon log", path, error))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| DaemonError::io("secure daemon log", path, error))?;
    Ok(file)
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
    use std::os::unix::fs::PermissionsExt;

    use tempfile::TempDir;

    use super::{UserDirectories, ensure_private_directory};

    #[cfg(target_os = "linux")]
    use super::linux_runtime_fallback;

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_paths_honor_xdg_overrides() {
        let dirs = UserDirectories::linux(
            Some(std::path::Path::new("/home/user")),
            Some(std::path::Path::new("/xdg/data")),
            Some(std::path::Path::new("/xdg/config")),
            Some(std::path::Path::new("/xdg/cache")),
            Some(std::path::Path::new("/xdg/state")),
            Some(std::path::Path::new("/run/user/1000")),
            std::path::Path::new("/tmp/cli-master-1000"),
        )
        .expect("linux directories should resolve");

        assert_eq!(dirs.data, std::path::Path::new("/xdg/data/cli-master"));
        assert_eq!(dirs.config, std::path::Path::new("/xdg/config/cli-master"));
        assert_eq!(dirs.cache, std::path::Path::new("/xdg/cache/cli-master"));
        assert_eq!(
            dirs.logs,
            std::path::Path::new("/xdg/state/cli-master/logs")
        );
        assert_eq!(
            dirs.runtime,
            std::path::Path::new("/run/user/1000/cli-master")
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_paths_fall_back_to_home_and_tmp() {
        let fallback = linux_runtime_fallback(1000);
        let dirs = UserDirectories::linux(
            Some(std::path::Path::new("/home/user")),
            None,
            None,
            None,
            None,
            None,
            &fallback,
        )
        .expect("linux directories should resolve");

        assert_eq!(
            dirs.data,
            std::path::Path::new("/home/user/.local/share/cli-master")
        );
        assert_eq!(
            dirs.config,
            std::path::Path::new("/home/user/.config/cli-master")
        );
        assert_eq!(
            dirs.cache,
            std::path::Path::new("/home/user/.cache/cli-master")
        );
        assert_eq!(
            dirs.logs,
            std::path::Path::new("/home/user/.local/state/cli-master/logs")
        );
        assert_eq!(dirs.runtime, fallback);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_paths_use_application_support_and_logs() {
        let dirs = UserDirectories::macos(std::path::Path::new("/Users/ada"));
        assert_eq!(
            dirs.data,
            std::path::Path::new("/Users/ada/Library/Application Support/CLI Master")
        );
        assert_eq!(
            dirs.config,
            std::path::Path::new("/Users/ada/Library/Application Support/CLI Master")
        );
        assert_eq!(
            dirs.cache,
            std::path::Path::new("/Users/ada/Library/Caches/CLI Master")
        );
        assert_eq!(
            dirs.logs,
            std::path::Path::new("/Users/ada/Library/Logs/CLI Master")
        );
        assert!(
            dirs.runtime
                .to_string_lossy()
                .starts_with("/tmp/cli-master-")
        );
    }

    #[test]
    fn private_directory_is_created_with_mode_700() {
        let temporary = TempDir::new().expect("temporary directory should exist");
        let path = temporary.path().join("private");
        ensure_private_directory(&path).expect("directory should be created");
        let mode = std::fs::metadata(&path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700);
    }

    #[test]
    fn private_directory_rejects_a_symlink() {
        let temporary = TempDir::new().expect("temporary directory should exist");
        let target = temporary.path().join("target");
        let link = temporary.path().join("link");
        std::fs::create_dir(&target).expect("target");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");
        let error = ensure_private_directory(&link).expect_err("symlink must be rejected");
        assert!(matches!(
            error,
            crate::DaemonError::UntrustedDirectory { .. }
        ));
    }
}
