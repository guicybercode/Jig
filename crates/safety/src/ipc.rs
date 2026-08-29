use std::path::Path;

use cli_master_core::{ApplicationError, ErrorCode};
use serde_json::Value;

use crate::paths::{ManagedRoots, resolve_path};

/// Known v0.1 methods. Unknown methods are rejected before any I/O.
const KNOWN_METHODS: &[&str] = &[
    "system.hello",
    "state.snapshot",
    "project.add",
    "project.list",
    "project.rename",
    "project.remove",
    "agent.list",
    "agent.detect",
    "agent.custom.create",
    "agent.custom.update",
    "agent.custom.remove",
    "session.create",
    "session.list",
    "session.get",
    "session.subscribe",
    "session.unsubscribe",
    "session.write",
    "session.resize",
    "session.stop",
    "session.kill",
    "session.restart",
    "session.rename",
    "session.delete",
    "git.status",
    "git.diff",
    "worktree.create",
    "worktree.list",
    "worktree.prepare_remove",
    "worktree.remove",
    "diagnostics.get",
    "diagnostics.export",
];

/// Validates an IPC method and rejects path traversal in payload strings.
///
/// This is a defense against a compromised frontend sending unexpected methods
/// or `../` paths. It is not a sandbox for the agent CLIs themselves.
///
/// # Errors
///
/// Returns [`ErrorCode::InvalidIpcPayload`] for unknown methods or unsafe paths.
pub fn validate_method_payload(
    method: &str,
    payload: &Value,
    roots: &ManagedRoots,
) -> Result<(), ApplicationError> {
    if method.is_empty() || !KNOWN_METHODS.contains(&method) {
        return Err(ApplicationError::new(
            ErrorCode::InvalidIpcPayload,
            "The desktop client sent an unknown request.",
        )
        .not_recoverable()
        .with_action("Update CLI Master. If this persists, the UI may be compromised.")
        .with_context("method", method));
    }

    if method.starts_with("worktree.")
        || method.starts_with("project.")
        || method == "git.status"
        || method == "git.diff"
    {
        validate_payload_paths(payload, roots)?;
    }

    Ok(())
}

fn validate_payload_paths(value: &Value, roots: &ManagedRoots) -> Result<(), ApplicationError> {
    match value {
        Value::String(text) if looks_like_path(text) => {
            if text.contains('\0') {
                return Err(ApplicationError::new(
                    ErrorCode::InvalidIpcPayload,
                    "A request path contained a NUL byte.",
                )
                .not_recoverable()
                .with_action("Retry with a valid path."));
            }
            let resolved = resolve_path(Path::new(text))?;
            if text.contains("..")
                && !crate::paths::is_within(&resolved.path, &roots.data_dir)?
                && !roots
                    .project_roots
                    .iter()
                    .any(|root| crate::paths::is_within(&resolved.path, root).unwrap_or(false))
            {
                return Err(ApplicationError::new(
                    ErrorCode::InvalidPath,
                    "A request path escaped the managed directories.",
                )
                .not_recoverable()
                .with_action("Choose a project or worktree that CLI Master already knows.")
                .with_context("path", resolved.path.display().to_string()));
            }
            Ok(())
        }
        Value::Array(items) => {
            for item in items {
                validate_payload_paths(item, roots)?;
            }
            Ok(())
        }
        Value::Object(map) => {
            for (key, item) in map {
                if key.eq_ignore_ascii_case("command") || key.eq_ignore_ascii_case("shell") {
                    return Err(ApplicationError::new(
                        ErrorCode::ShellInvocationRefused,
                        "The desktop client cannot submit a shell command string.",
                    )
                    .not_recoverable()
                    .with_action("Use the structured session and agent APIs."));
                }
                validate_payload_paths(item, roots)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn looks_like_path(text: &str) -> bool {
    text.starts_with('/') || text.contains('/') || text.contains("..")
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn unknown_method_is_rejected() {
        let temp = TempDir::new().expect("temp");
        let roots = ManagedRoots::new(temp.path());
        let error =
            validate_method_payload("system.eval", &json!({}), &roots).expect_err("unknown method");
        assert_eq!(error.code(), ErrorCode::InvalidIpcPayload);
    }

    #[test]
    fn shell_command_field_is_rejected() {
        let temp = TempDir::new().expect("temp");
        let roots = ManagedRoots::new(temp.path());
        let error =
            validate_method_payload("project.add", &json!({ "command": "rm -rf /" }), &roots)
                .expect_err("shell field");
        assert_eq!(error.code(), ErrorCode::ShellInvocationRefused);
    }
}
