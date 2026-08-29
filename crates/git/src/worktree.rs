use std::path::{Path, PathBuf};

/// Request to create an isolated Git worktree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorktreeCreate<'a> {
    /// Repository path or a directory inside it.
    pub repository: &'a Path,
    /// Directory that must contain the created worktree after normalization.
    pub managed_root: &'a Path,
    /// Project identifier used in the default path.
    pub project_id: &'a str,
    /// Human task label used to build a slug.
    pub task_label: &'a str,
    /// Optional explicit branch. Generated when absent.
    pub branch: Option<String>,
    /// Optional explicit path. Generated when absent.
    pub path: Option<PathBuf>,
}

/// A Git worktree observed through `git worktree list`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorktreeInfo {
    /// Worktree directory.
    pub path: PathBuf,
    /// Checked-out branch, or `HEAD` when detached.
    pub branch: String,
    /// True for the repository's primary worktree.
    pub is_primary: bool,
}

/// Result of `prepare_remove`, including a token bound to the observed state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemovalPlan {
    /// Repository root.
    pub repository: PathBuf,
    /// Worktree path to remove.
    pub path: PathBuf,
    /// Branch checked out in the worktree.
    pub branch: String,
    /// Whether uncommitted or untracked files exist.
    pub is_dirty: bool,
    /// Opaque token that `remove_worktree` must present unchanged.
    pub token: String,
}

pub(crate) fn slugify(label: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;
    for ch in label.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash && !slug.is_empty() {
            slug.push('-');
            previous_dash = true;
        }
    }
    let slug = slug.trim_matches('-').to_owned();
    if slug.is_empty() {
        "session".to_owned()
    } else {
        slug.chars().take(48).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::slugify;

    #[test]
    fn slugify_lowercases_and_strips_punctuation() {
        assert_eq!(
            slugify("Implement Authentication!"),
            "implement-authentication"
        );
        assert_eq!(slugify("   "), "session");
    }
}
