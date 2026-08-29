use std::path::Path;

use crate::{Git, GitError, GitErrorKind, os, pathspec, repository};

/// Default IPC diff cap: 2 MiB of Git stdout.
pub const MAX_DIFF_BYTES: usize = 2 * 1024 * 1024;

/// A bounded textual Git diff.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diff {
    /// UTF-8 text. Invalid byte sequences are replaced lossily for display.
    pub text: String,
    /// Whether the configured byte limit omitted output.
    pub truncated: bool,
    /// Whether Git reported binary content instead of a textual patch.
    pub binary: bool,
}

pub(crate) fn read(git: &Git, path: &Path, max_bytes: usize) -> Result<Diff, GitError> {
    read_with_pathspec(git, path, None, max_bytes)
}

pub(crate) fn read_path(
    git: &Git,
    path: &Path,
    pathspec: &Path,
    max_bytes: usize,
) -> Result<Diff, GitError> {
    read_with_pathspec(git, path, Some(pathspec), max_bytes)
}

fn read_with_pathspec(
    git: &Git,
    path: &Path,
    pathspec: Option<&Path>,
    max_bytes: usize,
) -> Result<Diff, GitError> {
    if max_bytes == 0 {
        return Err(GitError::new(
            GitErrorKind::InvalidInput,
            "Diff byte limit must be greater than zero",
            "Use a positive diff byte limit",
        ));
    }
    let root = repository::require_root(git, path)?;
    if let Some(pathspec) = pathspec {
        pathspec::ensure_pathspec_inside_root(&root, pathspec)?;
    }
    let head = git.execute(
        Some(&root),
        [os("rev-parse"), os("--verify"), os("--quiet"), os("HEAD")],
        64 * 1024,
    )?;
    let has_head = match head.status.code() {
        Some(0) => true,
        Some(1) => false,
        _ => return Err(git.command_error("inspect diff base", &head)),
    };
    let mut arguments = vec![
        os("-c"),
        os("color.ui=false"),
        os("diff"),
        os("--no-color"),
        os("--no-ext-diff"),
        os("--no-textconv"),
    ];
    if has_head {
        arguments.push(os("HEAD"));
    } else {
        // With an unborn HEAD, `--cached` compares the index to Git's empty
        // tree and therefore exposes staged initial content without requiring a
        // hash-algorithm-specific empty-tree object ID.
        arguments.push(os("--cached"));
    }
    arguments.push(os("--"));
    if let Some(pathspec) = pathspec {
        arguments.push(pathspec.as_os_str().to_os_string());
    }
    let output = git.execute(Some(&root), arguments, max_bytes)?;
    if !output.success() {
        return Err(git.command_error("generate a diff", &output));
    }
    Ok(bounded_diff(
        &output.stdout,
        max_bytes,
        output.stdout_truncated,
    ))
}

fn bounded_diff(bytes: &[u8], max_bytes: usize, source_truncated: bool) -> Diff {
    let binary = is_binary_diff(bytes);
    if binary && bytes.contains(&0) {
        return Diff {
            text: String::new(),
            truncated: source_truncated || bytes.len() >= max_bytes,
            binary: true,
        };
    }
    let text = String::from_utf8_lossy(bytes);
    if text.len() <= max_bytes {
        return Diff {
            text: text.into_owned(),
            truncated: source_truncated,
            binary,
        };
    }
    let mut boundary = max_bytes;
    while boundary > 0 && !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    Diff {
        text: text[..boundary].to_owned(),
        truncated: true,
        binary,
    }
}

fn is_binary_diff(bytes: &[u8]) -> bool {
    bytes.contains(&0)
        || contains_marker(bytes, b"Binary files ")
        || contains_marker(bytes, b"GIT binary patch")
}

fn contains_marker(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lossy_conversion_never_exceeds_byte_limit_or_splits_utf8() {
        let diff = bounded_diff(b"ab\xffcd", 4, false);
        assert!(diff.text.is_char_boundary(diff.text.len()));
        assert!(diff.text.len() <= 4);
        assert_eq!(diff.text, "ab");
        assert!(diff.truncated);
        assert!(!diff.binary);
    }

    #[test]
    fn binary_marker_is_preserved_as_text_without_file_bytes() {
        let diff = bounded_diff(
            b"Binary files a/blob.bin and b/blob.bin differ\n",
            64,
            false,
        );
        assert!(diff.binary);
        assert!(diff.text.contains("Binary files"));
        assert!(!diff.truncated);
    }

    #[test]
    fn nul_bytes_are_not_copied_into_the_wire_text() {
        let diff = bounded_diff(b"\x00\xff\x00secret", 32, false);
        assert!(diff.binary);
        assert!(diff.text.is_empty());
    }
}
