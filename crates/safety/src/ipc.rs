use std::path::Path;

use cli_master_core::{ApplicationError, ErrorCode};
use serde_json::Value;

use crate::paths::{ManagedRoots, assert_managed_worktree, is_within, resolve_path};

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

    validate_payload(method, payload, None, roots)?;

    Ok(())
}

fn validate_payload(
    method: &str,
    value: &Value,
    key: Option<&str>,
    roots: &ManagedRoots,
) -> Result<(), ApplicationError> {
    match value {
        Value::String(text) => {
            if text.contains('\0') {
                return Err(ApplicationError::new(
                    ErrorCode::InvalidIpcPayload,
                    "A request path contained a NUL byte.",
                )
                .not_recoverable()
                .with_action("Retry with a valid path."));
            }
            validate_string(method, key, text, roots)
        }
        Value::Array(items) => {
            for item in items {
                validate_payload(method, item, key, roots)?;
            }
            Ok(())
        }
        Value::Object(map) => {
            for (key, item) in map {
                if matches!(
                    normalize_payload_key(key).as_str(),
                    "command" | "commandstring" | "shell" | "shellcommand" | "cmd"
                ) {
                    return Err(ApplicationError::new(
                        ErrorCode::ShellInvocationRefused,
                        "The desktop client cannot submit a shell command string.",
                    )
                    .not_recoverable()
                    .with_action("Use the structured session and agent APIs."));
                }
                validate_payload(method, item, Some(key), roots)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn validate_string(
    method: &str,
    key: Option<&str>,
    text: &str,
    roots: &ManagedRoots,
) -> Result<(), ApplicationError> {
    let Some(key) = key.filter(|key| is_path_key(key)) else {
        return Ok(());
    };
    let path = Path::new(text);
    if path.as_os_str().is_empty() {
        return Err(invalid_ipc_path("A request path was empty.", text));
    }

    match method {
        "worktree.prepare_remove" | "worktree.remove" => {
            assert_managed_worktree(path, roots)
                .map_err(|_| invalid_ipc_path("A worktree removal path is not managed.", text))?;
        }
        "git.status" | "git.diff" | "worktree.list" => {
            if !is_known_project_or_worktree(path, roots)? {
                return Err(invalid_ipc_path(
                    "A Git request path is outside registered projects and managed worktrees.",
                    text,
                ));
            }
        }
        "project.remove" | "project.rename" => {
            let resolved = resolve_path(path)?.path;
            let is_registered = roots.project_roots.iter().any(|root| {
                resolve_path(root)
                    .map(|root| root.path == resolved)
                    .unwrap_or(false)
            });
            if !is_registered {
                return Err(invalid_ipc_path(
                    "A project mutation path is not a registered project root.",
                    text,
                ));
            }
        }
        "project.add" => {
            if !path.is_absolute() || contains_parent_component(path) {
                return Err(invalid_ipc_path(
                    "A project path must be absolute and cannot contain parent traversal.",
                    text,
                ));
            }
        }
        "worktree.create" => {
            if key.to_ascii_lowercase().contains("worktree") {
                assert_managed_worktree(path, roots)
                    .map_err(|_| invalid_ipc_path("A new worktree path is not managed.", text))?;
            } else if !is_known_project_or_worktree(path, roots)? {
                return Err(invalid_ipc_path(
                    "A worktree source path is outside registered projects.",
                    text,
                ));
            }
        }
        _ => {
            if contains_parent_component(path) {
                return Err(invalid_ipc_path(
                    "A request path contained parent traversal.",
                    text,
                ));
            }
        }
    }
    Ok(())
}

fn is_path_key(key: &str) -> bool {
    matches!(
        normalize_payload_key(key).as_str(),
        "path"
            | "root"
            | "cwd"
            | "repository"
            | "repositorypath"
            | "repopath"
            | "projectpath"
            | "worktreepath"
    )
}

fn normalize_payload_key(key: &str) -> String {
    key.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|character| character.to_ascii_lowercase())
        .collect()
}

fn is_known_project_or_worktree(
    path: &Path,
    roots: &ManagedRoots,
) -> Result<bool, ApplicationError> {
    if is_within(path, &roots.worktree_root)? {
        return Ok(true);
    }
    for root in &roots.project_roots {
        if is_within(path, root)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn contains_parent_component(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
}

fn invalid_ipc_path(message: &str, path: &str) -> ApplicationError {
    ApplicationError::new(ErrorCode::InvalidIpcPayload, message.to_owned())
        .not_recoverable()
        .with_action("Choose a project or worktree that CLI Master already knows.")
        .with_context("path", path)
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

    #[test]
    fn shell_command_field_is_rejected_for_custom_agents_too() {
        let temp = TempDir::new().expect("temp");
        let roots = ManagedRoots::new(temp.path());
        let error = validate_method_payload(
            "agent.custom.create",
            &json!({ "shell": "bash -c 'rm -rf /'" }),
            &roots,
        )
        .expect_err("shell field");
        assert_eq!(error.code(), ErrorCode::ShellInvocationRefused);
    }

    #[test]
    fn absolute_unmanaged_worktree_removal_is_rejected() {
        let temp = TempDir::new().expect("temp");
        let data = temp.path().join("data");
        std::fs::create_dir_all(data.join("worktrees")).expect("managed root");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&outside).expect("outside");
        let roots = ManagedRoots::new(data);
        let error = validate_method_payload("worktree.remove", &json!({ "path": outside }), &roots)
            .expect_err("unmanaged absolute path");
        assert_eq!(error.code(), ErrorCode::InvalidIpcPayload);
    }

    #[test]
    fn dots_inside_a_filename_are_not_parent_traversal() {
        let temp = TempDir::new().expect("temp");
        let project = temp.path().join("project..backup");
        std::fs::create_dir_all(&project).expect("project");
        let roots = ManagedRoots::new(temp.path().join("data"));
        validate_method_payload("project.add", &json!({ "path": project }), &roots)
            .expect("benign dots");
    }

    #[test]
    fn snake_case_path_keys_cannot_bypass_removal_validation() {
        let temp = TempDir::new().expect("temp");
        let data = temp.path().join("data");
        std::fs::create_dir_all(data.join("worktrees")).expect("managed root");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&outside).expect("outside");
        let roots = ManagedRoots::new(data);

        let error = validate_method_payload(
            "worktree.remove",
            &json!({ "worktree_path": outside }),
            &roots,
        )
        .expect_err("snake case path must be checked");
        assert_eq!(error.code(), ErrorCode::InvalidIpcPayload);
    }
}
