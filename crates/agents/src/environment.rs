use std::{
    env,
    ffi::{OsStr, OsString},
    fmt, fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

/// The explicit executable search path used for adapter detection.
///
/// Only `PATH` is imported by [`Self::from_current_process_path`]. Other
/// process environment variables, including credentials, are never captured.
#[derive(Clone, Default, Eq, PartialEq)]
pub struct LaunchEnvironment {
    search_paths: Vec<PathBuf>,
}

impl LaunchEnvironment {
    /// Creates an environment by parsing one platform `PATH` value.
    #[must_use]
    pub fn from_path(path: impl AsRef<OsStr>) -> Self {
        Self {
            search_paths: env::split_paths(path.as_ref()).collect(),
        }
    }

    /// Creates an environment from ordered search directories.
    #[must_use]
    pub fn from_search_paths<I, P>(search_paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        Self {
            search_paths: search_paths.into_iter().map(Into::into).collect(),
        }
    }

    /// Imports only the current process's `PATH`, or uses an empty search path
    /// if it is not set.
    #[must_use]
    pub fn from_current_process_path() -> Self {
        env::var_os("PATH").map_or_else(Self::default, Self::from_path)
    }

    /// Returns the ordered executable search directories.
    #[must_use]
    pub fn search_paths(&self) -> &[PathBuf] {
        &self.search_paths
    }

    /// Serializes the configured directories as a platform `PATH` value.
    ///
    /// # Errors
    ///
    /// Returns [`env::JoinPathsError`] if a directory contains the platform
    /// path separator.
    pub fn joined_path(&self) -> Result<OsString, env::JoinPathsError> {
        env::join_paths(&self.search_paths)
    }

    /// Resolves an absolute executable path or a bare name against this
    /// environment without invoking a shell.
    #[must_use]
    pub fn detect(&self, executable: impl AsRef<Path>) -> DetectionResult {
        let executable = executable.as_ref();
        if executable.is_absolute() {
            return inspect_candidate(executable.to_path_buf());
        }

        // Relative paths containing a directory component are deliberately not
        // interpreted relative to the daemon. Custom definitions accept either
        // an absolute path or a bare PATH-resolved executable name.
        if executable.components().count() != 1 || executable.as_os_str().is_empty() {
            return DetectionResult::NotFound;
        }

        let mut first_non_executable = None;
        for directory in &self.search_paths {
            let candidate = directory.join(executable);
            match inspect_candidate(candidate) {
                found @ DetectionResult::Found { .. } => return found,
                DetectionResult::NotExecutable { candidate } => {
                    first_non_executable.get_or_insert(candidate);
                }
                DetectionResult::NotFound => {}
            }
        }

        first_non_executable.map_or(DetectionResult::NotFound, |candidate| {
            DetectionResult::NotExecutable { candidate }
        })
    }
}

impl fmt::Debug for LaunchEnvironment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LaunchEnvironment")
            .field("search_paths", &self.search_paths)
            .finish()
    }
}

/// Result of checking one adapter executable without starting it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DetectionResult {
    /// An executable regular file was found.
    Found {
        /// Resolved executable path.
        executable: PathBuf,
    },
    /// No candidate exists in the explicit search path.
    NotFound,
    /// A candidate exists but is not an executable regular file.
    NotExecutable {
        /// First non-executable candidate in search order.
        candidate: PathBuf,
    },
}

impl DetectionResult {
    /// Returns whether the executable is ready to launch.
    #[must_use]
    pub const fn is_found(&self) -> bool {
        matches!(self, Self::Found { .. })
    }

    /// Returns the resolved executable when detection succeeded.
    #[must_use]
    pub fn executable(&self) -> Option<&Path> {
        match self {
            Self::Found { executable } => Some(executable),
            Self::NotFound | Self::NotExecutable { .. } => None,
        }
    }
}

fn inspect_candidate(candidate: PathBuf) -> DetectionResult {
    match fs::metadata(&candidate) {
        Ok(metadata) if metadata.is_file() && metadata.permissions().mode() & 0o111 != 0 => {
            DetectionResult::Found {
                executable: candidate,
            }
        }
        Ok(_) => DetectionResult::NotExecutable { candidate },
        Err(_) => DetectionResult::NotFound,
    }
}
