use std::path::PathBuf;

use cli_master_agents::{AgentRegistry, DetectionResult, LaunchEnvironment};
use cli_master_core::{APPLICATION_VERSION, PROTOCOL_V1};
use cli_master_git::Git;
use serde::{Deserialize, Serialize};

use crate::{DaemonConfig, DaemonError};

/// JSON report used by `cli-masterd --preflight`.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightReport {
    /// Application semantic version, matching Cargo, npm, and Tauri.
    pub application_version: String,
    /// IPC protocol version spoken by this binary.
    pub protocol_version: u16,
    /// False when a required check failed.
    pub ok: bool,
    /// Private directories that were created or verified.
    pub directories: Vec<DirectoryStatus>,
    /// Required system Git executable.
    pub git: DependencyStatus,
    /// Optional built-in agent CLIs. Missing agents do not fail preflight.
    pub agents: Vec<DependencyStatus>,
}

/// One private directory after the packaging permission check.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryStatus {
    /// Stable directory role.
    pub kind: String,
    /// Absolute or test path.
    pub path: PathBuf,
    /// Whether the directory is present, user-owned, and mode `0700`.
    pub ok: bool,
    /// Observed permission bits when the directory exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<u32>,
    /// Safe failure explanation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Presence of Git or an optional agent CLI.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyStatus {
    /// Adapter key or `git`.
    pub name: String,
    /// Whether packaging treats this executable as required.
    pub required: bool,
    /// Whether a usable executable was resolved.
    pub available: bool,
    /// Resolved executable path when detection succeeded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// Version string or a safe reason the check failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Suggested action when the check failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

/// Discovers platform directories, secures them, and checks Git plus optional CLIs.
///
/// # Errors
///
/// Returns [`DaemonError::MissingHomeDirectory`] when platform directories cannot
/// be resolved. Individual check failures are recorded in the report instead.
pub fn run() -> Result<PreflightReport, DaemonError> {
    let config = DaemonConfig::discover()?;
    Ok(collect(&config))
}

pub(crate) fn collect(config: &DaemonConfig) -> PreflightReport {
    let directories = inspect_directories(config);
    let git = inspect_git();
    let agents = inspect_agents();
    let ok = directories.iter().all(|directory| directory.ok) && git.available;
    PreflightReport {
        application_version: APPLICATION_VERSION.to_owned(),
        protocol_version: PROTOCOL_V1,
        ok,
        directories,
        git,
        agents,
    }
}

fn inspect_directories(config: &DaemonConfig) -> Vec<DirectoryStatus> {
    [
        ("data", config.data_directory()),
        ("config", config.config_directory()),
        ("cache", config.cache_directory()),
        ("log", config.log_directory()),
        ("runtime", config.runtime_directory()),
    ]
    .into_iter()
    .map(
        |(kind, path)| match crate::paths::ensure_private_directory(path) {
            Ok(()) => DirectoryStatus {
                kind: kind.to_owned(),
                path: path.to_path_buf(),
                ok: true,
                mode: directory_mode(path),
                error: None,
            },
            Err(error) => DirectoryStatus {
                kind: kind.to_owned(),
                path: path.to_path_buf(),
                ok: false,
                mode: directory_mode(path),
                error: Some(error.to_string()),
            },
        },
    )
    .collect()
}

fn directory_mode(path: &std::path::Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions().mode() & 0o777)
}

fn inspect_git() -> DependencyStatus {
    match Git::discover() {
        Ok(git) => {
            let version = git.version().ok();
            DependencyStatus {
                name: "git".to_owned(),
                required: true,
                available: true,
                path: Some(git.executable().to_path_buf()),
                detail: version,
                action: None,
            }
        }
        Err(error) => DependencyStatus {
            name: "git".to_owned(),
            required: true,
            available: false,
            path: error.path().map(PathBuf::from),
            detail: Some(error.message().to_owned()),
            action: Some(error.action().to_owned()),
        },
    }
}

fn inspect_agents() -> Vec<DependencyStatus> {
    let environment = LaunchEnvironment::from_current_process_path();
    let registry = AgentRegistry::new();
    registry
        .keys()
        .map(|key| {
            let adapter = registry
                .get(key)
                .expect("registry keys come from registered adapters");
            match adapter.detect(&environment) {
                DetectionResult::Found { executable } => DependencyStatus {
                    name: key.to_owned(),
                    required: false,
                    available: true,
                    path: Some(executable),
                    detail: None,
                    action: None,
                },
                DetectionResult::NotFound => DependencyStatus {
                    name: key.to_owned(),
                    required: false,
                    available: false,
                    path: None,
                    detail: Some("executable was not found on PATH".to_owned()),
                    action: Some(format!(
                        "Install {key} if you want to start sessions with that adapter"
                    )),
                },
                DetectionResult::NotExecutable { candidate } => DependencyStatus {
                    name: key.to_owned(),
                    required: false,
                    available: false,
                    path: Some(candidate),
                    detail: Some("path exists but is not executable".to_owned()),
                    action: Some(
                        "Fix the executable bit or point the adapter at a binary".to_owned(),
                    ),
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::collect;
    use crate::DaemonConfig;

    #[test]
    fn preflight_requires_git_and_treats_agents_as_optional() {
        let temporary = tempfile::TempDir::new().expect("temporary directory should exist");
        let config =
            DaemonConfig::from_paths(temporary.path().join("data"), temporary.path().join("run"));
        let report = collect(&config);

        assert!(report.git.available, "CI and developer machines have Git");
        assert!(report.ok);
        assert_eq!(report.protocol_version, cli_master_core::PROTOCOL_V1);
        assert_eq!(
            report.application_version,
            cli_master_core::APPLICATION_VERSION
        );
        assert!(report.directories.iter().all(|directory| directory.ok));
        assert_eq!(report.agents.len(), 4);
        for agent in &report.agents {
            assert!(!agent.required);
        }
    }
}
