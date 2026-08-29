use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use crate::error::GitError;

pub(crate) fn normalize_absolute(path: &Path) -> Result<PathBuf, GitError> {
    let absolute = std::path::absolute(path).map_err(|error| GitError::SpawnFailed {
        message: format!("could not make path absolute: {error}"),
    })?;
    Ok(lexical_normalize(&absolute))
}

pub(crate) fn lexical_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => match normalized.components().next_back() {
                Some(Component::RootDir | Component::Prefix(_)) | None => {}
                Some(_) => {
                    normalized.pop();
                }
            },
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

pub(crate) fn existing_real_path(path: &Path) -> Result<PathBuf, GitError> {
    path.canonicalize().map_err(|error| GitError::SpawnFailed {
        message: format!("could not resolve {}: {error}", path.display()),
    })
}

pub(crate) fn real_or_absolute(path: &Path) -> Result<PathBuf, GitError> {
    if path.exists() {
        existing_real_path(path)
    } else if let Some(parent) = path.parent()
        && parent.exists()
    {
        let file_name = path.file_name().ok_or_else(|| GitError::Internal {
            message: "path is missing a file name".to_owned(),
        })?;
        Ok(existing_real_path(parent)?.join(file_name))
    } else {
        normalize_absolute(path)
    }
}

pub(crate) fn ensure_within(path: &Path, root: &Path) -> Result<PathBuf, GitError> {
    let path = real_or_absolute(path)?;
    let root = real_or_absolute(root)?;
    if path.starts_with(&root) {
        Ok(path)
    } else {
        Err(GitError::PathOutsideManagedRoot { path, root })
    }
}

pub(crate) fn reject_symlink(path: &Path) -> Result<(), GitError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(GitError::SymlinkRejected {
            path: path.to_path_buf(),
        }),
        Ok(_) | Err(_) => Ok(()),
    }
}

pub(crate) fn reject_symlink_ancestors(path: &Path, root: &Path) -> Result<(), GitError> {
    let path = normalize_absolute(path)?;
    let root = normalize_absolute(root)?;
    let mut current = path.as_path();
    loop {
        if let Ok(metadata) = fs::symlink_metadata(current)
            && metadata.file_type().is_symlink()
        {
            return Err(GitError::SymlinkRejected {
                path: current.to_path_buf(),
            });
        }
        if current == root {
            break;
        }
        match current.parent() {
            Some(parent) if parent != current => current = parent,
            _ => break,
        }
    }
    Ok(())
}

pub(crate) fn paths_equal(left: &Path, right: &Path) -> bool {
    match (real_or_absolute(left), real_or_absolute(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

pub(crate) fn create_missing_ancestors(
    path: &Path,
    stop_at: &Path,
) -> Result<Vec<PathBuf>, GitError> {
    let parent = path.parent().ok_or_else(|| GitError::Internal {
        message: "worktree path has no parent directory".to_owned(),
    })?;
    let parent = ensure_within(parent, stop_at)?;
    let stop_at = normalize_absolute(stop_at)?;

    let mut missing = Vec::new();
    let mut current = parent;
    while current.starts_with(&stop_at) && current != stop_at {
        if current.exists() {
            break;
        }
        missing.push(current.clone());
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => break,
        }
    }
    missing.reverse();
    for directory in &missing {
        fs::create_dir(directory).map_err(|error| GitError::SpawnFailed {
            message: format!("could not create {}: {error}", directory.display()),
        })?;
    }
    Ok(missing)
}

pub(crate) fn remove_empty_dirs(directories: &[PathBuf]) {
    for directory in directories.iter().rev() {
        let _ = fs::remove_dir(directory);
    }
}
