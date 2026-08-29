use std::path::{Path, PathBuf};

use crate::{Git, GitError, GitErrorKind, os, repository};

/// High-level classification of a changed path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChangeKind {
    /// Existing content or metadata changed.
    Modified,
    /// A path was added, copied, or renamed into place.
    Added,
    /// A tracked path was deleted.
    Deleted,
    /// A path is not tracked by Git.
    Untracked,
}

/// One changed repository-relative path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangedFile {
    /// Repository-relative path.
    pub path: PathBuf,
    /// Previous path for a rename or copy.
    pub original_path: Option<PathBuf>,
    /// High-level UI classification.
    pub kind: ChangeKind,
    /// Whether the index differs from `HEAD`.
    pub staged: bool,
    /// Whether the working tree differs from the index.
    pub unstaged: bool,
}

/// Aggregate changed-file counts.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StatusCounts {
    /// Modified tracked paths.
    pub modified: usize,
    /// Added, copied, or renamed paths.
    pub added: usize,
    /// Deleted tracked paths.
    pub deleted: usize,
    /// Untracked paths.
    pub untracked: usize,
}

/// Branch and changed-file state parsed from porcelain v2.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryStatus {
    /// Symbolic branch, or `None` when `HEAD` is detached.
    pub branch: Option<String>,
    /// Changed paths in Git's stable porcelain order.
    pub files: Vec<ChangedFile>,
    /// Aggregate counts by UI category.
    pub counts: StatusCounts,
    /// Whether any index entry differs from `HEAD`.
    pub has_staged: bool,
    /// Whether any tracked working-tree path differs from the index.
    pub has_tracked_changes: bool,
    /// Whether any untracked path exists.
    pub has_untracked: bool,
}

impl RepositoryStatus {
    /// Returns whether the repository has staged, tracked, or untracked changes.
    #[must_use]
    pub const fn is_dirty(&self) -> bool {
        self.has_staged || self.has_tracked_changes || self.has_untracked
    }
}

pub(crate) fn read(git: &Git, path: &Path) -> Result<RepositoryStatus, GitError> {
    let root = repository::require_root(git, path)?;
    let output = git.checked(
        Some(&root),
        [
            os("status"),
            os("--porcelain=v2"),
            os("--branch"),
            os("-z"),
            os("--untracked-files=all"),
        ],
        "read repository status",
    )?;
    parse(&output.stdout)
}

fn parse(bytes: &[u8]) -> Result<RepositoryStatus, GitError> {
    parse_with_ignored(bytes).map(|(status, _)| status)
}

pub(crate) fn read_for_removal(
    git: &Git,
    path: &Path,
) -> Result<(RepositoryStatus, Vec<PathBuf>), GitError> {
    let root = repository::require_root(git, path)?;
    let output = git.checked(
        Some(&root),
        [
            os("status"),
            os("--porcelain=v2"),
            os("--branch"),
            os("-z"),
            os("--untracked-files=all"),
            os("--ignored=matching"),
        ],
        "read removal-safe repository status",
    )?;
    parse_with_ignored(&output.stdout)
}

fn parse_with_ignored(bytes: &[u8]) -> Result<(RepositoryStatus, Vec<PathBuf>), GitError> {
    let records: Vec<&[u8]> = bytes.split(|byte| *byte == 0).collect();
    let mut index = 0;
    let mut branch = None;
    let mut files = Vec::new();
    let mut ignored = Vec::new();
    while index < records.len() {
        let record = records[index];
        index += 1;
        if record.is_empty() {
            continue;
        }
        if let Some(value) = record.strip_prefix(b"# branch.head ") {
            if value != b"(detached)" && value != b"(initial)" {
                branch = Some(parse_utf8(value, "branch name")?.to_owned());
            }
            continue;
        }
        match record[0] {
            b'1' => files.push(parse_ordinary(record)?),
            b'2' => {
                let original = records.get(index).ok_or_else(invalid_porcelain)?;
                index += 1;
                files.push(parse_renamed(record, original)?);
            }
            b'u' => files.push(parse_unmerged(record)?),
            b'?' => files.push(ChangedFile {
                path: parse_path(field_after_prefix(record, b"? ")?),
                original_path: None,
                kind: ChangeKind::Untracked,
                staged: false,
                unstaged: false,
            }),
            b'!' => ignored.push(parse_path(field_after_prefix(record, b"! ")?)),
            _ if record.starts_with(b"# ") => {}
            _ => return Err(invalid_porcelain()),
        }
    }

    let mut counts = StatusCounts::default();
    let mut has_staged = false;
    let mut has_tracked_changes = false;
    let mut has_untracked = false;
    for file in &files {
        match file.kind {
            ChangeKind::Modified => counts.modified += 1,
            ChangeKind::Added => counts.added += 1,
            ChangeKind::Deleted => counts.deleted += 1,
            ChangeKind::Untracked => counts.untracked += 1,
        }
        has_staged |= file.staged;
        has_tracked_changes |= file.unstaged;
        has_untracked |= file.kind == ChangeKind::Untracked;
    }
    Ok((
        RepositoryStatus {
            branch,
            files,
            counts,
            has_staged,
            has_tracked_changes,
            has_untracked,
        },
        ignored,
    ))
}

fn parse_ordinary(record: &[u8]) -> Result<ChangedFile, GitError> {
    let fields = split_prefix_fields(record, 8)?;
    changed_file(fields[1], fields[8], None)
}

fn parse_renamed(record: &[u8], original: &[u8]) -> Result<ChangedFile, GitError> {
    let fields = split_prefix_fields(record, 9)?;
    changed_file(fields[1], fields[9], Some(parse_path(original)))
}

fn parse_unmerged(record: &[u8]) -> Result<ChangedFile, GitError> {
    let fields = split_prefix_fields(record, 10)?;
    let mut file = changed_file(fields[1], fields[10], None)?;
    file.kind = ChangeKind::Modified;
    file.staged = true;
    file.unstaged = true;
    Ok(file)
}

fn split_prefix_fields(record: &[u8], spaces: usize) -> Result<Vec<&[u8]>, GitError> {
    let mut fields = Vec::with_capacity(spaces + 1);
    let mut remainder = record;
    for _ in 0..spaces {
        let position = remainder
            .iter()
            .position(|byte| *byte == b' ')
            .ok_or_else(invalid_porcelain)?;
        fields.push(&remainder[..position]);
        remainder = &remainder[position + 1..];
    }
    fields.push(remainder);
    Ok(fields)
}

fn changed_file(
    xy: &[u8],
    path: &[u8],
    original_path: Option<PathBuf>,
) -> Result<ChangedFile, GitError> {
    if xy.len() != 2 {
        return Err(invalid_porcelain());
    }
    let index = xy[0];
    let worktree = xy[1];
    let kind = classify(index, worktree);
    Ok(ChangedFile {
        path: parse_path(path),
        original_path,
        kind,
        staged: index != b'.',
        unstaged: worktree != b'.',
    })
}

fn classify(index: u8, worktree: u8) -> ChangeKind {
    if index == b'D' || worktree == b'D' {
        ChangeKind::Deleted
    } else if matches!(index, b'A' | b'R' | b'C') || matches!(worktree, b'A' | b'R' | b'C') {
        ChangeKind::Added
    } else {
        ChangeKind::Modified
    }
}

fn field_after_prefix<'a>(record: &'a [u8], prefix: &[u8]) -> Result<&'a [u8], GitError> {
    record.strip_prefix(prefix).ok_or_else(invalid_porcelain)
}

fn parse_utf8<'a>(bytes: &'a [u8], label: &'static str) -> Result<&'a str, GitError> {
    std::str::from_utf8(bytes).map_err(|_| {
        GitError::new(
            GitErrorKind::InvalidOutput,
            format!("Git returned a non-UTF-8 {label}"),
            "Inspect the repository with the Git command line",
        )
    })
}

fn parse_path(bytes: &[u8]) -> PathBuf {
    #[cfg(unix)]
    {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        PathBuf::from(OsString::from_vec(bytes.to_vec()))
    }
    #[cfg(not(unix))]
    {
        PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
    }
}

fn invalid_porcelain() -> GitError {
    GitError::new(
        GitErrorKind::InvalidOutput,
        "Git returned malformed porcelain v2 status output",
        "Verify the repository with `git status` and update Git if the problem persists",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_branch_counts_and_rename_path() {
        let input = b"# branch.head main\0\
            1 .M N... 100644 100644 100644 abc abc src/lib.rs\0\
            1 A. N... 000000 100644 100644 000 abc new.txt\0\
            1 .D N... 100644 100644 000000 abc abc gone.txt\0\
            2 R. N... 100644 100644 100644 abc def R100 renamed.txt\0old.txt\0\
            ? untracked file.txt\0";
        let status = parse(input).expect("porcelain fixture should parse");
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(status.counts.modified, 1);
        assert_eq!(status.counts.added, 2);
        assert_eq!(status.counts.deleted, 1);
        assert_eq!(status.counts.untracked, 1);
        assert_eq!(
            status.files[3].original_path,
            Some(PathBuf::from("old.txt"))
        );
        assert!(status.has_staged);
        assert!(status.has_tracked_changes);
        assert!(status.has_untracked);
    }

    #[test]
    fn removal_parser_preserves_ignored_paths() {
        let input = b"# branch.head main\0! target/\0! local secret.txt\0";
        let (status, ignored) =
            parse_with_ignored(input).expect("removal porcelain fixture should parse");
        assert!(!status.is_dirty());
        assert_eq!(
            ignored,
            [PathBuf::from("target/"), PathBuf::from("local secret.txt")]
        );
    }
}
