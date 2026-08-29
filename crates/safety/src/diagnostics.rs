use std::path::Path;

use cli_master_agents::{AgentAdapter, AgentRegistry, DetectionResult, LaunchEnvironment};
use cli_master_core::{
    AgentDiagnostics, ApplicationError, DaemonDiagnostics, DiagnosticsReport, ErrorCode,
    ExecutableDiagnostics, SqliteDiagnostics, VERSION_DETECT_TIMEOUT,
};
use cli_master_storage::Storage;

use crate::log::Logger;
use crate::platform::PlatformPaths;
use crate::process::{SpawnRequest, run_command_unchecked};

/// Collects a sanitized diagnostics snapshot.
///
/// The report never includes the process environment, tokens, prompts, or
/// terminal contents.
#[must_use]
pub fn collect_diagnostics() -> DiagnosticsReport {
    let paths = PlatformPaths::current();
    Logger::global().set_file_path(paths.log_file.clone());
    let environment = LaunchEnvironment::from_current_process_path();
    let registry = AgentRegistry::new();
    let git = git_version();

    DiagnosticsReport {
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        os: std::env::consts::OS.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
        data_dir: paths.data_dir.clone(),
        config_dir: paths.config_dir.clone(),
        runtime_dir: paths.runtime_dir.clone(),
        database_path: paths.database_path.clone(),
        log_dir: paths.log_dir.clone(),
        git_available: git.is_some(),
        git_version: git,
        daemon: DaemonDiagnostics {
            connected: false,
            instance_id: None,
            status: "No daemon is running. Session processes are not attached.".to_owned(),
        },
        sqlite: sqlite_status(&paths.database_path),
        agents: agent_diagnostics(&registry, &environment),
        executables: resolved_executables(&environment),
        session_count: 0,
        worktree_count: 0,
        recent_logs: Logger::global().recent_logs(),
        recent_errors: Logger::global().recent_errors(),
    }
}

fn agent_diagnostics(
    registry: &AgentRegistry,
    environment: &LaunchEnvironment,
) -> Vec<AgentDiagnostics> {
    registry
        .keys()
        .map(|key| {
            let adapter = registry.get(key);
            let detection =
                adapter.map_or(DetectionResult::NotFound, |item| item.detect(environment));
            AgentDiagnostics {
                key: key.to_owned(),
                display_name: adapter.map_or(key, AgentAdapter::display_name).to_owned(),
                detected: detection.is_found(),
                executable: detection.executable().map(Path::to_path_buf),
            }
        })
        .collect()
}

fn sqlite_status(path: &Path) -> SqliteDiagnostics {
    if !path.exists() {
        return SqliteDiagnostics {
            file_exists: false,
            available: false,
            schema_version: None,
            status: "Database file has not been created yet.".to_owned(),
        };
    }
    match Storage::open(path).and_then(|storage| storage.schema_version()) {
        Ok(version) => SqliteDiagnostics {
            file_exists: true,
            available: true,
            schema_version: Some(version),
            status: format!("SQLite is reachable at schema version {version}."),
        },
        Err(error) => {
            Logger::global().error(
                "storage",
                "diagnostics.sqlite",
                &ApplicationError::new(
                    ErrorCode::DatabaseUnavailable,
                    "SQLite could not be opened for diagnostics.",
                )
                .with_action("Check disk permissions for the CLI Master data directory.")
                .with_source(&error),
            );
            SqliteDiagnostics {
                file_exists: true,
                available: false,
                schema_version: None,
                status: "SQLite exists but could not be opened.".to_owned(),
            }
        }
    }
}

fn git_version() -> Option<String> {
    let output = run_command_unchecked(
        &SpawnRequest::new("git")
            .arg("--version")
            .timeout(VERSION_DETECT_TIMEOUT)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_OPTIONAL_LOCKS", "0"),
    )
    .ok()?;
    if !output.success() {
        return None;
    }
    Some(
        output
            .stdout_text()
            .trim_start_matches("git version")
            .trim()
            .to_owned(),
    )
}

fn resolved_executables(environment: &LaunchEnvironment) -> Vec<ExecutableDiagnostics> {
    ["git", "kill"]
        .into_iter()
        .map(|name| ExecutableDiagnostics {
            path: match environment.detect(name) {
                DetectionResult::Found { executable } => Some(executable),
                DetectionResult::NotFound | DetectionResult::NotExecutable { .. } => None,
            },
            name: name.to_owned(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::{LogLevel, StructuredLog};

    #[test]
    fn diagnostics_omit_environment_and_redact_recent_logs() {
        Logger::global().write(&StructuredLog::new(
            LogLevel::Info,
            "diagnostics",
            "diagnostics.get",
            "export TOKEN=super-secret-value",
        ));
        Logger::global().error(
            "diagnostics",
            "diagnostics.get",
            &ApplicationError::new(ErrorCode::InvalidPath, "Path traversal was refused.")
                .with_action("Choose a managed worktree.")
                .with_technical("COOKIE=not-for-export"),
        );

        let report = collect_diagnostics();
        let export = report.to_export_text();
        assert!(export.contains("0.1.0"));
        assert!(!export.contains("super-secret-value"));
        assert!(!export.contains("not-for-export"));
        assert!(!export.contains("\"PWD\""));
        assert!(
            report
                .recent_errors
                .iter()
                .any(|error| error.action.is_some())
        );
        assert!(report.os == "linux" || report.os == "macos");
        assert!(!report.arch.is_empty());
        assert!(report.agents.iter().any(|agent| agent.key == "codex"));
    }
}
