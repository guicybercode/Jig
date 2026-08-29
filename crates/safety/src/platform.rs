use std::env;
use std::path::{Path, PathBuf};

/// User-specific data, config, log, runtime, and database locations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformPaths {
    /// `$XDG_DATA_HOME/cli-master` or the macOS application-support equivalent.
    pub data_dir: PathBuf,
    /// `$XDG_CONFIG_HOME/cli-master`.
    pub config_dir: PathBuf,
    /// Log directory with user-only permissions.
    pub log_dir: PathBuf,
    /// Runtime directory for the daemon socket.
    pub runtime_dir: PathBuf,
    /// SQLite database path.
    pub database_path: PathBuf,
    /// Managed worktree root.
    pub worktree_root: PathBuf,
    /// Structured log file.
    pub log_file: PathBuf,
}

impl PlatformPaths {
    /// Resolves platform paths from the current process environment.
    #[must_use]
    pub fn current() -> Self {
        let home = env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from);
        let data_dir = data_home(&home).join("cli-master");
        let config_dir = config_home(&home).join("cli-master");
        let log_dir = if cfg!(target_os = "macos") {
            home.join("Library/Logs/cli-master")
        } else {
            cache_home(&home).join("cli-master/logs")
        };
        let runtime_dir = runtime_home(&home).join("cli-master");
        Self {
            database_path: data_dir.join("cli-master.db"),
            worktree_root: data_dir.join("worktrees"),
            log_file: log_dir.join("cli-master.jsonl"),
            data_dir,
            config_dir,
            log_dir,
            runtime_dir,
        }
    }
}

fn data_home(home: &Path) -> PathBuf {
    absolute_or(env::var_os("XDG_DATA_HOME"), || {
        if cfg!(target_os = "macos") {
            home.join("Library/Application Support")
        } else {
            home.join(".local/share")
        }
    })
}

fn config_home(home: &Path) -> PathBuf {
    absolute_or(env::var_os("XDG_CONFIG_HOME"), || {
        if cfg!(target_os = "macos") {
            home.join("Library/Application Support")
        } else {
            home.join(".config")
        }
    })
}

fn cache_home(home: &Path) -> PathBuf {
    absolute_or(env::var_os("XDG_CACHE_HOME"), || home.join(".cache"))
}

fn runtime_home(home: &Path) -> PathBuf {
    absolute_or(env::var_os("XDG_RUNTIME_DIR"), || {
        env::temp_dir().join(format!("cli-master-{}", user_token(home)))
    })
}

fn absolute_or(value: Option<std::ffi::OsString>, fallback: impl FnOnce() -> PathBuf) -> PathBuf {
    value
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(fallback)
}

fn user_token(home: &Path) -> String {
    env::var("USER").unwrap_or_else(|_| {
        home.file_name().map_or_else(
            || "user".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_path_stays_under_data_dir() {
        let paths = PlatformPaths::current();
        assert!(paths.database_path.starts_with(&paths.data_dir));
        assert!(paths.worktree_root.starts_with(&paths.data_dir));
        assert_eq!(
            paths
                .database_path
                .file_name()
                .and_then(|name| name.to_str()),
            Some("cli-master.db")
        );
    }
}
