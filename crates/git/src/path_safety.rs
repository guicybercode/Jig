use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

use crate::{GitError, GitErrorKind};

pub(crate) fn canonical_existing_directory(
    path: &Path,
    label: &'static str,
) -> Result<PathBuf, GitError> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => {
            return Err(GitError::new(
                GitErrorKind::InvalidInput,
                format!("{label} is not a directory: {}", path.display()),
                "Choose a directory instead of a file",
            )
            .with_path(path));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(GitError::new(
                GitErrorKind::NotFound,
                format!("{label} does not exist: {}", path.display()),
                "Choose an existing directory",
            )
            .with_path(path));
        }
        Err(error) => return Err(GitError::io("inspect directory", error).with_path(path)),
    }
    path.canonicalize()
        .map_err(|error| GitError::io("resolve directory", error).with_path(path))
}

pub(crate) fn canonical_intended_directory(
    path: &Path,
    label: &'static str,
) -> Result<PathBuf, GitError> {
    validate_absolute_normal_path(path, label)?;
    let mut cursor = path;
    let mut missing = Vec::new();
    loop {
        match fs::symlink_metadata(cursor) {
            Ok(_) => {
                let metadata = fs::metadata(cursor).map_err(|error| {
                    GitError::io("resolve intended directory ancestor", error).with_path(cursor)
                })?;
                if !metadata.is_dir() {
                    return Err(GitError::new(
                        GitErrorKind::InvalidInput,
                        format!("{label} crosses a non-directory: {}", cursor.display()),
                        "Choose a path whose existing ancestors are directories",
                    )
                    .with_path(cursor));
                }
                let mut resolved = cursor.canonicalize().map_err(|error| {
                    GitError::io("resolve intended directory ancestor", error).with_path(cursor)
                })?;
                for component in missing.iter().rev() {
                    resolved.push(component);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let name = cursor.file_name().ok_or_else(|| {
                    GitError::new(
                        GitErrorKind::InvalidInput,
                        format!("{label} has no resolvable existing ancestor"),
                        "Configure an absolute path below an existing directory",
                    )
                    .with_path(path)
                })?;
                missing.push(name.to_os_string());
                cursor = cursor.parent().ok_or_else(|| {
                    GitError::new(
                        GitErrorKind::InvalidInput,
                        format!("{label} has no parent directory"),
                        "Configure an absolute path below an existing directory",
                    )
                    .with_path(path)
                })?;
            }
            Err(error) => {
                return Err(GitError::io("inspect intended directory", error).with_path(cursor));
            }
        }
    }
}

pub(crate) fn validate_descendant(
    managed_root: &Path,
    target: &Path,
    must_exist: bool,
) -> Result<(), GitError> {
    let checked = if must_exist {
        target
            .canonicalize()
            .map_err(|error| GitError::io("resolve managed worktree", error).with_path(target))?
    } else {
        if path_occupied(target)? {
            return Err(GitError::new(
                GitErrorKind::InvalidInput,
                format!("Worktree destination already exists: {}", target.display()),
                "Choose a different session name or identifier",
            )
            .with_path(target));
        }
        let parent = target.parent().ok_or_else(|| {
            GitError::new(
                GitErrorKind::UnsafePath,
                "Worktree destination has no parent directory",
                "Use a destination below the managed worktree root",
            )
        })?;
        let name = target.file_name().ok_or_else(|| {
            GitError::new(
                GitErrorKind::UnsafePath,
                "Worktree destination has no directory name",
                "Use a destination below the managed worktree root",
            )
        })?;
        canonical_intended_directory(parent, "worktree destination parent")?.join(name)
    };
    if checked == managed_root || !checked.starts_with(managed_root) {
        return Err(GitError::new(
            GitErrorKind::UnsafePath,
            format!(
                "Worktree path is outside the managed root: {}",
                checked.display()
            ),
            "Choose a worktree created below the configured managed worktree root",
        )
        .with_path(checked));
    }
    Ok(())
}

pub(crate) fn path_occupied(path: &Path) -> Result<bool, GitError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(GitError::io("inspect worktree destination", error).with_path(path)),
    }
}

pub(crate) fn paths_match(left: &Path, right: &Path) -> bool {
    left == right || normalize_if_possible(left) == normalize_if_possible(right)
}

fn validate_absolute_normal_path(path: &Path, label: &'static str) -> Result<(), GitError> {
    if path.as_os_str().is_empty() || !path.is_absolute() {
        return Err(GitError::new(
            GitErrorKind::InvalidInput,
            format!("{label} must be an absolute path: {}", path.display()),
            "Configure an absolute path",
        )
        .with_path(path));
    }
    if path
        .components()
        .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err(GitError::new(
            GitErrorKind::UnsafePath,
            format!(
                "{label} contains a relative path component: {}",
                path.display()
            ),
            "Normalize the path before planning the worktree",
        )
        .with_path(path));
    }
    Ok(())
}

fn normalize_if_possible(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        canonical_intended_directory(path, "worktree path").unwrap_or_else(|_| path.to_path_buf())
    })
}
