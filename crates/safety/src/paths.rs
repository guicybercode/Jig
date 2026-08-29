use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};

use cli_master_core::{ApplicationError, ErrorCode};

/// Directories the application is allowed to create or delete.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedRoots {
    /// `{data-dir}/worktrees`.
    pub worktree_root: PathBuf,
    /// Application data directory.
    pub data_dir: PathBuf,
    /// Registered project repository roots. These must never be deleted.
    pub project_roots: Vec<PathBuf>,
}

impl ManagedRoots {
    /// Creates a managed-root set with no registered projects.
    #[must_use]
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        let data_dir = data_dir.into();
        Self {
            worktree_root: data_dir.join("worktrees"),
            data_dir,
            project_roots: Vec::new(),
        }
    }

    /// Records a project repository that must not be recursively deleted.
    #[must_use]
    pub fn with_project_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.project_roots.push(root.into());
        self
    }
}

/// A path whose existing prefix has been canonicalized.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedPath {
    /// Absolute path. Existing prefixes are symlink-resolved.
    pub path: PathBuf,
    /// Whether the final path currently exists.
    pub exists: bool,
    /// Whether the original last component is a symlink.
    pub last_component_symlink: bool,
}

/// Collapses `.` and `..` without touching the filesystem.
///
/// This does not resolve symlinks and does not require the path to exist.
#[must_use]
pub fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir | Component::Prefix(_)) => {}
                Some(Component::ParentDir | Component::CurDir) | None => out.push(".."),
            },
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        out
    }
}

/// Canonicalizes a path that must already exist.
///
/// # Errors
///
/// Returns [`ErrorCode::InvalidPath`] when the path is missing or cannot be
/// canonicalized.
pub fn canonicalize_existing(path: &Path) -> Result<PathBuf, ApplicationError> {
    fs::canonicalize(path).map_err(|error| {
        ApplicationError::new(
            ErrorCode::InvalidPath,
            format!(
                "Path does not exist or cannot be resolved: {}",
                path.display()
            ),
        )
        .with_action("Choose an existing directory that CLI Master manages.")
        .with_context("path", path.display().to_string())
        .with_source(&error)
    })
}

/// Resolves a path that may not exist yet.
///
/// Existing prefixes are canonicalized so symlink escapes are visible. Missing
/// suffix components are joined lexically and `..` is applied against the
/// resolved prefix instead of inventing a canonical path for a missing file.
///
/// # Errors
///
/// Returns [`ErrorCode::InvalidPath`] when the path is empty or cannot be made
/// absolute.
pub fn resolve_path(path: &Path) -> Result<ResolvedPath, ApplicationError> {
    if path.as_os_str().is_empty() {
        return Err(invalid_path(path, "Path must not be empty."));
    }

    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .map_err(|error| {
                invalid_path(path, "Could not determine the current directory.").with_source(&error)
            })?
            .join(path)
    };

    let last_component_symlink =
        fs::symlink_metadata(&absolute).is_ok_and(|metadata| metadata.file_type().is_symlink());

    let mut existing = absolute.clone();
    let mut missing = Vec::new();
    loop {
        match fs::symlink_metadata(&existing) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => match existing.parent() {
                Some(parent) if parent != existing => {
                    match existing.components().next_back() {
                        Some(Component::ParentDir) => missing.push("..".into()),
                        Some(Component::Normal(name)) => missing.push(name.to_os_string()),
                        Some(Component::CurDir | Component::Prefix(_) | Component::RootDir)
                        | None => {}
                    }
                    existing = parent.to_path_buf();
                }
                _ => break,
            },
            Err(error) => {
                return Err(
                    invalid_path(&existing, "Could not inspect an existing path prefix.")
                        .with_source(&error),
                );
            }
        }
    }

    let mut resolved = if existing.exists() {
        fs::canonicalize(&existing).map_err(|error| {
            invalid_path(&existing, "Could not resolve an existing path prefix.")
                .with_source(&error)
        })?
    } else {
        normalize_lexical(&absolute)
    };

    for name in missing.into_iter().rev() {
        if name == ".." {
            if !resolved.pop() {
                resolved.push("..");
            }
        } else if name != "." {
            resolved.push(name);
        }
    }

    Ok(ResolvedPath {
        exists: absolute.exists(),
        last_component_symlink,
        path: resolved,
    })
}

/// Returns whether `candidate` is inside `root` after resolution.
///
/// # Errors
///
/// Returns [`ErrorCode::InvalidPath`] when either path cannot be resolved.
pub fn is_within(candidate: &Path, root: &Path) -> Result<bool, ApplicationError> {
    let candidate = resolve_path(candidate)?.path;
    let root = resolve_path(root)?.path;
    Ok(path_is_prefix(&root, &candidate))
}

/// Refuses `/`, the user home directory, ancestors of home, and project roots.
///
/// # Errors
///
/// Returns [`ErrorCode::CriticalPathRefused`] when the path is protected.
pub fn assert_not_critical(path: &Path, roots: &ManagedRoots) -> Result<(), ApplicationError> {
    let resolved = resolve_path(path)?.path;
    let forbidden = critical_paths(roots)
        .into_iter()
        .map(|item| resolve_path(&item).map(|resolved| resolved.path))
        .collect::<Result<Vec<_>, _>>()?;

    if forbidden.iter().any(|item| item == &resolved) {
        return Err(ApplicationError::new(
            ErrorCode::CriticalPathRefused,
            format!("Refusing to modify protected path {}.", resolved.display()),
        )
        .not_recoverable()
        .with_action("Choose a managed worktree directory instead.")
        .with_context("path", resolved.display().to_string()));
    }

    Ok(())
}

/// Confirms that `path` is a managed worktree location.
///
/// # Errors
///
/// Returns an error when the path escapes the managed worktree root, is a
/// protected location, or cannot be resolved.
pub fn assert_managed_worktree(
    path: &Path,
    roots: &ManagedRoots,
) -> Result<PathBuf, ApplicationError> {
    let resolved_path = resolve_path(path)?;
    if resolved_path.last_component_symlink {
        return Err(ApplicationError::new(
            ErrorCode::InvalidPath,
            "Refusing to operate on a worktree path that is a symbolic link.",
        )
        .not_recoverable()
        .with_action("Select the real managed worktree directory.")
        .with_context("path", path.display().to_string()));
    }
    let resolved = resolved_path.path;
    let worktree_root = resolve_path(&roots.worktree_root)?.path;
    let data_dir = resolve_path(&roots.data_dir)?.path;
    assert_not_critical(&resolved, roots)?;

    if !path_is_prefix(&worktree_root, &resolved) {
        return Err(ApplicationError::new(
            ErrorCode::UnmanagedPath,
            format!(
                "Path {} is outside the managed worktree directory.",
                resolved.display()
            ),
        )
        .not_recoverable()
        .with_action("Use a worktree created by CLI Master.")
        .with_context("path", resolved.display().to_string())
        .with_context("worktreeRoot", worktree_root.display().to_string()));
    }

    if resolved == worktree_root || resolved == data_dir {
        return Err(ApplicationError::new(
            ErrorCode::CriticalPathRefused,
            "Refusing to delete the application data or worktree root.",
        )
        .not_recoverable()
        .with_action("Select a specific worktree directory.")
        .with_context("path", resolved.display().to_string()));
    }

    Ok(resolved)
}

fn invalid_path(path: &Path, message: &str) -> ApplicationError {
    ApplicationError::new(ErrorCode::InvalidPath, message.to_owned())
        .with_action("Choose a valid absolute path.")
        .with_context("path", path.display().to_string())
}

fn critical_paths(roots: &ManagedRoots) -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from("/")];
    if let Some(home) = env::var_os("HOME") {
        let mut current = Some(PathBuf::from(home));
        while let Some(path) = current {
            if path.as_os_str().is_empty() || path == Path::new("/") {
                break;
            }
            current = path.parent().map(Path::to_path_buf);
            paths.push(path);
        }
    }
    paths.push(PathBuf::from("/home"));
    paths.push(PathBuf::from("/Users"));
    paths.push(roots.data_dir.clone());
    paths.push(roots.worktree_root.clone());
    paths.extend(roots.project_roots.iter().cloned());
    paths
        .into_iter()
        .filter(|path| !path.as_os_str().is_empty())
        .collect()
}

fn path_is_prefix(root: &Path, candidate: &Path) -> bool {
    let Ok(stripped) = candidate.strip_prefix(root) else {
        return false;
    };
    if stripped.as_os_str().is_empty() {
        return true;
    }
    stripped
        .components()
        .all(|component| !matches!(component, Component::ParentDir))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::symlink;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn lexical_normalize_collapses_parent_segments() {
        let path = Path::new("/tmp/worktrees/../worktrees/session");
        assert_eq!(
            normalize_lexical(path),
            PathBuf::from("/tmp/worktrees/session")
        );
        assert_eq!(
            normalize_lexical(Path::new("../../child")),
            PathBuf::from("../../child")
        );
    }

    #[test]
    fn spaces_and_unicode_are_preserved() {
        let path = Path::new("/tmp/projeto café/my worktree");
        let resolved = resolve_path(path).expect("unicode path should resolve");
        assert!(resolved.path.ends_with("my worktree"));
        assert!(resolved.path.to_string_lossy().contains("projeto café"));
    }

    #[test]
    fn traversal_does_not_stay_inside_root() {
        let temp = TempDir::new().expect("temp dir");
        let root = temp.path().join("worktrees");
        fs::create_dir_all(&root).expect("root");
        let escaped = root.join("nested").join("..").join("..").join("secret");
        assert!(!is_within(&escaped, &root).expect("paths should resolve"));
    }

    #[test]
    fn symlink_escape_is_detected() {
        let temp = TempDir::new().expect("temp dir");
        let outside = temp.path().join("outside");
        let managed = temp.path().join("managed");
        fs::create_dir_all(&outside).expect("outside");
        fs::create_dir_all(&managed).expect("managed");
        let link = managed.join("escape");
        symlink(&outside, &link).expect("symlink");

        let resolved = resolve_path(&link).expect("symlink should resolve");
        assert_eq!(resolved.path, fs::canonicalize(&outside).expect("canon"));
        assert!(resolved.last_component_symlink);
        assert!(!is_within(&link, &managed).expect("within"));
    }

    #[test]
    fn missing_path_uses_canonical_parent() {
        let temp = TempDir::new().expect("temp dir");
        let parent = temp.path().join("managed");
        fs::create_dir_all(&parent).expect("parent");
        let missing = parent.join("new-worktree");
        let resolved = resolve_path(&missing).expect("missing path");
        assert!(!resolved.exists);
        assert_eq!(
            resolved.path,
            fs::canonicalize(&parent)
                .expect("parent")
                .join("new-worktree")
        );
    }

    #[test]
    fn refuses_home_and_root() {
        let temp = TempDir::new().expect("temp dir");
        let roots = ManagedRoots::new(temp.path());
        let error = assert_not_critical(Path::new("/"), &roots).expect_err("root");
        assert_eq!(error.code(), ErrorCode::CriticalPathRefused);
        if let Some(home) = env::var_os("HOME") {
            let error = assert_not_critical(Path::new(&home), &roots).expect_err("home");
            assert_eq!(error.code(), ErrorCode::CriticalPathRefused);
        }
    }

    #[test]
    fn refuses_unmanaged_and_project_root_deletion() {
        let temp = TempDir::new().expect("temp dir");
        let project = temp.path().join("repo");
        fs::create_dir_all(&project).expect("project");
        let data = temp.path().join("data");
        fs::create_dir_all(data.join("worktrees")).expect("worktrees");
        let roots = ManagedRoots::new(&data).with_project_root(&project);

        let unmanaged = temp.path().join("other");
        fs::create_dir_all(&unmanaged).expect("unmanaged");
        let error = assert_managed_worktree(&unmanaged, &roots).expect_err("unmanaged");
        assert_eq!(error.code(), ErrorCode::UnmanagedPath);

        let error = assert_not_critical(&project, &roots).expect_err("project");
        assert_eq!(error.code(), ErrorCode::CriticalPathRefused);
    }

    #[test]
    fn relative_managed_roots_cannot_authorize_the_worktree_root_itself() {
        let current = env::current_dir().expect("current directory");
        let temp = tempfile::tempdir_in(&current).expect("temp in current directory");
        let relative_data = temp
            .path()
            .strip_prefix(&current)
            .expect("temp should be below current directory")
            .join("data");
        let absolute_root = current.join(&relative_data).join("worktrees");
        fs::create_dir_all(&absolute_root).expect("worktree root");

        let roots = ManagedRoots::new(relative_data);
        let error = assert_managed_worktree(&absolute_root, &roots).expect_err("managed root");
        assert_eq!(error.code(), ErrorCode::CriticalPathRefused);
    }

    #[test]
    fn canonical_project_root_is_protected_when_registered_through_a_symlink() {
        let temp = TempDir::new().expect("temp");
        let project = temp.path().join("project");
        let alias = temp.path().join("project-alias");
        fs::create_dir_all(&project).expect("project");
        symlink(&project, &alias).expect("project alias");
        let roots = ManagedRoots::new(temp.path().join("data")).with_project_root(alias);

        let error = assert_not_critical(&project, &roots).expect_err("project root");
        assert_eq!(error.code(), ErrorCode::CriticalPathRefused);
    }

    #[test]
    fn final_component_symlink_is_never_a_removal_target() {
        let temp = TempDir::new().expect("temp");
        let data = temp.path().join("data");
        let real_worktree = data.join("worktrees/project/real");
        let alias = data.join("worktrees/project/alias");
        fs::create_dir_all(&real_worktree).expect("worktree");
        symlink(&real_worktree, &alias).expect("alias");
        let roots = ManagedRoots::new(data);

        let error = assert_managed_worktree(&alias, &roots).expect_err("symlink target");
        assert_eq!(error.code(), ErrorCode::InvalidPath);
    }

    #[test]
    fn broken_symlink_prefix_cannot_authorize_a_future_escape() {
        let temp = TempDir::new().expect("temp");
        let data = temp.path().join("data");
        let managed = data.join("worktrees/project");
        let outside = temp.path().join("outside-not-created");
        fs::create_dir_all(&managed).expect("managed");
        let escape = managed.join("escape");
        symlink(&outside, &escape).expect("broken escape symlink");
        let roots = ManagedRoots::new(data);

        let error = assert_managed_worktree(&escape.join("future"), &roots)
            .expect_err("broken symlink prefix");
        assert!(matches!(
            error.code(),
            ErrorCode::InvalidPath | ErrorCode::UnmanagedPath
        ));
    }
}
