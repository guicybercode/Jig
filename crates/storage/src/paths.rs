use std::env;
use std::path::PathBuf;

use crate::error::{EntityKind, StorageError, StorageErrorKind};

/// Returns the platform data directory that owns `cli-master.db`.
///
/// Linux uses `$XDG_DATA_HOME/cli-master` when set, otherwise
/// `~/.local/share/cli-master`. macOS uses
/// `~/Library/Application Support/cli-master`.
///
/// # Errors
///
/// Returns an error when `HOME` is missing or the current OS is outside the
/// v0.1 support set.
pub fn default_data_dir() -> Result<PathBuf, StorageError> {
    #[cfg(target_os = "macos")]
    {
        Ok(home_dir()?.join("Library/Application Support/cli-master"))
    }
    #[cfg(target_os = "linux")]
    {
        if let Some(xdg) = env::var_os("XDG_DATA_HOME") {
            let xdg = PathBuf::from(xdg);
            if !xdg.as_os_str().is_empty() {
                return Ok(xdg.join("cli-master"));
            }
        }
        Ok(home_dir()?.join(".local/share/cli-master"))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Err(unsupported_platform())
    }
}

/// Returns the default file-backed database path for this user.
///
/// # Errors
///
/// Returns an error when the platform data directory cannot be resolved.
pub fn default_database_path() -> Result<PathBuf, StorageError> {
    Ok(default_data_dir()?.join("cli-master.db"))
}

fn home_dir() -> Result<PathBuf, StorageError> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| {
            StorageError::new(
                "resolve data dir",
                EntityKind::Database,
                StorageErrorKind::InvalidInput("HOME is not set"),
                "Set HOME and restart the daemon.",
            )
        })
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn unsupported_platform() -> StorageError {
    StorageError::new(
        "resolve data dir",
        EntityKind::Database,
        StorageErrorKind::InvalidInput("unsupported platform"),
        "CLI Master Beta v0.1 supports Linux and macOS only.",
    )
}

#[cfg(test)]
mod tests {
    use super::default_database_path;

    #[test]
    fn default_path_is_absolute_and_named() {
        let path = default_database_path().expect("default path should resolve");
        assert!(path.is_absolute());
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("cli-master.db")
        );
        let rendered = path.to_string_lossy();
        assert!(
            rendered.contains("cli-master"),
            "path should live under the application data directory: {rendered}"
        );
    }
}
