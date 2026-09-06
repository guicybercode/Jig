//! Pure local file identifiers and editor request/response contracts.

use std::fmt;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Deserializer, Serialize, de};

use super::{GitTarget, WireValidationError};

/// Maximum decoded length of a relative Unix file identifier.
pub const MAX_FILE_PATH_BYTES: usize = 4_096;
/// Maximum UTF-8 byte length accepted by the first text-editor slice.
pub const MAX_FILE_TEXT_BYTES: usize = 128 * 1_024;
/// Default number of entries requested in one directory page.
pub const DEFAULT_FILE_LIST_LIMIT: u16 = 100;
/// Maximum number of entries requested in one directory page.
pub const MAX_FILE_LIST_LIMIT: u16 = 200;

/// A registered project, session or worktree whose root the daemon resolves.
pub type FileTarget = GitTarget;

/// Byte-exact relative Unix path represented as canonical padded base64.
///
/// Empty bytes identify the target root for directory listing only. This type
/// does not interpret Unix filenames as Git pathspecs or Windows paths.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct FilePath {
    encoded: String,
    #[serde(skip)]
    bytes: Vec<u8>,
}

impl FilePath {
    /// Validates an encoded relative path without accessing the filesystem.
    ///
    /// # Errors
    ///
    /// Rejects noncanonical base64, oversized paths, NUL, absolute paths, empty
    /// components and the traversal components `.` or `..`.
    pub fn try_new(encoded: impl Into<String>) -> Result<Self, WireValidationError> {
        let encoded = encoded.into();
        if encoded.len() > MAX_FILE_PATH_BYTES.div_ceil(3) * 4 {
            return Err(path_error("must decode to at most 4096 bytes"));
        }
        let bytes = STANDARD
            .decode(&encoded)
            .map_err(|_| path_error("must use canonical padded base64"))?;
        if STANDARD.encode(&bytes) != encoded {
            return Err(path_error("must use canonical padded base64"));
        }
        validate_path_bytes(&bytes)?;
        Ok(Self { encoded, bytes })
    }

    /// Validates exact Unix path bytes and encodes their wire identifier.
    ///
    /// # Errors
    ///
    /// Returns an error for a path violating the relative path invariants.
    pub fn try_from_bytes(bytes: impl AsRef<[u8]>) -> Result<Self, WireValidationError> {
        let bytes = bytes.as_ref();
        validate_path_bytes(bytes)?;
        Ok(Self {
            encoded: STANDARD.encode(bytes),
            bytes: bytes.to_vec(),
        })
    }

    /// Returns the root identifier accepted by directory listing.
    #[must_use]
    pub fn root() -> Self {
        Self {
            encoded: String::new(),
            bytes: Vec::new(),
        }
    }

    /// Returns the exact decoded bytes; display text must not replace them.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the canonical base64 wire value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.encoded
    }

    /// Returns whether this path refers to the registered root.
    #[must_use]
    pub fn is_root(&self) -> bool {
        self.bytes.is_empty()
    }
}

impl<'de> Deserialize<'de> for FilePath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_new(String::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

/// A nonempty single Unix filename used as a directory pagination cursor.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct FileName(FilePath);

impl FileName {
    /// Validates a canonical base64 filename without directory components.
    ///
    /// # Errors
    ///
    /// Rejects invalid paths, the root, and names containing a slash.
    pub fn try_new(encoded: impl Into<String>) -> Result<Self, WireValidationError> {
        Self::from_path(FilePath::try_new(encoded)?)
    }

    /// Encodes and validates an exact Unix filename.
    ///
    /// # Errors
    ///
    /// Rejects invalid paths, the root, and names containing a slash.
    pub fn try_from_bytes(bytes: impl AsRef<[u8]>) -> Result<Self, WireValidationError> {
        Self::from_path(FilePath::try_from_bytes(bytes)?)
    }

    fn from_path(path: FilePath) -> Result<Self, WireValidationError> {
        if path.is_root() || path.as_bytes().contains(&b'/') {
            return Err(WireValidationError::new(
                "afterNameBase64",
                "must identify one nonempty filename",
            ));
        }
        Ok(Self(path))
    }

    /// Returns exact filename bytes for lexicographic cursor comparisons.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    /// Returns the canonical base64 cursor.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl<'de> Deserialize<'de> for FileName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_new(String::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

/// Opaque versioned content/identity digest produced by the daemon.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct FileRevision(String);

impl FileRevision {
    /// Validates the wire shape without inspecting or hashing a file.
    ///
    /// # Errors
    ///
    /// Requires the exact prefix `v1:` followed by 64 lowercase hex digits.
    pub fn try_new(value: impl Into<String>) -> Result<Self, WireValidationError> {
        let value = value.into();
        if value.len() != 67
            || !value.starts_with("v1:")
            || !value.as_bytes()[3..]
                .iter()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
        {
            return Err(WireValidationError::new(
                "revision",
                "must use v1: followed by 64 lowercase hexadecimal digits",
            ));
        }
        Ok(Self(value))
    }

    /// Returns the opaque revision string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for FileRevision {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_new(String::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

/// Request to list one registered target directory with a bounded page size.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileListRequest {
    /// Registered target whose authoritative root the daemon resolves.
    pub target: FileTarget,
    /// Relative directory; an empty identifier means the target root.
    pub path_base64: FilePath,
    /// Requested page size, between one and 200 inclusive.
    pub limit: u16,
    /// Exclusive cursor compared using exact Unix filename bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_name_base64: Option<FileName>,
}

impl FileListRequest {
    /// Builds a bounded directory request, using 100 when the limit is absent.
    ///
    /// # Errors
    ///
    /// Rejects a zero page size or a size above 200 entries.
    pub fn try_new(
        target: FileTarget,
        path_base64: FilePath,
        limit: Option<u16>,
        after_name_base64: Option<FileName>,
    ) -> Result<Self, WireValidationError> {
        let limit = limit.unwrap_or(DEFAULT_FILE_LIST_LIMIT);
        if !(1..=MAX_FILE_LIST_LIMIT).contains(&limit) {
            return Err(WireValidationError::new(
                "limit",
                "must be between 1 and 200",
            ));
        }
        Ok(Self {
            target,
            path_base64,
            limit,
            after_name_base64,
        })
    }
}

impl<'de> Deserialize<'de> for FileListRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        struct Payload {
            target: FileTarget,
            path_base64: FilePath,
            #[serde(default)]
            limit: Option<u16>,
            #[serde(default)]
            after_name_base64: Option<FileName>,
        }
        let value = Payload::deserialize(deserializer)?;
        Self::try_new(
            value.target,
            value.path_base64,
            value.limit,
            value.after_name_base64,
        )
        .map_err(de::Error::custom)
    }
}

/// Request to read a bounded existing text file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileReadRequest {
    /// Registered target whose authoritative root the daemon resolves.
    pub target: FileTarget,
    /// Nonempty relative file identifier.
    pub path_base64: FilePath,
}

impl FileReadRequest {
    /// Builds a read request for a non-root identifier.
    ///
    /// # Errors
    ///
    /// Rejects the empty root identifier.
    pub fn try_new(target: FileTarget, path_base64: FilePath) -> Result<Self, WireValidationError> {
        validate_leaf_path(&path_base64)?;
        Ok(Self {
            target,
            path_base64,
        })
    }
}

impl<'de> Deserialize<'de> for FileReadRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        struct Payload {
            target: FileTarget,
            path_base64: FilePath,
        }
        let value = Payload::deserialize(deserializer)?;
        Self::try_new(value.target, value.path_base64).map_err(de::Error::custom)
    }
}

/// Request to replace an existing text file after an optimistic revision check.
#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileWriteRequest {
    /// Registered target whose authoritative root the daemon resolves.
    pub target: FileTarget,
    /// Nonempty relative file identifier; this operation never creates a file.
    pub path_base64: FilePath,
    /// Exact replacement UTF-8 text, without implicit newline or BOM changes.
    pub text: String,
    /// Revision that the caller read before editing.
    pub expected_revision: FileRevision,
}

impl FileWriteRequest {
    /// Builds a bounded request without accessing the file or interpreting text.
    ///
    /// # Errors
    ///
    /// Rejects the root, NUL-containing text or more than 128 KiB of UTF-8 bytes.
    pub fn try_new(
        target: FileTarget,
        path_base64: FilePath,
        text: impl Into<String>,
        expected_revision: FileRevision,
    ) -> Result<Self, WireValidationError> {
        validate_leaf_path(&path_base64)?;
        let text = text.into();
        validate_text(&text)?;
        Ok(Self {
            target,
            path_base64,
            text,
            expected_revision,
        })
    }
}

impl fmt::Debug for FileWriteRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FileWriteRequest")
            .field("target", &self.target)
            .field("path_base64", &self.path_base64)
            .field("text_bytes", &self.text.len())
            .field("expected_revision", &self.expected_revision)
            .finish()
    }
}

impl<'de> Deserialize<'de> for FileWriteRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        struct Payload {
            target: FileTarget,
            path_base64: FilePath,
            text: String,
            expected_revision: FileRevision,
        }
        let value = Payload::deserialize(deserializer)?;
        Self::try_new(
            value.target,
            value.path_base64,
            value.text,
            value.expected_revision,
        )
        .map_err(de::Error::custom)
    }
}

/// Observed entry type; visibility does not grant permission to open an entry.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileEntryKind {
    /// A regular file, which can still be oversized, binary or inaccessible.
    File,
    /// A directory that may be listed through descriptor-relative traversal.
    Directory,
    /// A visible symlink that the file service does not follow.
    Symlink,
    /// A device, socket, FIFO or future unsupported entry kind.
    #[serde(other)]
    Other,
}

/// One observed directory entry with separate identity and display text.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FileEntry {
    /// Exact relative identifier, never reconstructed from the display name.
    pub path_base64: FilePath,
    /// Escaped display text suitable for rendering as text, not HTML.
    pub display_name: String,
    /// Observed filesystem kind.
    pub kind: FileEntryKind,
    /// Observed file length when meaningful for the entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    /// Modification time in Unix epoch milliseconds when representable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_at_ms: Option<i64>,
}

/// A bounded observed directory page, not a filesystem transaction snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FileListResponse {
    /// Directory entries in exact filename-byte order.
    pub entries: Vec<FileEntry>,
    /// Cursor for the next page; absence means no more entries were observed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_after_name_base64: Option<FileName>,
    /// Observation time in Unix epoch milliseconds.
    pub observed_at_ms: i64,
}

/// Exact text and revision observed through a descriptor-safe bounded read.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FileReadResponse {
    /// Exact relative file identifier.
    pub path_base64: FilePath,
    /// File contents, preserving line endings and any BOM.
    pub text: String,
    /// Opaque revision derived from content and filesystem identity.
    pub revision: FileRevision,
    /// Observed UTF-8 byte length.
    pub size_bytes: u64,
    /// Modification time in Unix epoch milliseconds when representable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_at_ms: Option<i64>,
    /// Observation time in Unix epoch milliseconds.
    pub observed_at_ms: i64,
}

impl fmt::Debug for FileReadResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FileReadResponse")
            .field("path_base64", &self.path_base64)
            .field("text_bytes", &self.text.len())
            .field("revision", &self.revision)
            .field("size_bytes", &self.size_bytes)
            .field("modified_at_ms", &self.modified_at_ms)
            .field("observed_at_ms", &self.observed_at_ms)
            .finish()
    }
}

/// Identity and revision of an atomically published text save.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FileWriteResponse {
    /// Exact relative file identifier.
    pub path_base64: FilePath,
    /// Revision of the published file.
    pub revision: FileRevision,
    /// Published UTF-8 byte length.
    pub size_bytes: u64,
    /// Modification time in Unix epoch milliseconds when representable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_at_ms: Option<i64>,
    /// Publication time in Unix epoch milliseconds.
    pub written_at_ms: i64,
}

fn path_error(message: &'static str) -> WireValidationError {
    WireValidationError::new("pathBase64", message)
}

fn validate_path_bytes(bytes: &[u8]) -> Result<(), WireValidationError> {
    if bytes.len() > MAX_FILE_PATH_BYTES {
        return Err(path_error("must decode to at most 4096 bytes"));
    }
    if bytes.contains(&0) {
        return Err(path_error("must not contain a NUL byte"));
    }
    if !bytes.is_empty()
        && bytes
            .split(|byte| *byte == b'/')
            .any(|component| component.is_empty() || component == b"." || component == b"..")
    {
        return Err(path_error(
            "must use nonempty relative child components without traversal",
        ));
    }
    Ok(())
}

fn validate_leaf_path(path: &FilePath) -> Result<(), WireValidationError> {
    if path.is_root() {
        return Err(path_error(
            "must identify a file rather than the target root",
        ));
    }
    Ok(())
}

fn validate_text(text: &str) -> Result<(), WireValidationError> {
    if text.len() > MAX_FILE_TEXT_BYTES {
        return Err(WireValidationError::new(
            "text",
            "must be at most 131072 UTF-8 bytes",
        ));
    }
    if text.contains('\0') {
        return Err(WireValidationError::new(
            "text",
            "must not contain a NUL byte",
        ));
    }
    Ok(())
}
