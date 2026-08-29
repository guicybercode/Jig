use std::{
    env,
    ffi::{OsStr, OsString},
    fmt, fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use crate::{
    PathImportError,
    process::{PATH_IMPORT_OUTPUT_LIMIT, run_limited},
};
use rustix::fs::{Access, AtFlags, CWD, accessat};

const LOGIN_PATH_BEGIN: &str = "__CLI_MASTER_PATH_BEGIN__";
const LOGIN_PATH_END: &str = "__CLI_MASTER_PATH_END__";
/// Constant POSIX command. The shell expands `$PATH`; this crate never interpolates user input into it.
const LOGIN_PATH_COMMAND: &str = "printf '%s\\n' '__CLI_MASTER_PATH_BEGIN__'; printf '%s' \"$PATH\"; printf '\\n%s\\n' '__CLI_MASTER_PATH_END__'";

/// The explicit executable search path used for adapter detection.
///
/// GUI applications often inherit a truncated PATH. [`Self::desktop`] therefore
/// appends well-known user and package-manager directories that exist on disk.
/// Dotfiles are not sourced unless the user explicitly imports a login-shell PATH.
#[derive(Clone, Default, Eq, PartialEq)]
pub struct LaunchEnvironment {
    inherited: Vec<PathBuf>,
    extra: Vec<PathBuf>,
    include_standard: bool,
    standard_override: Option<Vec<PathBuf>>,
}

impl LaunchEnvironment {
    /// Creates an environment by parsing one platform `PATH` value.
    #[must_use]
    pub fn from_path(path: impl AsRef<OsStr>) -> Self {
        Self {
            inherited: env::split_paths(path.as_ref()).collect(),
            extra: Vec::new(),
            include_standard: false,
            standard_override: None,
        }
    }

    /// Creates an isolated environment from ordered search directories.
    ///
    /// Standard desktop fallbacks are not added. Tests should prefer this
    /// constructor so missing-agent cases do not observe a host `codex`.
    #[must_use]
    pub fn from_search_paths<I, P>(search_paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        Self {
            inherited: search_paths.into_iter().map(Into::into).collect(),
            extra: Vec::new(),
            include_standard: false,
            standard_override: None,
        }
    }

    /// Imports only the current process's `PATH`, or uses an empty search path
    /// if it is not set.
    #[must_use]
    pub fn from_current_process_path() -> Self {
        env::var_os("PATH").map_or_else(Self::default, Self::from_path)
    }

    /// Builds the search path used by a desktop app: inherited PATH plus
    /// standard user/package-manager directories that currently exist.
    #[must_use]
    pub fn desktop() -> Self {
        Self::from_current_process_path().with_standard_paths(true)
    }

    /// Appends user-configured search directories after the inherited PATH and
    /// before standard fallbacks.
    #[must_use]
    pub fn with_extra_paths<I, P>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        self.extra.extend(paths.into_iter().map(Into::into));
        self
    }

    /// Enables or disables well-known Linux/macOS executable directories.
    #[must_use]
    pub const fn with_standard_paths(mut self, enabled: bool) -> Self {
        self.include_standard = enabled;
        self
    }

    /// Replaces the platform standard directories. Intended for tests.
    #[must_use]
    pub fn with_standard_path_override<I, P>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        self.include_standard = true;
        self.standard_override = Some(paths.into_iter().map(Into::into).collect());
        self
    }

    /// Returns inherited PATH entries before extra and standard directories.
    #[must_use]
    pub fn inherited_paths(&self) -> &[PathBuf] {
        &self.inherited
    }

    /// Returns user-configured extra search directories.
    #[must_use]
    pub fn extra_paths(&self) -> &[PathBuf] {
        &self.extra
    }

    /// Returns the ordered executable search directories actually used.
    #[must_use]
    pub fn search_paths(&self) -> Vec<PathBuf> {
        merge_unique(
            self.inherited
                .iter()
                .chain(self.extra.iter())
                .cloned()
                .chain(
                    self.standard_directories()
                        .into_iter()
                        .filter(|path| path.is_dir()),
                ),
        )
    }

    /// Serializes the effective directories as a platform `PATH` value.
    ///
    /// # Errors
    ///
    /// Returns [`env::JoinPathsError`] if a directory contains the platform
    /// path separator.
    pub fn joined_path(&self) -> Result<OsString, env::JoinPathsError> {
        env::join_paths(self.search_paths())
    }

    /// Describes how the effective PATH was assembled without dumping the
    /// process environment.
    #[must_use]
    pub fn path_diagnostics(&self) -> PathDiagnostics {
        let standard = self.standard_directories();
        let mut standard_present = Vec::new();
        let mut standard_absent = Vec::new();
        for path in &standard {
            if path.is_dir() {
                standard_present.push(path.clone());
            } else {
                standard_absent.push(path.clone());
            }
        }

        let mut notes = vec![
            "Inherited PATH is used first so a terminal-launched daemon keeps the user's order.".to_owned(),
            "User-configured extra directories are searched next.".to_owned(),
            "Standard fallbacks (Homebrew, ~/.local/bin, cargo, pnpm, mise, asdf) are appended only when those directories exist.".to_owned(),
            "Dotfiles are not sourced unless login-shell PATH import is requested explicitly.".to_owned(),
        ];
        if self.include_standard {
            notes.push("Standard desktop fallbacks are enabled.".to_owned());
        } else {
            notes.push("Standard desktop fallbacks are disabled for this environment.".to_owned());
        }

        PathDiagnostics {
            inherited: self.inherited.clone(),
            extra: self.extra.clone(),
            standard_present,
            standard_absent,
            effective: self.search_paths(),
            notes,
        }
    }

    /// Resolves an absolute executable path or a bare name against this
    /// environment without invoking a shell. A successful result contains a
    /// canonical absolute path, so changing the process working directory
    /// cannot change which file a later launch addresses.
    #[must_use]
    pub fn detect(&self, executable: impl AsRef<Path>) -> DetectionResult {
        let executable = executable.as_ref();
        if executable.as_os_str().is_empty() {
            return DetectionResult::NotFound;
        }
        if executable.is_absolute() {
            return inspect_candidate(executable.to_path_buf());
        }

        // Relative paths containing a directory component are deliberately not
        // interpreted relative to the daemon. Custom definitions accept either
        // an absolute path, a leading `~/`, or a bare PATH-resolved name.
        if executable.components().count() != 1 {
            return DetectionResult::NotFound;
        }

        let mut first_non_executable = None;
        for directory in self.search_paths() {
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

    fn standard_directories(&self) -> Vec<PathBuf> {
        if !self.include_standard {
            return Vec::new();
        }
        self.standard_override
            .clone()
            .unwrap_or_else(standard_search_directories)
    }
}

impl fmt::Debug for LaunchEnvironment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LaunchEnvironment")
            .field("inherited", &self.inherited)
            .field("extra", &self.extra)
            .field("include_standard", &self.include_standard)
            .field("standard_override", &self.standard_override)
            .field("effective", &self.search_paths())
            .finish()
    }
}

/// Safe description of executable search path assembly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathDiagnostics {
    /// Directories from the process `PATH`.
    pub inherited: Vec<PathBuf>,
    /// User-configured extra directories.
    pub extra: Vec<PathBuf>,
    /// Standard fallbacks that currently exist.
    pub standard_present: Vec<PathBuf>,
    /// Standard fallbacks that are documented but absent on this machine.
    pub standard_absent: Vec<PathBuf>,
    /// Deduplicated directories used for detection.
    pub effective: Vec<PathBuf>,
    /// Human-readable explanation of the search policy.
    pub notes: Vec<String>,
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

/// Expands a leading `~/` using `HOME`. Other `~user` forms are rejected.
///
/// # Errors
///
/// Returns [`crate::AgentError::HomeDirectoryUnavailable`] when `HOME` is unset
/// and the path starts with `~`.
pub fn expand_leading_tilde(path: &str) -> Result<String, crate::AgentError> {
    if path == "~" || path.starts_with("~/") {
        let home = env::var("HOME").map_err(|_| crate::AgentError::HomeDirectoryUnavailable)?;
        if path == "~" {
            return Ok(home);
        }
        Ok(format!("{home}/{}", &path[2..]))
    } else if path.starts_with('~') {
        Err(crate::AgentError::UnsupportedTildeExpansion)
    } else {
        Ok(path.to_owned())
    }
}

/// Reads PATH from the user's login shell using a constant command.
///
/// This executes shell startup files. The full environment is discarded; only
/// the marked PATH payload is returned. Callers should preview the result
/// before persisting it.
///
/// # Errors
///
/// Returns an error if no shell is available, the process times out, or stdout
/// does not contain the expected markers.
pub fn read_login_shell_path(timeout: Duration) -> Result<OsString, PathImportError> {
    let shell = login_shell_executable().ok_or(PathImportError::ShellNotFound)?;
    let mut command = Command::new(shell);
    command.args(["-lc", LOGIN_PATH_COMMAND]);
    let output = run_limited(command, timeout, PATH_IMPORT_OUTPUT_LIMIT)
        .map_err(|_| PathImportError::SpawnFailed)?;
    if output.timed_out {
        return Err(PathImportError::Timeout);
    }
    if output.exit_code != Some(0) {
        return Err(PathImportError::Unsuccessful {
            exit_code: output.exit_code,
        });
    }
    parse_marked_path(&output.stdout)
}

/// Well-known executable directories for desktop-launched apps.
#[must_use]
pub fn standard_search_directories() -> Vec<PathBuf> {
    standard_search_directories_for(
        env::var_os("HOME").map(PathBuf::from).as_deref(),
        env::var_os("XDG_DATA_HOME").map(PathBuf::from).as_deref(),
        env::var_os("PNPM_HOME").map(PathBuf::from).as_deref(),
        env::var_os("CARGO_HOME").map(PathBuf::from).as_deref(),
    )
}

fn standard_search_directories_for(
    home: Option<&Path>,
    xdg_data_home: Option<&Path>,
    pnpm_home: Option<&Path>,
    cargo_home: Option<&Path>,
) -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if let Some(home) = home {
        directories.extend([
            home.join(".local/bin"),
            home.join("bin"),
            home.join(".cargo/bin"),
            home.join(".npm-global/bin"),
            home.join(".local/share/pnpm"),
            home.join("Library/pnpm"),
            home.join(".local/share/mise/shims"),
            home.join(".mise/shims"),
            home.join(".asdf/shims"),
        ]);
    }
    if let Some(xdg) = xdg_data_home {
        directories.push(xdg.join("mise/shims"));
        directories.push(xdg.join("pnpm"));
    }
    if let Some(pnpm_home) = pnpm_home {
        directories.push(pnpm_home.to_path_buf());
    }
    if let Some(cargo_home) = cargo_home {
        directories.push(cargo_home.join("bin"));
    }
    directories.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/opt/homebrew/sbin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/local/sbin"),
        PathBuf::from("/home/linuxbrew/.linuxbrew/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
    ]);
    merge_unique(directories)
}

fn login_shell_executable() -> Option<PathBuf> {
    if let Some(shell) = env::var_os("SHELL") {
        let path = PathBuf::from(shell);
        if !path.as_os_str().is_empty() {
            return Some(path);
        }
    }
    for candidate in ["/bin/bash", "/bin/zsh"] {
        let path = PathBuf::from(candidate);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

fn parse_marked_path(stdout: &[u8]) -> Result<OsString, PathImportError> {
    let text = String::from_utf8_lossy(stdout);
    let begin = text
        .find(LOGIN_PATH_BEGIN)
        .ok_or(PathImportError::MarkersMissing)?;
    let after_begin = begin + LOGIN_PATH_BEGIN.len();
    let rest = text[after_begin..].trim_start_matches(['\r', '\n']);
    let end = rest
        .find(LOGIN_PATH_END)
        .ok_or(PathImportError::MarkersMissing)?;
    let path = rest[..end].trim_end_matches(['\r', '\n']);
    Ok(OsString::from(path))
}

fn merge_unique(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut unique = Vec::new();
    for path in paths {
        if path.as_os_str().is_empty() {
            continue;
        }
        if !unique.contains(&path) {
            unique.push(path);
        }
    }
    unique
}

fn inspect_candidate(candidate: PathBuf) -> DetectionResult {
    let Ok(resolved) = fs::canonicalize(&candidate) else {
        return DetectionResult::NotFound;
    };
    let Ok(metadata) = fs::metadata(&resolved) else {
        return DetectionResult::NotFound;
    };

    if !metadata.is_file() || accessat(CWD, &resolved, Access::EXEC_OK, AtFlags::EACCESS).is_err() {
        return DetectionResult::NotExecutable { candidate };
    }

    DetectionResult::Found {
        executable: resolved,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_directories_include_homebrew_and_user_bin() {
        let dirs = standard_search_directories_for(
            Some(Path::new("/Users/ada")),
            None,
            Some(Path::new("/Users/ada/Library/pnpm")),
            Some(Path::new("/Users/ada/.cargo")),
        );
        assert!(dirs.contains(&PathBuf::from("/Users/ada/.local/bin")));
        assert!(dirs.contains(&PathBuf::from("/Users/ada/.cargo/bin")));
        assert!(dirs.contains(&PathBuf::from("/opt/homebrew/bin")));
        assert!(dirs.contains(&PathBuf::from("/Users/ada/Library/pnpm")));
    }

    #[test]
    fn extra_paths_are_searched_before_standard_overrides() {
        let first = PathBuf::from("/tmp/first");
        let extra = PathBuf::from("/tmp/extra");
        let standard = PathBuf::from("/tmp/standard");
        let environment = LaunchEnvironment::from_search_paths([first.clone()])
            .with_extra_paths([extra.clone()])
            .with_standard_path_override([standard.clone()]);
        let diagnostics = environment.path_diagnostics();
        assert_eq!(diagnostics.inherited.as_slice(), [first.as_path()]);
        assert_eq!(diagnostics.extra.as_slice(), [extra.as_path()]);
        let effective = diagnostics.effective;
        let first_idx = effective.iter().position(|path| path == &first);
        let extra_idx = effective.iter().position(|path| path == &extra);
        let standard_idx = effective.iter().position(|path| path == &standard);
        if let (Some(first_idx), Some(extra_idx)) = (first_idx, extra_idx) {
            assert!(first_idx < extra_idx);
        }
        if let (Some(extra_idx), Some(standard_idx)) = (extra_idx, standard_idx) {
            assert!(extra_idx < standard_idx);
        }
    }

    #[test]
    fn parse_marked_path_extracts_only_the_payload() {
        let stdout =
            format!("noise\n{LOGIN_PATH_BEGIN}\n/opt/homebrew/bin:/usr/bin\n{LOGIN_PATH_END}\n");
        let path = parse_marked_path(stdout.as_bytes()).expect("markers should parse");
        assert_eq!(path, OsString::from("/opt/homebrew/bin:/usr/bin"));
    }
}
