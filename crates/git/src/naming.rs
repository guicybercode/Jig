use std::path::Path;

use crate::{Git, GitError, GitErrorKind, os, repository};

const MAX_SLUG_BYTES: usize = 48;
const MAX_COLLISIONS: usize = 10_000;

/// Converts a task label into a non-empty, lowercase ASCII slug.
///
/// Non-ASCII characters are treated as separators. Runs of punctuation or
/// whitespace collapse to one `-`, and output is capped at 48 bytes.
#[must_use]
pub fn slugify(value: &str) -> String {
    let mut result = String::new();
    let mut separator_pending = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            if separator_pending && !result.is_empty() && result.len() < MAX_SLUG_BYTES {
                result.push('-');
            }
            separator_pending = false;
            if result.len() < MAX_SLUG_BYTES {
                result.push(character.to_ascii_lowercase());
            }
        } else if !result.is_empty() {
            separator_pending = true;
        }
        if result.len() >= MAX_SLUG_BYTES {
            break;
        }
    }
    while result.ends_with('-') {
        result.pop();
    }
    if result.is_empty() {
        "task".to_owned()
    } else {
        result
    }
}

pub(crate) fn generate_branch_name(
    git: &Git,
    repository: &Path,
    task_name: &str,
    short_id: &str,
) -> Result<String, GitError> {
    let root = repository::require_root(git, repository)?;
    let slug = slugify(task_name);
    let id = slugify(short_id).replace('-', "");
    let id = id.get(..id.len().min(12)).unwrap_or(&id);
    let base = format!("agent/{slug}-{id}");

    for collision in 1..=MAX_COLLISIONS {
        let candidate = if collision == 1 {
            base.clone()
        } else {
            format!("{base}-{collision}")
        };
        validate_branch(git, &root, &candidate)?;
        if !branch_exists(git, &root, &candidate)? {
            return Ok(candidate);
        }
    }
    Err(GitError::new(
        GitErrorKind::InvalidInput,
        "Could not allocate a unique agent branch name",
        "Use a different task name or session identifier",
    ))
}

pub(crate) fn validate_branch(git: &Git, root: &Path, branch: &str) -> Result<(), GitError> {
    let output = git.execute(
        Some(root),
        [os("check-ref-format"), os("--branch"), os(branch)],
        64 * 1024,
    )?;
    if output.success() {
        Ok(())
    } else {
        Err(GitError::new(
            GitErrorKind::InvalidInput,
            format!("Generated branch name is invalid: {branch}"),
            "Use an ASCII task name and identifier",
        ))
    }
}

pub(crate) fn branch_exists(git: &Git, root: &Path, branch: &str) -> Result<bool, GitError> {
    let reference = format!("refs/heads/{branch}");
    let output = git.execute(
        Some(root),
        [os("show-ref"), os("--verify"), os("--quiet"), os(reference)],
        64 * 1024,
    )?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(git.command_error("check whether a branch exists", &output)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_is_ascii_bounded_and_safe() {
        assert_eq!(slugify("  Fix AUTH / API!!! "), "fix-auth-api");
        assert_eq!(slugify("東京"), "task");
        assert!(slugify(&"a".repeat(100)).len() <= MAX_SLUG_BYTES);
        assert!(slugify("Olá café").is_ascii());
    }
}
