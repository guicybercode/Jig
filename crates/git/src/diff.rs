use std::path::Path;

use crate::{Git, GitError, GitErrorKind, os, repository};

/// A bounded textual Git diff.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diff {
    /// UTF-8 text. Invalid byte sequences are replaced lossily for display.
    pub text: String,
    /// Whether the configured byte limit omitted output.
    pub truncated: bool,
}

pub(crate) fn read(git: &Git, path: &Path, max_bytes: usize) -> Result<Diff, GitError> {
    if max_bytes == 0 {
        return Err(GitError::new(
            GitErrorKind::InvalidInput,
            "Diff byte limit must be greater than zero",
            "Use a positive diff byte limit",
        ));
    }
    let root = repository::require_root(git, path)?;
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
        os("--text"),
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
    let output = git.execute(Some(&root), arguments, max_bytes)?;
    if !output.success() {
        return Err(git.command_error("generate a diff", &output));
    }
    Ok(bounded_lossy_diff(
        &output.stdout,
        max_bytes,
        output.stdout_truncated,
    ))
}

fn bounded_lossy_diff(bytes: &[u8], max_bytes: usize, source_truncated: bool) -> Diff {
    let text = String::from_utf8_lossy(bytes);
    if text.len() <= max_bytes {
        return Diff {
            text: text.into_owned(),
            truncated: source_truncated,
        };
    }
    let mut boundary = max_bytes;
    while boundary > 0 && !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    Diff {
        text: text[..boundary].to_owned(),
        truncated: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lossy_conversion_never_exceeds_byte_limit_or_splits_utf8() {
        let diff = bounded_lossy_diff(b"ab\xffcd", 4, false);
        assert!(diff.text.is_char_boundary(diff.text.len()));
        assert!(diff.text.len() <= 4);
        assert_eq!(diff.text, "ab");
        assert!(diff.truncated);
    }
}
