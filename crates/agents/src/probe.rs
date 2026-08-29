use std::{
    fmt,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use serde::Serialize;

use crate::{
    DetectionResult, LaunchEnvironment,
    process::{DEFAULT_PROBE_OUTPUT_LIMIT, DEFAULT_PROBE_TIMEOUT, run_limited},
};

/// Options for a non-destructive executable probe.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProbeOptions {
    timeout: Duration,
    max_output_bytes: usize,
    version_argument: Option<&'static str>,
}

impl Default for ProbeOptions {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_PROBE_TIMEOUT,
            max_output_bytes: DEFAULT_PROBE_OUTPUT_LIMIT,
            version_argument: Some("--version"),
        }
    }
}

impl ProbeOptions {
    /// Disables the `--version` child process. Resolution still runs.
    #[must_use]
    pub const fn without_version_probe(mut self) -> Self {
        self.version_argument = None;
        self
    }

    /// Overrides the probe timeout. Tests use a short value for hang fixtures.
    #[must_use]
    pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Overrides captured output size.
    #[must_use]
    pub const fn with_max_output_bytes(mut self, max_output_bytes: usize) -> Self {
        self.max_output_bytes = max_output_bytes;
        self
    }

    /// Returns the timeout.
    #[must_use]
    pub const fn timeout(&self) -> Duration {
        self.timeout
    }
}

/// Outcome of a safe executable test used by diagnostics and custom-agent setup.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutableTestReport {
    /// Whether a regular executable file was resolved.
    pub installed: bool,
    /// Absolute path when resolution succeeded or a non-executable candidate exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_path: Option<PathBuf>,
    /// First-line version preview when a version probe ran successfully.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Whether the binary could be resolved and, if requested, probed.
    pub launch_test: LaunchTestStatus,
    /// Safe, user-facing warning. Never includes environment values.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// Structured launch-test result shown in the diagnostics UI.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum LaunchTestStatus {
    /// The executable was resolved. A version probe, if requested, did not hang.
    Success,
    /// No candidate was found in the search path.
    NotFound,
    /// A candidate exists but cannot be executed.
    NotExecutable {
        /// Path that failed the executable check.
        candidate: PathBuf,
    },
    /// The version probe did not exit before the timeout.
    Timeout,
    /// The process could not be started.
    Failed {
        /// Safe explanation that does not include captured output.
        message: String,
    },
}

impl fmt::Display for LaunchTestStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Success => formatter.write_str("success"),
            Self::NotFound => formatter.write_str("error: executable not found"),
            Self::NotExecutable { candidate } => {
                write!(formatter, "error: not executable ({})", candidate.display())
            }
            Self::Timeout => formatter.write_str("error: version probe timed out"),
            Self::Failed { message } => write!(formatter, "error: {message}"),
        }
    }
}

/// Resolves `executable` and optionally runs a bounded `--version` probe.
///
/// Stdin is closed. No prompt is written. Output is truncated and only a short
/// version line is retained. The child is killed if it exceeds the timeout.
///
/// # Errors
///
/// This function always returns a report. It does not panic on missing files.
#[must_use]
pub fn test_executable(
    executable: impl AsRef<Path>,
    environment: &LaunchEnvironment,
    options: ProbeOptions,
) -> ExecutableTestReport {
    match environment.detect(executable.as_ref()) {
        DetectionResult::NotFound => ExecutableTestReport {
            installed: false,
            resolved_path: None,
            version: None,
            launch_test: LaunchTestStatus::NotFound,
            warning: Some(
                "Install the CLI or add its directory to the executable search path.".to_owned(),
            ),
        },
        DetectionResult::NotExecutable { candidate } => ExecutableTestReport {
            installed: false,
            resolved_path: Some(candidate.clone()),
            version: None,
            launch_test: LaunchTestStatus::NotExecutable { candidate },
            warning: Some("The candidate exists but is not an executable regular file.".to_owned()),
        },
        DetectionResult::Found { executable } => probe_resolved_executable(&executable, options),
    }
}

fn probe_resolved_executable(executable: &Path, options: ProbeOptions) -> ExecutableTestReport {
    let Some(version_argument) = options.version_argument else {
        return ExecutableTestReport {
            installed: true,
            resolved_path: Some(executable.to_path_buf()),
            version: None,
            launch_test: LaunchTestStatus::Success,
            warning: None,
        };
    };

    let mut command = Command::new(executable);
    command.arg(version_argument);
    command.env("PAGER", "cat");
    command.env("GIT_PAGER", "cat");
    command.env("NO_COLOR", "1");

    match run_limited(command, options.timeout, options.max_output_bytes) {
        Ok(output) if output.timed_out => ExecutableTestReport {
            installed: true,
            resolved_path: Some(executable.to_path_buf()),
            version: None,
            launch_test: LaunchTestStatus::Timeout,
            warning: Some(
                "The executable did not exit from --version before the timeout; it was killed."
                    .to_owned(),
            ),
        },
        Ok(output) => {
            let version =
                first_version_line(&output.stdout).or_else(|| first_version_line(&output.stderr));
            let warning = if version.is_none() {
                Some(
                    "The executable is installed but did not print a usable --version line."
                        .to_owned(),
                )
            } else {
                None
            };
            ExecutableTestReport {
                installed: true,
                resolved_path: Some(executable.to_path_buf()),
                version,
                launch_test: LaunchTestStatus::Success,
                warning,
            }
        }
        Err(_) => ExecutableTestReport {
            installed: true,
            resolved_path: Some(executable.to_path_buf()),
            version: None,
            launch_test: LaunchTestStatus::Failed {
                message: "the executable could not be started for a version probe".to_owned(),
            },
            warning: Some("The file is executable but could not be spawned.".to_owned()),
        },
    }
}

fn first_version_line(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    let mut preview: String = line.chars().take(200).collect();
    if preview.contains('\0') {
        return None;
    }
    if line.chars().count() > 200 {
        preview.push('…');
    }
    Some(preview)
}
