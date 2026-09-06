//! Pure contracts for saved local prompts and reusable context.
//!
//! Text crosses the local IPC boundary intentionally, but is excluded from
//! diagnostic formatting. This module performs no filesystem or process I/O.

pub mod discovery;

use std::{error::Error, fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, de};
use uuid::{Uuid, Variant};

use crate::ProjectId;

/// Maximum saved title length in UTF-8 bytes.
pub const MAX_KNOWLEDGE_TITLE_BYTES: usize = 256;
/// Maximum saved prompt or context length in UTF-8 bytes.
pub const MAX_KNOWLEDGE_BODY_BYTES: usize = 64 * 1_024;
/// Highest revision that JavaScript can represent without rounding.
pub const MAX_KNOWLEDGE_REVISION: u64 = 9_007_199_254_740_991;

/// Safe validation failure containing only static labels and explanations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KnowledgeValidationError {
    field: &'static str,
    message: &'static str,
}

impl KnowledgeValidationError {
    const fn new(field: &'static str, message: &'static str) -> Self {
        Self { field, message }
    }

    /// Returns the invalid field's stable label.
    #[must_use]
    pub const fn field(&self) -> &'static str {
        self.field
    }

    /// Returns an explanation that never includes submitted content.
    #[must_use]
    pub const fn message(&self) -> &'static str {
        self.message
    }
}

impl fmt::Display for KnowledgeValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} {}", self.field, self.message)
    }
}

impl Error for KnowledgeValidationError {}

/// UUID version 7 identifier for one saved prompt or context entry.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct KnowledgeId(Uuid);

impl KnowledgeId {
    /// Generates a time-ordered UUID version 7 identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// Validates an existing UUID without changing it.
    ///
    /// # Errors
    ///
    /// Returns an error unless the UUID uses version 7 and the RFC variant.
    pub fn try_from_uuid(value: Uuid) -> Result<Self, KnowledgeValidationError> {
        if value.get_version_num() != 7 || value.get_variant() != Variant::RFC4122 {
            return Err(KnowledgeValidationError::new(
                "id",
                "must be a UUID version 7",
            ));
        }
        Ok(Self(value))
    }

    /// Returns the underlying UUID.
    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }

    /// Consumes the identifier and returns the underlying UUID.
    #[must_use]
    pub const fn into_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for KnowledgeId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for KnowledgeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for KnowledgeId {
    type Err = KnowledgeValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let uuid = Uuid::parse_str(value)
            .map_err(|_| KnowledgeValidationError::new("id", "must be a UUID version 7"))?;
        Self::try_from_uuid(uuid)
    }
}

impl<'de> Deserialize<'de> for KnowledgeId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)
            .map_err(|_| de::Error::custom("id must be a UUID version 7 string"))?;
        value.parse().map_err(de::Error::custom)
    }
}

/// The local purpose of a saved knowledge entry.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeKind {
    /// Reusable instructions selected explicitly by a user.
    Prompt,
    /// Reusable local background information selected explicitly by a user.
    Context,
}

macro_rules! knowledge_text {
    ($name:ident, $description:literal, $field:literal, $maximum:ident) => {
        #[doc = $description]
        #[derive(Clone, Eq, PartialEq, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Validates text while preserving its exact whitespace and content.
            ///
            /// # Errors
            ///
            /// Returns an error for blank text, a NUL byte, or an exceeded byte limit.
            pub fn try_new(value: impl Into<String>) -> Result<Self, KnowledgeValidationError> {
                let value = value.into();
                validate_text($field, &value, $maximum)?;
                Ok(Self(value))
            }

            /// Returns validated text for explicit use or local persistence.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Consumes the value and returns its text.
            #[must_use]
            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_struct(stringify!($name))
                    .field("bytes", &self.0.len())
                    .finish_non_exhaustive()
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)
                    .map_err(|_| de::Error::custom(concat!($field, " must be text")))?;
                Self::try_new(value).map_err(de::Error::custom)
            }
        }
    };
}

knowledge_text!(
    KnowledgeTitle,
    "Non-blank saved title bounded to 256 UTF-8 bytes; diagnostic output is redacted.",
    "title",
    MAX_KNOWLEDGE_TITLE_BYTES
);
knowledge_text!(
    KnowledgeBody,
    "Non-blank prompt or context bounded to 64 KiB; diagnostic output is redacted.",
    "body",
    MAX_KNOWLEDGE_BODY_BYTES
);

/// One persisted local prompt or context entry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeEntry {
    /// Stable identifier generated by the local owner.
    pub id: KnowledgeId,
    /// Prompt or context classification.
    pub kind: KnowledgeKind,
    /// Owning project, or no project for a globally reusable entry.
    pub project_id: Option<ProjectId>,
    /// User-facing title, excluded from diagnostic formatting.
    pub title: KnowledgeTitle,
    /// Explicitly saved user content, excluded from diagnostic formatting.
    pub body: KnowledgeBody,
    /// Positive revision used for optimistic concurrency.
    pub revision: u64,
    /// Creation time as Unix epoch milliseconds.
    pub created_at_ms: i64,
    /// Most recent edit time as Unix epoch milliseconds.
    pub updated_at_ms: i64,
}

impl KnowledgeEntry {
    /// Checks invariants for a programmatically assembled entry.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid revisions or timestamps outside the ordered,
    /// non-negative JavaScript-safe epoch millisecond range.
    pub const fn validate(&self) -> Result<(), KnowledgeValidationError> {
        match validate_revision("revision", self.revision) {
            Ok(()) => (),
            Err(error) => return Err(error),
        }
        if self.created_at_ms < 0
            || self.updated_at_ms < self.created_at_ms
            || self.updated_at_ms > 9_007_199_254_740_991
        {
            return Err(KnowledgeValidationError::new(
                "timestamps",
                "must satisfy 0 <= createdAtMs <= updatedAtMs <= 9007199254740991",
            ));
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for KnowledgeEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        struct Fields {
            id: KnowledgeId,
            kind: KnowledgeKind,
            project_id: Option<ProjectId>,
            title: KnowledgeTitle,
            body: KnowledgeBody,
            revision: u64,
            created_at_ms: i64,
            updated_at_ms: i64,
        }

        let fields = Fields::deserialize(deserializer)
            .map_err(|_| de::Error::custom("invalid saved knowledge entry"))?;
        let entry = Self {
            id: fields.id,
            kind: fields.kind,
            project_id: fields.project_id,
            title: fields.title,
            body: fields.body,
            revision: fields.revision,
            created_at_ms: fields.created_at_ms,
            updated_at_ms: fields.updated_at_ms,
        };
        entry.validate().map_err(de::Error::custom)?;
        Ok(entry)
    }
}

/// Selects global entries, optionally including entries owned by one project.
#[derive(Clone, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeListRequest {
    /// When absent, list global entries only; when present, include this project.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    /// Optional prompt/context filter applied to both scopes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<KnowledgeKind>,
    /// Exclusive cursor in ascending identifier order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<KnowledgeId>,
    /// Literal case-insensitive title/body search, bounded to 256 UTF-8 bytes.
    ///
    /// Empty text means no search filter. The daemon trims surrounding whitespace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
}

impl KnowledgeListRequest {
    /// Checks search limits for programmatic callers.
    ///
    /// # Errors
    ///
    /// Returns an error for an oversized or NUL-containing search query.
    pub fn validate(&self) -> Result<(), KnowledgeValidationError> {
        if let Some(query) = &self.query {
            if query.len() > MAX_KNOWLEDGE_TITLE_BYTES {
                return Err(KnowledgeValidationError::new(
                    "query",
                    "exceeds its UTF-8 byte limit",
                ));
            }
            if query.contains('\0') {
                return Err(KnowledgeValidationError::new(
                    "query",
                    "must not contain a NUL byte",
                ));
            }
        }
        Ok(())
    }
}

impl fmt::Debug for KnowledgeListRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("KnowledgeListRequest")
            .field("project_id", &self.project_id)
            .field("kind", &self.kind)
            .field("cursor", &self.cursor)
            .field("query_bytes", &self.query.as_ref().map(String::len))
            .finish()
    }
}

impl<'de> Deserialize<'de> for KnowledgeListRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        struct Fields {
            project_id: Option<ProjectId>,
            kind: Option<KnowledgeKind>,
            cursor: Option<KnowledgeId>,
            query: Option<String>,
        }

        let fields = Fields::deserialize(deserializer)
            .map_err(|_| de::Error::custom("invalid knowledge list request"))?;
        let mut request = Self {
            project_id: fields.project_id,
            kind: fields.kind,
            cursor: fields.cursor,
            query: fields.query,
        };
        request.validate().map_err(de::Error::custom)?;
        request.query = request.query.map(|query| query.trim().to_owned());
        Ok(request)
    }
}

/// Bounded page of local knowledge in ascending identifier order.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct KnowledgeListResponse {
    /// Entries selected and packed within the daemon's row and byte limits.
    pub entries: Vec<KnowledgeEntry>,
    /// Exclusive cursor for the next page, or null when the result is exhausted.
    pub next_cursor: Option<KnowledgeId>,
}

/// Creates a local entry or replaces its text with an optimistic revision check.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSaveRequest {
    /// Absent for creation; identifies the existing entry for an update.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<KnowledgeId>,
    /// Absent for creation; must equal the persisted revision for an update.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_revision: Option<u64>,
    /// Owning project, or global scope when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    /// Explicit prompt/context classification.
    pub kind: KnowledgeKind,
    /// Validated saved title.
    pub title: KnowledgeTitle,
    /// Validated saved content.
    pub body: KnowledgeBody,
}

impl KnowledgeSaveRequest {
    /// Checks mutation shape and revision bounds for programmatic callers.
    ///
    /// Persistence must check the expected revision atomically with the write.
    ///
    /// # Errors
    ///
    /// Returns an error for a partial id/revision pair or an invalid revision.
    pub const fn validate(&self) -> Result<(), KnowledgeValidationError> {
        match (self.id, self.expected_revision) {
            (None, None) => Ok(()),
            (Some(_), Some(revision)) => validate_revision("expectedRevision", revision),
            _ => Err(KnowledgeValidationError::new(
                "id and expectedRevision",
                "must either both be absent for creation or both be present for an update",
            )),
        }
    }
}

impl<'de> Deserialize<'de> for KnowledgeSaveRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        struct Fields {
            id: Option<KnowledgeId>,
            expected_revision: Option<u64>,
            project_id: Option<ProjectId>,
            kind: KnowledgeKind,
            title: KnowledgeTitle,
            body: KnowledgeBody,
        }

        let fields = Fields::deserialize(deserializer)
            .map_err(|_| de::Error::custom("invalid knowledge save request"))?;
        let request = Self {
            id: fields.id,
            expected_revision: fields.expected_revision,
            project_id: fields.project_id,
            kind: fields.kind,
            title: fields.title,
            body: fields.body,
        };
        request.validate().map_err(de::Error::custom)?;
        Ok(request)
    }
}

/// Removes one local entry only if its revision still matches the user's copy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDeleteRequest {
    /// Stable identifier of the entry to remove.
    pub id: KnowledgeId,
    /// Revision observed when removal was requested.
    pub expected_revision: u64,
}

impl KnowledgeDeleteRequest {
    /// Checks revision bounds for programmatic callers.
    ///
    /// # Errors
    ///
    /// Returns an error for a zero or JavaScript-unsafe revision.
    pub const fn validate(&self) -> Result<(), KnowledgeValidationError> {
        validate_revision("expectedRevision", self.expected_revision)
    }
}

impl<'de> Deserialize<'de> for KnowledgeDeleteRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        struct Fields {
            id: KnowledgeId,
            expected_revision: u64,
        }

        let fields = Fields::deserialize(deserializer)
            .map_err(|_| de::Error::custom("invalid knowledge delete request"))?;
        let request = Self {
            id: fields.id,
            expected_revision: fields.expected_revision,
        };
        request.validate().map_err(de::Error::custom)?;
        Ok(request)
    }
}

fn validate_text(
    field: &'static str,
    value: &str,
    maximum_bytes: usize,
) -> Result<(), KnowledgeValidationError> {
    if value.len() > maximum_bytes {
        return Err(KnowledgeValidationError::new(
            field,
            "exceeds its UTF-8 byte limit",
        ));
    }
    if value.trim().is_empty() {
        return Err(KnowledgeValidationError::new(field, "must not be blank"));
    }
    if value.contains('\0') {
        return Err(KnowledgeValidationError::new(
            field,
            "must not contain a NUL byte",
        ));
    }
    Ok(())
}

const fn validate_revision(
    field: &'static str,
    revision: u64,
) -> Result<(), KnowledgeValidationError> {
    if revision == 0 || revision > MAX_KNOWLEDGE_REVISION {
        return Err(KnowledgeValidationError::new(
            field,
            "must be an integer between 1 and 9007199254740991",
        ));
    }
    Ok(())
}
