use std::collections::BTreeSet;

use cli_master_core::SessionId;
use unicode_normalization::UnicodeNormalization;

use crate::error::GitError;

const BRANCH_PREFIX: &str = "agent/";
const MAX_SLUG_LEN: usize = 48;
const MAX_NUMERIC_SUFFIX: u32 = 32;
const FALLBACK_SLUG: &str = "session";

const RESERVED_NAMES: &[&str] = &[
    ".",
    "..",
    "head",
    "fetch_head",
    "orig_head",
    "merge_head",
    "cherry_pick_head",
    "git",
    "refs",
    "objects",
    "hooks",
    "info",
    "con",
    "prn",
    "aux",
    "nul",
    "com1",
    "com2",
    "com3",
    "com4",
    "com5",
    "com6",
    "com7",
    "com8",
    "com9",
    "lpt1",
    "lpt2",
    "lpt3",
    "lpt4",
    "lpt5",
    "lpt6",
    "lpt7",
    "lpt8",
    "lpt9",
];

/// Branch and directory names allocated for one session worktree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllocatedNames {
    /// Generated branch, including the `agent/` prefix.
    pub branch: String,
    /// Directory leaf under the managed project worktree folder.
    pub directory: String,
}

/// Turns a session title into a lowercase ASCII slug.
#[must_use]
pub fn slugify(input: &str) -> String {
    let mut slug = String::new();
    let mut pending_hyphen = false;

    for character in input.to_lowercase().nfkd() {
        if character.is_ascii_alphanumeric() {
            if pending_hyphen && !slug.is_empty() {
                slug.push('-');
            }
            pending_hyphen = false;
            slug.push(character);
            if slug.len() >= MAX_SLUG_LEN {
                break;
            }
            continue;
        }

        if character.is_whitespace()
            || matches!(character, '-' | '_' | '/' | '\\' | '.' | ':' | '@')
        {
            pending_hyphen = !slug.is_empty();
        }
    }

    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.len() > MAX_SLUG_LEN {
        slug.truncate(MAX_SLUG_LEN);
        while slug.ends_with('-') {
            slug.pop();
        }
    }

    if slug.is_empty() || is_reserved(&slug) {
        FALLBACK_SLUG.to_owned()
    } else {
        slug
    }
}

/// Last eight hex characters of the session UUID, used as a stable suffix.
#[must_use]
pub fn session_suffix(session_id: SessionId) -> String {
    let hex = session_id.as_uuid().as_simple().to_string();
    hex[24..32].to_owned()
}

/// Allocates a unique branch and directory from a session name.
///
/// # Errors
///
/// Returns [`GitError::NameInvalid`] when no safe unique name can be produced.
pub fn allocate_names(
    session_name: &str,
    session_id: SessionId,
    taken_branches: &BTreeSet<String>,
    taken_directories: &BTreeSet<String>,
) -> Result<AllocatedNames, GitError> {
    let slug = slugify(session_name);
    let suffix = session_suffix(session_id);
    let mut candidates = vec![
        (format!("{BRANCH_PREFIX}{slug}"), slug.clone()),
        (
            format!("{BRANCH_PREFIX}{slug}-{suffix}"),
            format!("{slug}-{suffix}"),
        ),
    ];
    for index in 2..=MAX_NUMERIC_SUFFIX {
        candidates.push((
            format!("{BRANCH_PREFIX}{slug}-{suffix}-{index}"),
            format!("{slug}-{suffix}-{index}"),
        ));
    }

    for (branch, directory) in candidates {
        if !contains_ignore_ascii_case(taken_branches, &branch)
            && !contains_ignore_ascii_case(taken_directories, &directory)
            && is_safe_branch(&branch)
        {
            return Ok(AllocatedNames { branch, directory });
        }
    }

    Err(GitError::NameInvalid {
        session_name: session_name.to_owned(),
    })
}

fn contains_ignore_ascii_case(set: &BTreeSet<String>, value: &str) -> bool {
    set.iter()
        .any(|existing| existing.eq_ignore_ascii_case(value))
}

fn is_reserved(slug: &str) -> bool {
    RESERVED_NAMES
        .iter()
        .any(|reserved| slug.eq_ignore_ascii_case(reserved))
}

fn is_safe_branch(branch: &str) -> bool {
    !branch.is_empty()
        && !branch.starts_with('-')
        && !branch.ends_with('.')
        && !branch.to_ascii_lowercase().ends_with(".lock")
        && !branch.contains("..")
        && !branch.contains("//")
        && !branch.contains("@{")
        && branch
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '/'))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use cli_master_core::SessionId;

    use super::{allocate_names, session_suffix, slugify};

    #[test]
    fn slugs_oauth_example() {
        assert_eq!(
            slugify("Implement OAuth Refresh"),
            "implement-oauth-refresh"
        );
    }

    #[test]
    fn slugs_unicode_and_path_traversal() {
        assert_eq!(slugify("Implementação OAuth"), "implementacao-oauth");
        assert_eq!(slugify("../../../etc/passwd"), "etc-passwd");
        assert_eq!(slugify("!!!"), "session");
        assert_eq!(slugify("CON"), "session");
        assert_eq!(slugify("HEAD"), "session");
    }

    #[test]
    fn allocates_plain_slug_then_session_suffix() {
        let id = SessionId::new();
        let suffix = session_suffix(id);
        let first = allocate_names(
            "Implement OAuth Refresh",
            id,
            &BTreeSet::new(),
            &BTreeSet::new(),
        )
        .expect("first name");
        assert_eq!(first.branch, "agent/implement-oauth-refresh");
        assert_eq!(first.directory, "implement-oauth-refresh");

        let taken_branches = BTreeSet::from([first.branch.clone()]);
        let taken_directories = BTreeSet::from([first.directory.clone()]);
        let second = allocate_names(
            "Implement OAuth Refresh",
            id,
            &taken_branches,
            &taken_directories,
        )
        .expect("second name");
        assert_eq!(
            second.branch,
            format!("agent/implement-oauth-refresh-{suffix}")
        );
        assert_eq!(
            second.directory,
            format!("implement-oauth-refresh-{suffix}")
        );
    }
}
