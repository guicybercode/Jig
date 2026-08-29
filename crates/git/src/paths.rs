use std::{
    ffi::OsString,
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

/// Resolves the longest existing prefix, then appends missing components.
///
/// On macOS `/var` is a symlink to `/private/var`. Canonicalizing only the
/// final missing leaf would leave `/var/...` compared against `/private/var/...`.
pub(crate) fn real_or_absolute(path: &Path) -> Result<PathBuf, GitError> {
    let absolute = normalize_absolute(path)?;
    let mut existing = absolute.as_path();
    let mut missing: Vec<OsString> = Vec::new();
    loop {
        if existing.exists() {
            let mut resolved = existing_real_path(existing)?;
            for part in missing.iter().rev() {
                resolved.push(part);
            }
            return Ok(resolved);
        }
        match existing.file_name() {
            Some(name) => {
                missing.push(name.to_os_string());
                match existing.parent() {
                    Some(parent) if parent != existing => existing = parent,
                    _ => return Ok(absolute),
                }
            }
            None => return Ok(absolute),
        }
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
    let stop_at = real_or_absolute(stop_at)?;

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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::symlink;

    use tempfile::TempDir;

    use super::{ensure_within, real_or_absolute};

    #[test]
    fn real_or_absolute_follows_a_symlink_prefix() {
        let temp = TempDir::new().expect("temp");
        let real = temp.path().join("real");
        let link = temp.path().join("link");
        fs::create_dir(&real).expect("real dir");
        symlink(&real, &link).expect("symlink");

        let nested = link.join("missing").join("leaf");
        let resolved = real_or_absolute(&nested).expect("resolve");
        let expected = real
            .canonicalize()
            .expect("canonical real")
            .join("missing")
            .join("leaf");
        assert_eq!(resolved, expected);
        assert!(ensure_within(&nested, &link).is_ok());
        assert!(ensure_within(&nested, &real).is_ok());
    }

    #[test]
    fn ensure_within_rejects_a_symlink_that_leaves_the_root() {
        let temp = TempDir::new().expect("temp");
        let root = temp.path().join("managed");
        let outside = temp.path().join("outside");
        fs::create_dir(&root).expect("root");
        fs::create_dir(&outside).expect("outside");
        symlink(&outside, root.join("escape")).expect("escape symlink");
        let error = ensure_within(&root.join("escape").join("leaf"), &root)
            .expect_err("escaped path must be rejected");
        assert!(matches!(
            error,
            crate::error::GitError::PathOutsideManagedRoot { .. }
        ));
    }
}
