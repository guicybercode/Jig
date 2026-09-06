//! Pure organization metadata, deliberately separate from process lifecycle.

use std::{collections::BTreeSet, error::Error, fmt};

use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{ProjectId, SessionId};

/// Maximum number of distinct entities in one organization read.
pub const MAX_ORGANIZATION_TARGETS: usize = 100;
/// Highest organization revision representable exactly by JavaScript.
pub const MAX_ORGANIZATION_REVISION: u64 = 9_007_199_254_740_991;

/// An existing project or session selected through the established catalog.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum OrganizationTarget {
    /// Organization metadata for a registered project.
    Project {
        /// Existing project identifier.
        id: ProjectId,
    },
    /// Organization metadata for a daemon-owned session.
    Session {
        /// Existing session identifier.
        id: SessionId,
    },
}

/// User-managed work progress; never a signal about process liveness.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationWorkflow {
    /// Work has not begun.
    Backlog,
    /// Work is underway.
    InProgress,
    /// Work awaits review.
    InReview,
    /// Work is blocked on a dependency.
    Blocked,
    /// Work is considered complete by the user.
    Done,
}

/// Safe validation failure containing a static explanation only.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationValidationError(&'static str);

impl fmt::Display for OrganizationValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Error for OrganizationValidationError {}

/// Validated, ordered batch containing between one and 100 distinct targets.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct OrganizationTargets(Vec<OrganizationTarget>);

impl OrganizationTargets {
    /// Validates batch size and uniqueness while retaining caller order.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, or duplicate-target batches.
    pub fn try_new(targets: Vec<OrganizationTarget>) -> Result<Self, OrganizationValidationError> {
        if targets.is_empty() || targets.len() > MAX_ORGANIZATION_TARGETS {
            return Err(OrganizationValidationError(
                "targets must contain between 1 and 100 entries",
            ));
        }
        if targets.iter().copied().collect::<BTreeSet<_>>().len() != targets.len() {
            return Err(OrganizationValidationError(
                "targets must not contain duplicates",
            ));
        }
        Ok(Self(targets))
    }

    /// Returns the validated targets in request order.
    #[must_use]
    pub fn as_slice(&self) -> &[OrganizationTarget] {
        &self.0
    }

    /// Consumes the batch and returns its ordered targets.
    #[must_use]
    pub fn into_inner(self) -> Vec<OrganizationTarget> {
        self.0
    }
}

impl<'de> Deserialize<'de> for OrganizationTargets {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let targets = Vec::deserialize(deserializer)
            .map_err(|_| de::Error::custom("invalid organization targets"))?;
        Self::try_new(targets).map_err(de::Error::custom)
    }
}

/// Reads organization metadata without creating default rows.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct OrganizationGetRequest {
    /// Existing entities in the desired response order.
    pub targets: OrganizationTargets,
}

/// Organization metadata in exactly the requested target order.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct OrganizationGetResponse {
    /// One entry per requested existing entity.
    pub entries: Vec<OrganizationEntry>,
}

/// Visibility and workflow metadata independent of session process state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationEntry {
    /// Existing entity that owns the organization metadata.
    pub target: OrganizationTarget,
    /// User selected the entity for pinned display.
    pub pinned: bool,
    /// User archived the entity; a session may continue executing.
    pub archived: bool,
    /// Null for projects; user-managed work progress for sessions.
    pub workflow: Option<OrganizationWorkflow>,
    /// Zero for unwritten defaults; positive after an explicit save.
    pub revision: u64,
    /// Null for unwritten defaults; otherwise Unix epoch milliseconds.
    pub updated_at_ms: Option<i64>,
}

impl OrganizationEntry {
    /// Returns unwritten defaults for an existing entity.
    #[must_use]
    pub const fn defaults(target: OrganizationTarget) -> Self {
        Self {
            target,
            pinned: false,
            archived: false,
            workflow: match target {
                OrganizationTarget::Project { .. } => None,
                OrganizationTarget::Session { .. } => Some(OrganizationWorkflow::Backlog),
            },
            revision: 0,
            updated_at_ms: None,
        }
    }

    /// Checks workflow, revision, timestamp, and unwritten-default consistency.
    ///
    /// # Errors
    ///
    /// Rejects mismatched target/workflow, unsafe numbers, or non-default revision-zero values.
    pub fn validate(&self) -> Result<(), OrganizationValidationError> {
        validate_workflow(self.target, self.workflow)?;
        validate_revision(self.revision)?;
        if self.revision == 0 {
            if self != &Self::defaults(self.target) {
                return Err(OrganizationValidationError(
                    "revision zero requires unwritten default values",
                ));
            }
        } else if !self
            .updated_at_ms
            .is_some_and(|value| (0..=9_007_199_254_740_991).contains(&value))
        {
            return Err(OrganizationValidationError(
                "saved entries require a non-negative JavaScript-safe updatedAtMs",
            ));
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for OrganizationEntry {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        struct Fields {
            target: OrganizationTarget,
            pinned: bool,
            archived: bool,
            #[serde(deserialize_with = "required_nullable")]
            workflow: Option<OrganizationWorkflow>,
            revision: u64,
            #[serde(deserialize_with = "required_nullable")]
            updated_at_ms: Option<i64>,
        }
        let fields = Fields::deserialize(deserializer)
            .map_err(|_| de::Error::custom("invalid organization entry"))?;
        let entry = Self {
            target: fields.target,
            pinned: fields.pinned,
            archived: fields.archived,
            workflow: fields.workflow,
            revision: fields.revision,
            updated_at_ms: fields.updated_at_ms,
        };
        entry.validate().map_err(de::Error::custom)?;
        Ok(entry)
    }
}

/// Replaces all organization flags with an optimistic revision check.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationSaveRequest {
    /// Existing project or session whose organization metadata changes.
    pub target: OrganizationTarget,
    /// Zero for the first explicit save; otherwise the observed revision.
    pub expected_revision: u64,
    /// Complete desired pinned flag.
    pub pinned: bool,
    /// Complete desired archived flag.
    pub archived: bool,
    /// Explicit null for projects; required workflow for sessions.
    pub workflow: Option<OrganizationWorkflow>,
}

impl OrganizationSaveRequest {
    /// Validates target/workflow consistency and the expected revision range.
    ///
    /// # Errors
    ///
    /// Rejects project workflow, missing session workflow, or an unsafe revision.
    pub fn validate(&self) -> Result<(), OrganizationValidationError> {
        validate_workflow(self.target, self.workflow)?;
        validate_revision(self.expected_revision)
    }
}

impl<'de> Deserialize<'de> for OrganizationSaveRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        struct Fields {
            target: OrganizationTarget,
            expected_revision: u64,
            pinned: bool,
            archived: bool,
            #[serde(deserialize_with = "required_nullable")]
            workflow: Option<OrganizationWorkflow>,
        }
        let fields = Fields::deserialize(deserializer)
            .map_err(|_| de::Error::custom("invalid organization save request"))?;
        let request = Self {
            target: fields.target,
            expected_revision: fields.expected_revision,
            pinned: fields.pinned,
            archived: fields.archived,
            workflow: fields.workflow,
        };
        request.validate().map_err(de::Error::custom)?;
        Ok(request)
    }
}

fn required_nullable<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(deserializer)
}

fn validate_workflow(
    target: OrganizationTarget,
    workflow: Option<OrganizationWorkflow>,
) -> Result<(), OrganizationValidationError> {
    match (target, workflow) {
        (OrganizationTarget::Project { .. }, None)
        | (OrganizationTarget::Session { .. }, Some(_)) => Ok(()),
        _ => Err(OrganizationValidationError(
            "projects require null workflow and sessions require a workflow value",
        )),
    }
}

const fn validate_revision(revision: u64) -> Result<(), OrganizationValidationError> {
    if revision > MAX_ORGANIZATION_REVISION {
        Err(OrganizationValidationError(
            "revision must be a JavaScript-safe non-negative integer",
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn batches_are_bounded_distinct_typed_and_ordered() {
        let target = OrganizationTarget::Project {
            id: ProjectId::new(),
        };
        assert!(OrganizationTargets::try_new(vec![]).is_err());
        assert!(OrganizationTargets::try_new(vec![target, target]).is_err());
        let targets = (0..100)
            .map(|_| OrganizationTarget::Session {
                id: SessionId::new(),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            OrganizationTargets::try_new(targets.clone())
                .unwrap()
                .as_slice(),
            targets
        );
        assert!(OrganizationTargets::try_new([targets, vec![target]].concat()).is_err());
        assert!(
            serde_json::from_value::<OrganizationGetRequest>(json!({"targets":[target,target]}))
                .is_err()
        );
        assert!(
            serde_json::from_value::<OrganizationTarget>(
                json!({"kind":"project","id":ProjectId::new(),"pid":1})
            )
            .is_err()
        );
    }

    #[test]
    fn defaults_require_explicit_null_timestamp_and_matching_workflow() {
        for target in [
            OrganizationTarget::Project {
                id: ProjectId::new(),
            },
            OrganizationTarget::Session {
                id: SessionId::new(),
            },
        ] {
            let defaults = OrganizationEntry::defaults(target);
            let value = serde_json::to_value(&defaults).unwrap();
            assert_eq!(value["revision"], 0);
            assert!(
                value
                    .get("updatedAtMs")
                    .is_some_and(serde_json::Value::is_null)
            );
            assert_eq!(
                serde_json::from_value::<OrganizationEntry>(value.clone()).unwrap(),
                defaults
            );
            let mut invalid = value;
            invalid["pinned"] = json!(true);
            assert!(serde_json::from_value::<OrganizationEntry>(invalid).is_err());
        }
    }

    #[test]
    fn saves_require_whole_record_and_correct_target_workflow() {
        let mut project = json!({"target":{"kind":"project","id":ProjectId::new()},"expectedRevision":0,"pinned":true,"archived":false,"workflow":null});
        assert!(serde_json::from_value::<OrganizationSaveRequest>(project.clone()).is_ok());
        project.as_object_mut().unwrap().remove("workflow");
        assert!(serde_json::from_value::<OrganizationSaveRequest>(project.clone()).is_err());
        project["workflow"] = json!("done");
        assert!(serde_json::from_value::<OrganizationSaveRequest>(project.clone()).is_err());
        project["target"] = json!({"kind":"session","id":SessionId::new()});
        for workflow in ["backlog", "in_progress", "in_review", "blocked", "done"] {
            project["workflow"] = json!(workflow);
            assert!(serde_json::from_value::<OrganizationSaveRequest>(project.clone()).is_ok());
        }
        project["workflow"] = serde_json::Value::Null;
        assert!(serde_json::from_value::<OrganizationSaveRequest>(project).is_err());
    }

    #[test]
    fn revisions_and_timestamps_reject_unsafe_or_inconsistent_values() {
        let target = OrganizationTarget::Project {
            id: ProjectId::new(),
        };
        let mut entry = OrganizationEntry {
            revision: 1,
            updated_at_ms: Some(0),
            ..OrganizationEntry::defaults(target)
        };
        assert!(entry.validate().is_ok());
        for timestamp in [None, Some(-1), Some(9_007_199_254_740_992)] {
            entry.updated_at_ms = timestamp;
            assert!(serde_json::from_value::<OrganizationEntry>(json!(entry)).is_err());
        }
        for revision in [
            json!(-1),
            json!(1.5),
            json!(MAX_ORGANIZATION_REVISION + 1),
            json!("1"),
        ] {
            assert!(serde_json::from_value::<OrganizationSaveRequest>(json!({"target":target,"expectedRevision":revision,"pinned":false,"archived":false,"workflow":null})).is_err());
        }
    }
}
