use std::{error::Error, fmt, path::Path};

use serde::{Deserialize, Deserializer, Serialize, de};

/// Maximum UTF-8 byte length accepted for user-facing names.
pub const MAX_DISPLAY_NAME_BYTES: usize = 256;
/// Maximum UTF-8 byte length accepted for a project-relative directory.
pub const MAX_RELATIVE_DIRECTORY_BYTES: usize = 1_024;
/// Maximum decoded PTY output bytes carried by one event.
pub const MAX_PTY_OUTPUT_BYTES: usize = 8 * 1_024;
/// Maximum decoded PTY input bytes accepted by one request.
pub const MAX_PTY_INPUT_BYTES: usize = 64 * 1_024;
/// Maximum row or column count accepted for a PTY resize.
pub const MAX_TERMINAL_DIMENSION: u16 = 4_096;

const MAX_PROJECT_PATH_BYTES: usize = 4_096;
const MAX_EXECUTABLE_BYTES: usize = 4_096;
const MIN_CONFIRMATION_TOKEN_BYTES: usize = 16;
const MAX_CONFIRMATION_TOKEN_BYTES: usize = 256;

/// Validation failure for a value received at the IPC boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WireValidationError {
    field: &'static str,
    message: &'static str,
}

impl WireValidationError {
    pub(crate) const fn new(field: &'static str, message: &'static str) -> Self {
        Self { field, message }
    }

    /// Returns the stable field label associated with the failure.
    #[must_use]
    pub const fn field(&self) -> &'static str {
        self.field
    }

    /// Returns a safe explanation suitable for an invalid-request response.
    #[must_use]
    pub const fn message(&self) -> &'static str {
        self.message
    }
}

impl fmt::Display for WireValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} {}", self.field, self.message)
    }
}

impl Error for WireValidationError {}

/// Canonical non-blank user-facing project, agent, or session name.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct DisplayName(String);

impl DisplayName {
    /// Trims and validates a user-facing name.
    ///
    /// # Errors
    ///
    /// Returns an error for a blank, oversized, NUL-containing, or control-character name.
    pub fn try_new(value: impl Into<String>) -> Result<Self, WireValidationError> {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(WireValidationError::new("name", "must not be blank"));
        }
        if trimmed.len() > MAX_DISPLAY_NAME_BYTES {
            return Err(WireValidationError::new(
                "name",
                "must be at most 256 UTF-8 bytes",
            ));
        }
        if trimmed.chars().any(char::is_control) {
            return Err(WireValidationError::new(
                "name",
                "must not contain control characters",
            ));
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// Returns the canonical name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes this value and returns the canonical name.
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl<'de> Deserialize<'de> for DisplayName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::try_new(value).map_err(de::Error::custom)
    }
}

/// Absolute local path selected while registering a project.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct SelectedProjectPath(String);

impl SelectedProjectPath {
    /// Validates an absolute UTF-8 path selected by the user.
    ///
    /// The daemon still canonicalizes the path and verifies repository state.
    ///
    /// # Errors
    ///
    /// Returns an error for a non-absolute, empty, oversized, or NUL-containing path.
    pub fn try_new(value: impl Into<String>) -> Result<Self, WireValidationError> {
        let value = value.into();
        if value.is_empty() {
            return Err(WireValidationError::new("path", "must not be empty"));
        }
        if value.len() > MAX_PROJECT_PATH_BYTES {
            return Err(WireValidationError::new(
                "path",
                "must be at most 4096 UTF-8 bytes",
            ));
        }
        if value.contains('\0') {
            return Err(WireValidationError::new(
                "path",
                "must not contain a NUL byte",
            ));
        }
        if !Path::new(&value).is_absolute() {
            return Err(WireValidationError::new("path", "must be absolute"));
        }
        Ok(Self(value))
    }

    /// Returns the selected path as UTF-8 text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for SelectedProjectPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::try_new(value).map_err(de::Error::custom)
    }
}

/// Safe project-relative directory chosen for a session.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct RelativeDirectory(String);

impl RelativeDirectory {
    /// Validates a slash-separated directory below the daemon-selected session root.
    ///
    /// # Errors
    ///
    /// Returns an error for absolute paths, empty components, `.` or `..`, backslashes,
    /// control characters, or values exceeding the wire limit.
    pub fn try_new(value: impl Into<String>) -> Result<Self, WireValidationError> {
        let value = value.into();
        if value.is_empty() {
            return Err(WireValidationError::new(
                "relative_directory",
                "must not be empty",
            ));
        }
        if value.len() > MAX_RELATIVE_DIRECTORY_BYTES {
            return Err(WireValidationError::new(
                "relative_directory",
                "must be at most 1024 UTF-8 bytes",
            ));
        }
        if value.starts_with('/') || value.starts_with('\\') || value.contains('\\') {
            return Err(WireValidationError::new(
                "relative_directory",
                "must use a relative forward-slash path",
            ));
        }
        if value.chars().any(char::is_control) {
            return Err(WireValidationError::new(
                "relative_directory",
                "must not contain control characters",
            ));
        }
        if value
            .split('/')
            .any(|component| component.is_empty() || matches!(component, "." | ".."))
        {
            return Err(WireValidationError::new(
                "relative_directory",
                "must contain only non-empty child components",
            ));
        }
        if value
            .split('/')
            .next()
            .is_some_and(|component| component.ends_with(':'))
        {
            return Err(WireValidationError::new(
                "relative_directory",
                "must not contain a platform path prefix",
            ));
        }
        Ok(Self(value))
    }

    /// Returns the normalized relative directory.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for RelativeDirectory {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::try_new(value).map_err(de::Error::custom)
    }
}

/// Bare executable name or absolute executable path for a custom agent.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ExecutableName(String);

impl ExecutableName {
    pub(crate) const fn from_validated(value: String) -> Self {
        Self(value)
    }

    /// Validates a structured executable value that does not invoke a shell.
    ///
    /// # Errors
    ///
    /// Returns an error for blank, oversized, NUL-containing, or relative nested paths.
    pub fn try_new(value: impl Into<String>) -> Result<Self, WireValidationError> {
        let value = value.into();
        if value.trim().is_empty() || value != value.trim() {
            return Err(WireValidationError::new(
                "executable",
                "must be non-blank without surrounding whitespace",
            ));
        }
        if value.len() > MAX_EXECUTABLE_BYTES {
            return Err(WireValidationError::new(
                "executable",
                "must be at most 4096 UTF-8 bytes",
            ));
        }
        if value.contains('\0') {
            return Err(WireValidationError::new(
                "executable",
                "must not contain a NUL byte",
            ));
        }
        let path = Path::new(&value);
        if !path.is_absolute() && (value.contains('/') || value.contains('\\')) {
            return Err(WireValidationError::new(
                "executable",
                "must be an absolute path or bare executable name",
            ));
        }
        Ok(Self(value))
    }

    /// Returns the executable name or path.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ExecutableName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::try_new(value).map_err(de::Error::custom)
    }
}

/// Opaque short-lived token authorizing a previously prepared worktree removal.
#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ConfirmationToken(String);

impl ConfirmationToken {
    /// Validates a base64url-like opaque confirmation token.
    ///
    /// # Errors
    ///
    /// Returns an error when length or character constraints are violated.
    pub fn try_new(value: impl Into<String>) -> Result<Self, WireValidationError> {
        let value = value.into();
        if !(MIN_CONFIRMATION_TOKEN_BYTES..=MAX_CONFIRMATION_TOKEN_BYTES).contains(&value.len()) {
            return Err(WireValidationError::new(
                "confirmation_token",
                "must contain between 16 and 256 ASCII bytes",
            ));
        }
        if !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~'))
        {
            return Err(WireValidationError::new(
                "confirmation_token",
                "contains unsupported characters",
            ));
        }
        Ok(Self(value))
    }

    /// Returns the opaque token for equality verification by the daemon.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ConfirmationToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConfirmationToken")
            .field("length", &self.0.len())
            .finish()
    }
}

impl<'de> Deserialize<'de> for ConfirmationToken {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::try_new(value).map_err(de::Error::custom)
    }
}

macro_rules! base64_payload {
    ($name:ident, $doc:literal, $field:literal, $max:expr) => {
        #[doc = $doc]
        #[derive(Clone, Eq, PartialEq, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Validates canonical padded base64 without decoding terminal contents.
            ///
            /// # Errors
            ///
            /// Returns an error for malformed base64, an empty payload, or a decoded
            /// length above the payload-specific limit.
            pub fn try_new(value: impl Into<String>) -> Result<Self, WireValidationError> {
                let value = value.into();
                validate_base64($field, &value, $max)?;
                Ok(Self(value))
            }

            /// Returns the validated encoded bytes.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Consumes this value and returns the validated encoded bytes.
            #[must_use]
            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_struct(stringify!($name))
                    .field("encoded_length", &self.0.len())
                    .finish()
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::try_new(value).map_err(de::Error::custom)
            }
        }
    };
}

base64_payload!(
    PtyInputBase64,
    "Canonical base64 PTY input bounded to 64 KiB after decoding.",
    "base64",
    MAX_PTY_INPUT_BYTES
);
base64_payload!(
    PtyOutputBase64,
    "Canonical base64 PTY output bounded to 8 KiB after decoding.",
    "base64",
    MAX_PTY_OUTPUT_BYTES
);

/// Non-zero bounded PTY row or column count.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct TerminalDimension(u16);

impl TerminalDimension {
    /// Validates a PTY dimension.
    ///
    /// # Errors
    ///
    /// Returns an error for zero or a value above [`MAX_TERMINAL_DIMENSION`].
    pub const fn try_new(value: u16) -> Result<Self, WireValidationError> {
        if value == 0 || value > MAX_TERMINAL_DIMENSION {
            Err(WireValidationError::new(
                "terminal_dimension",
                "must be between 1 and 4096",
            ))
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the validated row or column count.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl<'de> Deserialize<'de> for TerminalDimension {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u16::deserialize(deserializer)?;
        Self::try_new(value).map_err(de::Error::custom)
    }
}

/// Last terminal output sequence observed by a reconnecting subscriber.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct OutputCursor(u64);

impl OutputCursor {
    /// Wraps a monotonically increasing terminal output sequence.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the sequence value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Monotonic sequence assigned to one emitted terminal output chunk.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct OutputSequence(u64);

impl OutputSequence {
    /// Wraps a terminal output sequence assigned by the daemon.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the sequence value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

fn validate_base64(
    field: &'static str,
    value: &str,
    maximum_decoded_bytes: usize,
) -> Result<(), WireValidationError> {
    let maximum_encoded_bytes = maximum_decoded_bytes.div_ceil(3) * 4;
    if value.is_empty() {
        return Err(WireValidationError::new(field, "must not be empty"));
    }
    if value.len() > maximum_encoded_bytes {
        return Err(WireValidationError::new(
            field,
            "exceeds the decoded byte limit",
        ));
    }
    if value.len() % 4 != 0 {
        return Err(WireValidationError::new(
            field,
            "must use canonical padded base64",
        ));
    }

    let padding = value.bytes().rev().take_while(|byte| *byte == b'=').count();
    if padding > 2 {
        return Err(WireValidationError::new(
            field,
            "has invalid base64 padding",
        ));
    }
    let content_length = value.len() - padding;
    if !value.as_bytes()[..content_length]
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/'))
        || !value.as_bytes()[content_length..]
            .iter()
            .all(|byte| *byte == b'=')
    {
        return Err(WireValidationError::new(
            field,
            "contains invalid base64 characters",
        ));
    }
    let has_canonical_tail = match padding {
        2 => base64_sextet(value.as_bytes()[content_length - 1])
            .is_some_and(|value| value.trailing_zeros() >= 4),
        1 => base64_sextet(value.as_bytes()[content_length - 1])
            .is_some_and(|value| value.trailing_zeros() >= 2),
        _ => true,
    };
    if !has_canonical_tail {
        return Err(WireValidationError::new(
            field,
            "must use canonical base64 padding bits",
        ));
    }

    let decoded_length = (value.len() / 4) * 3 - padding;
    if decoded_length > maximum_decoded_bytes {
        return Err(WireValidationError::new(
            field,
            "exceeds the decoded byte limit",
        ));
    }
    Ok(())
}

fn base64_sextet(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_trimmed_and_control_characters_are_rejected() {
        assert_eq!(
            DisplayName::try_new("  Work item  ").unwrap().as_str(),
            "Work item"
        );
        assert!(DisplayName::try_new("line\nbreak").is_err());
    }

    #[test]
    fn relative_directory_rejects_parent_and_platform_prefixes() {
        assert!(RelativeDirectory::try_new("packages/ui").is_ok());
        for unsafe_path in ["../outside", "a/../outside", "/tmp/elsewhere", "C:/outside"] {
            assert!(
                RelativeDirectory::try_new(unsafe_path).is_err(),
                "{unsafe_path}"
            );
        }
    }

    #[test]
    fn base64_validation_enforces_shape_and_decoded_limits() {
        assert_eq!(PtyInputBase64::try_new("Aw==").unwrap().as_str(), "Aw==");
        for invalid in ["", "A", "A===", "ab=c", "YW Jj", "AB==", "AAB="] {
            assert!(PtyInputBase64::try_new(invalid).is_err(), "{invalid}");
        }

        let at_limit = "A".repeat((MAX_PTY_OUTPUT_BYTES / 3) * 4) + "AAA=";
        assert_eq!(at_limit.len(), MAX_PTY_OUTPUT_BYTES.div_ceil(3) * 4);
        assert!(PtyOutputBase64::try_new(at_limit).is_ok());
        let over_limit = "A".repeat(MAX_PTY_OUTPUT_BYTES.div_ceil(3) * 4);
        assert!(PtyOutputBase64::try_new(format!("{over_limit}AAAA")).is_err());

        let input_at_limit = "AAAA".repeat(MAX_PTY_INPUT_BYTES / 3)
            + if MAX_PTY_INPUT_BYTES % 3 == 1 {
                "AA=="
            } else {
                ""
            };
        assert!(PtyInputBase64::try_new(input_at_limit).is_ok());
        let input_over_limit = "AAAA".repeat(MAX_PTY_INPUT_BYTES.div_ceil(3));
        assert!(PtyInputBase64::try_new(input_over_limit).is_err());
    }

    #[test]
    fn terminal_dimensions_reject_zero_and_excessive_values() {
        assert!(TerminalDimension::try_new(1).is_ok());
        assert!(TerminalDimension::try_new(MAX_TERMINAL_DIMENSION).is_ok());
        assert!(TerminalDimension::try_new(0).is_err());
        assert!(TerminalDimension::try_new(MAX_TERMINAL_DIMENSION + 1).is_err());
    }

    #[test]
    fn opaque_values_redact_contents_from_debug() {
        let token = ConfirmationToken::try_new("abcdefghijklmnop").unwrap();
        let input = PtyInputBase64::try_new("c2VjcmV0").unwrap();
        assert!(!format!("{token:?}").contains("abcdefghijklmnop"));
        assert!(!format!("{input:?}").contains("c2VjcmV0"));
    }
}
