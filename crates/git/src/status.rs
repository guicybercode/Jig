use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{command::GitCommand, error::GitError};

/// Kind of path change in the index or worktree.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    /// Unmodified in this side.
    Unmodified,
    /// Content changed.
    Modified,
    /// Newly added.
    Added,
    /// Deleted.
    Deleted,
    /// Renamed.
    Renamed,
    /// Copied.
    Copied,
    /// File type changed.
    TypeChanged,
    /// Unmerged or conflicted.
    Unmerged,
    /// Status letter was not recognized.
    Unknown,
}

/// One porcelain v2 status record.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusEntry {
    /// Current path.
    pub path: String,
    /// Original path when the change is a rename or copy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_path: Option<String>,
    /// Index (staged) change.
    pub staged: ChangeKind,
    /// Worktree (unstaged) change.
    pub unstaged: ChangeKind,
    /// Whether the path is untracked.
    pub untracked: bool,
}

/// Repository status derived from `git status --porcelain=v2 -z --branch`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    /// Current branch name, when `HEAD` is not detached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Whether `HEAD` is detached.
    pub detached: bool,
    /// Whether the repository has no commits.
    pub unborn: bool,
    /// Commits ahead of the upstream branch, when Git reports them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ahead: Option<u32>,
    /// Commits behind the upstream branch, when Git reports them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behind: Option<u32>,
    /// Paths with a worktree or index modification.
    pub modified: Vec<String>,
    /// Newly staged paths (`A` in the index).
    pub added: Vec<String>,
    /// Deleted paths.
    pub deleted: Vec<String>,
    /// Renamed paths, current name first.
    pub renamed: Vec<String>,
    /// Untracked paths.
    pub untracked: Vec<String>,
    /// Paths with a non-empty index side.
    pub staged: Vec<String>,
    /// Paths with a non-empty worktree side.
    pub unstaged: Vec<String>,
    /// Raw parsed entries.
    pub entries: Vec<StatusEntry>,
}

impl GitStatus {
    /// Returns whether the worktree has any staged, unstaged, or untracked changes.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        !self.entries.is_empty()
    }
}

pub(crate) fn status(executable: &Path, path: &Path) -> Result<GitStatus, GitError> {
    crate::command::inspect_path(path)?;
    let output = GitCommand::new(executable)
        .read_only()
        .repo(path)
        .arg("status")
        .arg("--porcelain=v2")
        .arg("-z")
        .arg("--branch")
        .arg("--untracked-files=all")
        .run_checked()?;
    parse_porcelain_v2(&output.stdout)
}

pub(crate) fn parse_porcelain_v2(bytes: &[u8]) -> Result<GitStatus, GitError> {
    let mut records = Vec::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        let Some(end) = rest.iter().position(|byte| *byte == 0) else {
            if rest.iter().all(u8::is_ascii_whitespace) {
                break;
            }
            return Err(GitError::InvalidOutput {
                reason: "porcelain v2 status was not NUL-terminated".to_owned(),
            });
        };
        let record = &rest[..end];
        rest = &rest[end + 1..];
        if record.is_empty() {
            continue;
        }
        records.push(record);
    }

    let mut status = GitStatus {
        branch: None,
        detached: false,
        unborn: false,
        ahead: None,
        behind: None,
        modified: Vec::new(),
        added: Vec::new(),
        deleted: Vec::new(),
        renamed: Vec::new(),
        untracked: Vec::new(),
        staged: Vec::new(),
        unstaged: Vec::new(),
        entries: Vec::new(),
    };

    let mut index = 0;
    while index < records.len() {
        let record = records[index];
        index += 1;
        if record.starts_with(b"# ") {
            parse_header(&mut status, record)?;
            continue;
        }
        if record == b"?" || record.starts_with(b"? ") {
            let path = path_after_prefix(record, b"? ")?;
            push_unique(&mut status.untracked, &path);
            status.entries.push(StatusEntry {
                path,
                original_path: None,
                staged: ChangeKind::Unmodified,
                unstaged: ChangeKind::Unmodified,
                untracked: true,
            });
            continue;
        }
        if record.starts_with(b"! ") {
            continue;
        }
        if record.starts_with(b"1 ") || record.starts_with(b"u ") {
            let entry = parse_ordinary(record)?;
            classify(&mut status, &entry);
            status.entries.push(entry);
            continue;
        }
        if record.starts_with(b"2 ") {
            let original = records.get(index).ok_or_else(|| GitError::InvalidOutput {
                reason: "rename record was missing the original path".to_owned(),
            })?;
            index += 1;
            let mut entry = parse_ordinary(record)?;
            let original_path = std::str::from_utf8(original)
                .map_err(|_| GitError::InvalidOutput {
                    reason: "rename original path was not UTF-8".to_owned(),
                })?
                .to_owned();
            entry.original_path = Some(original_path);
            if entry.staged == ChangeKind::Unmodified && entry.unstaged == ChangeKind::Unmodified {
                entry.staged = ChangeKind::Renamed;
            }
            classify(&mut status, &entry);
            status.entries.push(entry);
            continue;
        }
        return Err(GitError::InvalidOutput {
            reason: format!(
                "unrecognized porcelain v2 record: {}",
                String::from_utf8_lossy(record)
            ),
        });
    }

    Ok(status)
}

fn parse_header(status: &mut GitStatus, record: &[u8]) -> Result<(), GitError> {
    let line = std::str::from_utf8(record).map_err(|_| GitError::InvalidOutput {
        reason: "status header was not UTF-8".to_owned(),
    })?;
    if let Some(value) = line.strip_prefix("# branch.oid ") {
        if value == "(initial)" {
            status.unborn = true;
        }
        return Ok(());
    }
    if let Some(value) = line.strip_prefix("# branch.head ") {
        if value == "(detached)" {
            status.detached = true;
            status.branch = None;
        } else {
            status.branch = Some(value.to_owned());
        }
        return Ok(());
    }
    if let Some(value) = line.strip_prefix("# branch.ab ") {
        parse_ahead_behind(status, value)?;
    }
    Ok(())
}

fn parse_ahead_behind(status: &mut GitStatus, value: &str) -> Result<(), GitError> {
    let mut ahead = None;
    let mut behind = None;
    for token in value.split_whitespace() {
        if let Some(number) = token.strip_prefix('+') {
            ahead = Some(parse_count(number)?);
        } else if let Some(number) = token.strip_prefix('-') {
            behind = Some(parse_count(number)?);
        }
    }
    status.ahead = ahead;
    status.behind = behind;
    Ok(())
}

fn parse_count(value: &str) -> Result<u32, GitError> {
    value.parse().map_err(|_| GitError::InvalidOutput {
        reason: format!("invalid ahead/behind count {value:?}"),
    })
}

fn parse_ordinary(record: &[u8]) -> Result<StatusEntry, GitError> {
    let line = std::str::from_utf8(record).map_err(|_| GitError::InvalidOutput {
        reason: "status record was not UTF-8".to_owned(),
    })?;
    let mut parts = line.splitn(9, ' ');
    let kind = parts.next();
    let xy = parts.next().ok_or_else(|| GitError::InvalidOutput {
        reason: "status record was missing the XY field".to_owned(),
    })?;
    for _ in 0..6 {
        parts.next();
    }
    let path = if kind == Some("2") {
        parts
            .next()
            .and_then(|score_and_path| score_and_path.split_once(' '))
            .map(|(_, path)| path)
            .ok_or_else(|| GitError::InvalidOutput {
                reason: "rename record was missing the path".to_owned(),
            })?
    } else {
        parts.next().ok_or_else(|| GitError::InvalidOutput {
            reason: "status record was missing the path".to_owned(),
        })?
    };
    let mut chars = xy.chars();
    let staged = change_kind(chars.next().unwrap_or('.'));
    let unstaged = change_kind(chars.next().unwrap_or('.'));
    Ok(StatusEntry {
        path: path.to_owned(),
        original_path: None,
        staged,
        unstaged,
        untracked: false,
    })
}

fn change_kind(letter: char) -> ChangeKind {
    match letter {
        '.' => ChangeKind::Unmodified,
        'M' => ChangeKind::Modified,
        'A' => ChangeKind::Added,
        'D' => ChangeKind::Deleted,
        'R' => ChangeKind::Renamed,
        'C' => ChangeKind::Copied,
        'T' => ChangeKind::TypeChanged,
        'U' => ChangeKind::Unmerged,
        _ => ChangeKind::Unknown,
    }
}

fn classify(status: &mut GitStatus, entry: &StatusEntry) {
    if entry.staged != ChangeKind::Unmodified {
        push_unique(&mut status.staged, &entry.path);
    }
    if entry.unstaged != ChangeKind::Unmodified {
        push_unique(&mut status.unstaged, &entry.path);
    }
    match (entry.staged, entry.unstaged) {
        (ChangeKind::Modified, _) | (_, ChangeKind::Modified) => {
            push_unique(&mut status.modified, &entry.path);
        }
        (ChangeKind::Added, _) | (_, ChangeKind::Added) => {
            push_unique(&mut status.added, &entry.path);
        }
        (ChangeKind::Deleted, _) | (_, ChangeKind::Deleted) => {
            push_unique(&mut status.deleted, &entry.path);
        }
        (ChangeKind::Renamed, _) | (_, ChangeKind::Renamed) => {
            push_unique(&mut status.renamed, &entry.path);
        }
        _ => {}
    }
    if entry.original_path.is_some() {
        push_unique(&mut status.renamed, &entry.path);
    }
}

fn path_after_prefix(record: &[u8], prefix: &[u8]) -> Result<String, GitError> {
    let path = record.get(prefix.len()..).unwrap_or_default();
    std::str::from_utf8(path)
        .map(ToOwned::to_owned)
        .map_err(|_| GitError::InvalidOutput {
            reason: "untracked path was not UTF-8".to_owned(),
        })
}

fn push_unique(values: &mut Vec<String>, path: &str) {
    if !values.iter().any(|existing| existing == path) {
        values.push(path.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::{ChangeKind, parse_porcelain_v2};

    #[test]
    fn parses_branch_ahead_and_changed_files() {
        let mut bytes = Vec::new();
        for record in [
            "# branch.oid abcdef",
            "# branch.head main",
            "# branch.ab +2 -1",
            "1 M. N... 100644 100644 100644 0 0 src/lib.rs",
            "1 A. N... 100644 100644 100644 0 0 added.rs",
            "? untracked.txt",
        ] {
            bytes.extend_from_slice(record.as_bytes());
            bytes.push(0);
        }
        bytes.extend_from_slice(b"2 R. N... 100644 100644 100644 0 0 R100 new.rs");
        bytes.push(0);
        bytes.extend_from_slice(b"old.rs");
        bytes.push(0);

        let status = parse_porcelain_v2(&bytes).expect("status should parse");
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(status.ahead, Some(2));
        assert_eq!(status.behind, Some(1));
        assert_eq!(status.modified, ["src/lib.rs"]);
        assert_eq!(status.added, ["added.rs"]);
        assert_eq!(status.untracked, ["untracked.txt"]);
        assert_eq!(status.renamed, ["new.rs"]);
        assert_eq!(status.staged, ["src/lib.rs", "added.rs", "new.rs"]);
        assert!(status.entries.iter().any(|entry| {
            entry.path == "new.rs"
                && entry.original_path.as_deref() == Some("old.rs")
                && entry.staged == ChangeKind::Renamed
        }));
    }

    #[test]
    fn parses_detached_and_unborn_headers() {
        let mut bytes = Vec::new();
        for record in ["# branch.oid (initial)", "# branch.head (detached)"] {
            bytes.extend_from_slice(record.as_bytes());
            bytes.push(0);
        }
        let status = parse_porcelain_v2(&bytes).expect("status should parse");
        assert!(status.unborn);
        assert!(status.detached);
        assert!(status.branch.is_none());
        assert!(!status.is_dirty());
    }
}
