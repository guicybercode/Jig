use std::{
    ffi::OsStr,
    path::{Component, Path},
};

use crate::{GitError, GitErrorKind};

pub(crate) fn validate_pathspec(pathspec: &Path) -> Result<(), GitError> {
    if pathspec.as_os_str().is_empty() {
        return Err(GitError::new(
            GitErrorKind::InvalidInput,
            "Diff pathspec must not be empty",
            "Choose a repository-relative file path",
        ));
    }
    if pathspec.is_absolute() {
        return Err(GitError::new(
            GitErrorKind::InvalidInput,
            "Diff pathspec must be repository-relative",
            "Choose a path inside the registered repository",
        )
        .with_path(pathspec));
    }
    let mut saw_normal = false;
    for component in pathspec.components() {
        match component {
            Component::Normal(name) => {
                saw_normal = true;
                if is_option_like(name) {
                    return Err(GitError::new(
                        GitErrorKind::InvalidInput,
                        "Diff pathspec must not look like a Git option",
                        "Choose a repository-relative file path that does not start with a dash",
                    )
                    .with_path(pathspec));
                }
            }
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                return Err(GitError::new(
                    GitErrorKind::UnsafePath,
                    "Diff pathspec must not contain traversal or prefix components",
                    "Choose a repository-relative file path",
                )
                .with_path(pathspec));
            }
        }
    }
    if !saw_normal {
        return Err(GitError::new(
            GitErrorKind::InvalidInput,
            "Diff pathspec must contain a file name",
            "Choose a repository-relative file path",
        )
        .with_path(pathspec));
    }
    Ok(())
}

pub(crate) fn ensure_pathspec_inside_root(root: &Path, pathspec: &Path) -> Result<(), GitError> {
    validate_pathspec(pathspec)?;
    let joined = root.join(pathspec);
    match joined.symlink_metadata() {
        Ok(_) => {
            let canonical = joined.canonicalize().map_err(|error| {
                GitError::io("resolve diff pathspec", error).with_path(pathspec)
            })?;
            if !canonical.starts_with(root) {
                return Err(GitError::new(
                    GitErrorKind::UnsafePath,
                    "Diff pathspec escaped the repository root",
                    "Choose a path inside the registered repository",
                )
                .with_path(pathspec));
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(GitError::io("inspect diff pathspec", error).with_path(pathspec)),
    }
}

/// Lossy UTF-8 display form of a repository path for IPC.
#[must_use]
pub fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn is_option_like(name: &OsStr) -> bool {
    name == OsStr::new("--") || os_bytes(name).first() == Some(&b'-')
}

fn os_bytes(value: &OsStr) -> &[u8] {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        value.as_bytes()
    }
    #[cfg(not(unix))]
    {
        value.to_str().map_or(&[], str::as_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::{display_path, validate_pathspec};
    use std::path::Path;

    #[test]
    fn accepts_nested_relative_files() {
        validate_pathspec(Path::new("src/lib.rs")).expect("relative file should be accepted");
        validate_pathspec(Path::new(".gitignore")).expect("dotfile should be accepted");
    }

    #[test]
    fn rejects_traversal_and_option_like_names() {
        for invalid in ["../secret", "/tmp/secret", "-u", "--", "./foo"] {
            assert!(validate_pathspec(Path::new(invalid)).is_err(), "{invalid}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn wire_display_replaces_invalid_utf8() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let path = Path::new(OsStr::from_bytes(&[b'c', b'a', 0xff, b'e']));
        let displayed = display_path(path);
        assert!(displayed.contains('\u{fffd}') || displayed.contains('�'));
        assert!(!displayed.as_bytes().contains(&0xff));
    }
}
