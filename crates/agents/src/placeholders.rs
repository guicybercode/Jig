use std::{collections::BTreeMap, error::Error, fmt};

/// Controlled placeholder names that may appear in structured agent fields.
pub const PROJECT_PATH: &str = "PROJECT_PATH";
/// Worktree directory for a session, when Git isolation is enabled.
pub const WORKTREE_PATH: &str = "WORKTREE_PATH";
/// Stable session identifier.
pub const SESSION_ID: &str = "SESSION_ID";
/// User-facing session name.
pub const SESSION_NAME: &str = "SESSION_NAME";

const KNOWN: [&str; 4] = [PROJECT_PATH, WORKTREE_PATH, SESSION_ID, SESSION_NAME];

/// Values substituted for known placeholders. Unknown names are never expanded.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PlaceholderContext {
    values: BTreeMap<String, String>,
}

impl PlaceholderContext {
    /// Creates an empty placeholder context.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets `${PROJECT_PATH}`.
    ///
    /// # Errors
    ///
    /// Returns an error if the value contains a NUL byte.
    pub fn with_project_path(self, value: impl Into<String>) -> Result<Self, PlaceholderError> {
        self.insert(PROJECT_PATH, value)
    }

    /// Sets `${WORKTREE_PATH}`.
    ///
    /// # Errors
    ///
    /// Returns an error if the value contains a NUL byte.
    pub fn with_worktree_path(self, value: impl Into<String>) -> Result<Self, PlaceholderError> {
        self.insert(WORKTREE_PATH, value)
    }

    /// Sets `${SESSION_ID}`.
    ///
    /// # Errors
    ///
    /// Returns an error if the value contains a NUL byte.
    pub fn with_session_id(self, value: impl Into<String>) -> Result<Self, PlaceholderError> {
        self.insert(SESSION_ID, value)
    }

    /// Sets `${SESSION_NAME}`.
    ///
    /// # Errors
    ///
    /// Returns an error if the value contains a NUL byte.
    pub fn with_session_name(self, value: impl Into<String>) -> Result<Self, PlaceholderError> {
        self.insert(SESSION_NAME, value)
    }

    /// Returns the value for a known placeholder name, if set.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    fn insert(mut self, name: &str, value: impl Into<String>) -> Result<Self, PlaceholderError> {
        let value = value.into();
        if value.contains('\0') {
            return Err(PlaceholderError::ContainsNul);
        }
        self.values.insert(name.to_owned(), value);
        Ok(self)
    }
}

/// Failure while expanding a controlled placeholder.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlaceholderError {
    /// `${` was not closed by `}`.
    Unclosed,
    /// `${}` did not contain a name.
    EmptyName,
    /// The name is not in the allow-list.
    Unknown {
        /// Placeholder name extracted from `${NAME}`.
        name: String,
    },
    /// The name is known but was not provided for this launch.
    Unavailable {
        /// Placeholder name that was not supplied.
        name: String,
    },
    /// A placeholder value contained a NUL byte.
    ContainsNul,
}

impl fmt::Display for PlaceholderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unclosed => formatter.write_str("placeholder is missing a closing '}'"),
            Self::EmptyName => formatter.write_str("placeholder name must not be empty"),
            Self::Unknown { name } => {
                write!(formatter, "unknown placeholder '${{{name}}}'")
            }
            Self::Unavailable { name } => {
                write!(
                    formatter,
                    "placeholder '${{{name}}}' is not available for this launch"
                )
            }
            Self::ContainsNul => {
                formatter.write_str("placeholder value must not contain a NUL byte")
            }
        }
    }
}

impl Error for PlaceholderError {}

/// Returns whether `value` contains at least one `${...}` placeholder.
#[must_use]
pub fn contains_placeholder(value: &str) -> bool {
    value.contains("${")
}

/// Expands `$$` into `$` and known `${NAME}` placeholders in a single pass.
///
/// The result is never passed to a shell. Callers must keep the expanded value
/// as one argument, environment value, path, or title string.
///
/// # Errors
///
/// Returns an error for unclosed placeholders, unknown names, or known names
/// that were not supplied in `context`.
pub fn expand(value: &str, context: &PlaceholderContext) -> Result<String, PlaceholderError> {
    let mut output = String::with_capacity(value.len());
    let mut rest = value;
    while !rest.is_empty() {
        if let Some(stripped) = rest.strip_prefix("$$") {
            output.push('$');
            rest = stripped;
            continue;
        }
        if let Some(stripped) = rest.strip_prefix("${") {
            let Some(end) = stripped.find('}') else {
                return Err(PlaceholderError::Unclosed);
            };
            let name = &stripped[..end];
            if name.is_empty() {
                return Err(PlaceholderError::EmptyName);
            }
            if !KNOWN.contains(&name) {
                return Err(PlaceholderError::Unknown {
                    name: name.to_owned(),
                });
            }
            let Some(replacement) = context.get(name) else {
                return Err(PlaceholderError::Unavailable {
                    name: name.to_owned(),
                });
            };
            output.push_str(replacement);
            rest = &stripped[end + 1..];
            continue;
        }
        let Some(character) = rest.chars().next() else {
            break;
        };
        output.push(character);
        rest = &rest[character.len_utf8()..];
    }
    Ok(output)
}

/// Expands placeholders in each argument independently.
///
/// # Errors
///
/// Returns the first expansion error.
pub fn expand_args(
    args: &[String],
    context: &PlaceholderContext,
) -> Result<Vec<String>, PlaceholderError> {
    args.iter()
        .map(|argument| expand(argument, context))
        .collect()
}

/// Expands placeholders in environment values. Keys are never expanded.
///
/// # Errors
///
/// Returns the first expansion error.
pub fn expand_env(
    env: &BTreeMap<String, String>,
    context: &PlaceholderContext,
) -> Result<BTreeMap<String, String>, PlaceholderError> {
    let mut expanded = BTreeMap::new();
    for (key, value) in env {
        expanded.insert(key.clone(), expand(value, context)?);
    }
    Ok(expanded)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> PlaceholderContext {
        PlaceholderContext::new()
            .with_project_path("/tmp/project")
            .expect("project path")
            .with_session_id("sess-1")
            .expect("session id")
            .with_session_name("Implement auth")
            .expect("session name")
    }

    #[test]
    fn expands_known_placeholders_and_escapes_dollars() {
        let expanded =
            expand("$$${PROJECT_PATH} ${SESSION_NAME}", &context()).expect("should expand");
        assert_eq!(expanded, "$/tmp/project Implement auth");
    }

    #[test]
    fn rejects_unknown_and_unavailable_placeholders() {
        assert!(matches!(
            expand("${HOME}", &PlaceholderContext::new()),
            Err(PlaceholderError::Unknown { name }) if name == "HOME"
        ));
        assert!(matches!(
            expand("${WORKTREE_PATH}", &context()),
            Err(PlaceholderError::Unavailable { name }) if name == "WORKTREE_PATH"
        ));
    }

    #[test]
    fn does_not_reexpand_replacement_values() {
        let context = PlaceholderContext::new()
            .with_project_path("/tmp/${SESSION_ID}")
            .expect("value may look like a placeholder");
        let expanded = expand("${PROJECT_PATH}", &context).expect("single pass");
        assert_eq!(expanded, "/tmp/${SESSION_ID}");
    }
}
