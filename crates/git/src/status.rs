use crate::GitError;

/// Parsed `git status --porcelain=v2` snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitStatus {
    /// Current branch name, if Git reported one.
    pub branch: Option<String>,
    /// True when staged, unstaged, unmerged, or untracked paths exist.
    pub is_dirty: bool,
    /// Number of changed or untracked paths.
    pub changed_paths: usize,
}

/// Size-capped textual diff.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitDiff {
    /// Diff text, possibly truncated.
    pub text: String,
    /// Whether the diff exceeded the v0.1 size cap.
    pub truncated: bool,
}

pub(crate) fn parse_porcelain_v2(bytes: &[u8]) -> Result<GitStatus, GitError> {
    let text = String::from_utf8(bytes.to_vec()).map_err(|_| GitError::InvalidUtf8("status"))?;
    let mut branch = None;
    let mut changed_paths = 0;

    for record in text.split('\0') {
        if record.is_empty() {
            continue;
        }
        let line = record.lines().next().unwrap_or(record);
        if let Some(name) = line.strip_prefix("# branch.head ") {
            if name != "(detached)" {
                branch = Some(name.to_owned());
            }
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        if line.starts_with('1')
            || line.starts_with('2')
            || line.starts_with('u')
            || line.starts_with('?')
            || line.starts_with('!')
        {
            changed_paths += 1;
        }
    }

    Ok(GitStatus {
        branch,
        is_dirty: changed_paths > 0,
        changed_paths,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_porcelain_v2;

    #[test]
    fn parses_clean_branch_status() {
        let raw = b"# branch.head main\0# branch.oid abc\0";
        let status = parse_porcelain_v2(raw).expect("status should parse");
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert!(!status.is_dirty);
        assert_eq!(status.changed_paths, 0);
    }

    #[test]
    fn treats_untracked_and_modified_as_dirty() {
        let raw =
            b"# branch.head feature\0\x31 .M N... 100644 100644 100644 0 0 README.md\0? dirty.txt\0";
        let status = parse_porcelain_v2(raw).expect("status should parse");
        assert!(status.is_dirty);
        assert_eq!(status.changed_paths, 2);
    }
}
