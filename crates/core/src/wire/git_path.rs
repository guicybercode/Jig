use std::path::Path;

use serde::{Deserialize, Deserializer, Serialize, de};

use super::value::WireValidationError;

const MAX_GIT_PATHSPEC_BYTES: usize = 1_024;

/// Repository-relative Git pathspec accepted on the wire.
///
/// This is never an absolute filesystem path. Traversal components and values
/// that Git would parse as options are rejected before the daemon invokes Git.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct GitRelativePath(String);

impl GitRelativePath {
    /// Validates a repository-relative pathspec from a Git inspection request.
    ///
    /// # Errors
    ///
    /// Returns an error for empty values, absolute paths, `.` / `..` components,
    /// backslashes, control characters, or option-like names such as `-u` or `--`.
    pub fn try_new(value: impl Into<String>) -> Result<Self, WireValidationError> {
        let value = value.into();
        if value.is_empty() {
            return Err(WireValidationError::new("path", "must not be empty"));
        }
        if value.len() > MAX_GIT_PATHSPEC_BYTES {
            return Err(WireValidationError::new(
                "path",
                "must be at most 1024 UTF-8 bytes",
            ));
        }
        if value.starts_with('/') || value.starts_with('\\') || value.contains('\\') {
            return Err(WireValidationError::new(
                "path",
                "must use a relative forward-slash path",
            ));
        }
        if value.chars().any(char::is_control) {
            return Err(WireValidationError::new(
                "path",
                "must not contain control characters",
            ));
        }
        if value
            .split('/')
            .any(|component| component.is_empty() || matches!(component, "." | ".." | "--"))
        {
            return Err(WireValidationError::new(
                "path",
                "must contain only non-empty child components",
            ));
        }
        if value.split('/').any(|component| component.starts_with('-')) {
            return Err(WireValidationError::new(
                "path",
                "must not look like a Git option",
            ));
        }
        if value
            .split('/')
            .next()
            .is_some_and(|component| component.ends_with(':'))
        {
            return Err(WireValidationError::new(
                "path",
                "must not contain a platform path prefix",
            ));
        }
        Ok(Self(value))
    }

    /// Returns the validated pathspec as UTF-8 text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the validated pathspec as a platform path.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }
}

impl<'de> Deserialize<'de> for GitRelativePath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::try_new(value).map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::GitRelativePath;

    #[test]
    fn accepts_nested_repository_relative_files() {
        assert_eq!(
            GitRelativePath::try_new("src/lib.rs").unwrap().as_str(),
            "src/lib.rs"
        );
        assert!(GitRelativePath::try_new(".gitignore").is_ok());
        assert!(GitRelativePath::try_new("weird name.txt").is_ok());
    }

    #[test]
    fn rejects_traversal_absolute_and_option_like_values() {
        for invalid in [
            "",
            "/tmp/secret",
            "../outside",
            "a/../b",
            "./hidden",
            "--",
            "-u",
            "src/-cached",
            "C:/windows",
            "a\\b",
            "line\nbreak",
        ] {
            assert!(GitRelativePath::try_new(invalid).is_err(), "{invalid}");
        }
    }
}
