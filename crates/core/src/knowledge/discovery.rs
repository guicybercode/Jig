//! Read-only discovery capabilities for known local rule and skill documents.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, de};

use super::KnowledgeId;
use crate::ProjectId;

macro_rules! capability_id {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(
            Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
        )]
        #[serde(transparent)]
        pub struct $name(KnowledgeId);

        impl $name {
            /// Generates an ephemeral UUID version 7 capability identifier.
            #[must_use]
            pub fn new() -> Self {
                Self(KnowledgeId::new())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

capability_id!(
    KnowledgeScanId,
    "Opaque identifier for one expiring daemon-owned source inventory."
);
capability_id!(
    KnowledgeSourceId,
    "Opaque identifier for a source within exactly one inventory."
);

/// Inventories known global sources and optionally one registered project root.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct KnowledgeDiscoverRequest {
    /// Project whose root sources should accompany global sources.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
}

/// Requests a source by its scan capability; client-supplied paths are forbidden.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct KnowledgeReadRequest {
    /// Unexpired scan that owns the entry capability.
    pub scan_id: KnowledgeScanId,
    /// Source selected from that scan.
    pub entry_id: KnowledgeSourceId,
}

/// Category of a discovered local document.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeSourceKind {
    /// A rule or instruction document.
    Rule,
    /// A skill's SKILL.md document; supporting files are not read.
    Skill,
}

/// Native tool associated with a documented source location.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeProvider {
    /// `OpenAI` Codex.
    Codex,
    /// Anthropic Claude Code.
    Claude,
    /// Cursor.
    Cursor,
}

/// Scope implied by the source location, not proof of native activation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeSourceScope {
    /// User-owned global source.
    Global,
    /// Source in the selected registered project's root configuration.
    Project,
    /// Machine-managed Codex skills directory.
    Admin,
}

/// Metadata availability observed during discovery; reads revalidate it.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeSourceAvailability {
    /// A regular, bounded file was found; UTF-8 is checked only when read.
    Available,
    /// File exceeds the 64 KiB content limit.
    TooLarge,
    /// Leaf symlink is intentionally not followed.
    Symlink,
    /// Candidate is a directory, FIFO, socket, or device rather than a regular file.
    NonRegular,
    /// Candidate could not be opened safely with current permissions.
    Unreadable,
}

/// Provenance and metadata for a discovered rule or skill.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct KnowledgeSourceEntry {
    /// Opaque selector usable only with the scan that returned it.
    pub entry_id: KnowledgeSourceId,
    /// Rule or skill classification.
    pub kind: KnowledgeSourceKind,
    /// Native provider associated with this location.
    pub provider: KnowledgeProvider,
    /// Global, project, or administrator scope.
    pub scope: KnowledgeSourceScope,
    /// Daemon-observed display path; never accepted as a read selector.
    pub source_path: String,
    /// Filesystem name, not parsed or executed frontmatter.
    pub name: String,
    /// Relative project scope ("." or a nested path), or empty for global/admin sources.
    pub scope_directory: String,
    /// Static documented precedence caveat; never an effective/active assertion.
    pub precedence_hint: String,
    /// Whether an intentional skill-directory symlink was traversed.
    pub via_symlink: bool,
    /// Availability observed during the inventory.
    pub availability: KnowledgeSourceAvailability,
}

/// Safe, bounded explanation of an incomplete or unsupported discovery case.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct KnowledgeDiscoveryIssue {
    /// Stable machine-readable issue code.
    pub code: String,
    /// Display-only source location when available.
    pub source_path: Option<String>,
    /// Static explanation without document content or underlying OS errors.
    pub message: String,
}

/// Bounded metadata inventory; document contents require an explicit read.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct KnowledgeDiscoverResponse {
    /// Expiring capability for this inventory.
    pub scan_id: KnowledgeScanId,
    /// Source metadata in stable source-spec/path order.
    pub entries: Vec<KnowledgeSourceEntry>,
    /// True if a scan or response limit prevented a complete inventory.
    pub truncated: bool,
    /// Partial results and unsupported-policy explanations.
    pub issues: Vec<KnowledgeDiscoveryIssue>,
}

/// A source revalidated against its inventory metadata and explicitly read.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct KnowledgeReadResponse {
    /// Original source provenance and capability.
    pub entry: KnowledgeSourceEntry,
    /// Raw bounded UTF-8 document content, excluded from diagnostics.
    #[serde(deserialize_with = "deserialize_content")]
    pub content: String,
}

fn deserialize_content<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let content = String::deserialize(deserializer)
        .map_err(|_| de::Error::custom("source content must be UTF-8 text"))?;
    if content.len() > super::MAX_KNOWLEDGE_BODY_BYTES || content.contains('\0') {
        return Err(de::Error::custom(
            "source content must be NUL-free text of at most 64 KiB",
        ));
    }
    Ok(content)
}

impl fmt::Debug for KnowledgeReadResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("KnowledgeReadResponse")
            .field("entry", &self.entry)
            .field("content_bytes", &self.content.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn read_content_rejects_oversized_or_nul_text_without_echoing_content() {
        for content in ["x".repeat(65_537), "PRIVATE_CONTENT\0".into()] {
            let value = json!({
                "entry": {
                    "entryId": KnowledgeSourceId::new(), "kind": "rule", "provider": "codex",
                    "scope": "project", "sourcePath": "/project/AGENTS.md", "name": "AGENTS.md",
                    "scopeDirectory": ".", "precedenceHint": "Inventory only", "viaSymlink": false,
                    "availability": "available",
                },
                "content": content,
            });
            let error = serde_json::from_value::<KnowledgeReadResponse>(value).unwrap_err();
            assert!(!error.to_string().contains("PRIVATE_CONTENT"));
        }
    }

    #[test]
    fn discovery_requests_accept_only_opaque_capabilities_and_project_selection() {
        assert!(
            serde_json::from_value::<KnowledgeDiscoverRequest>(json!({"path":"/private"})).is_err()
        );
        assert!(serde_json::from_value::<KnowledgeReadRequest>(json!({
            "scanId": KnowledgeScanId::new(), "entryId": KnowledgeSourceId::new(), "path": "/private",
        })).is_err());
        assert!(serde_json::from_value::<KnowledgeReadRequest>(json!({
            "scanId": "00000000-0000-0000-0000-000000000000", "entryId": KnowledgeSourceId::new(),
        })).is_err());
    }
}
