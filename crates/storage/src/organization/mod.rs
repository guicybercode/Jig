//! Revisioned organization metadata with snapshot reads and atomic writes.

use std::{collections::BTreeMap, error::Error, fmt};

use cli_master_core::{
    ApiError,
    organization::{
        MAX_ORGANIZATION_REVISION, OrganizationEntry, OrganizationGetRequest,
        OrganizationGetResponse, OrganizationSaveRequest, OrganizationTarget,
        OrganizationValidationError, OrganizationWorkflow,
    },
};
use rusqlite::{Connection, Row, TransactionBehavior, params, params_from_iter};

use crate::{Storage, StorageError};

/// Organization repository failure containing no process or user-text details.
pub enum OrganizationStorageError {
    /// Caller supplied a target/workflow or revision mismatch.
    InvalidInput(OrganizationValidationError),
    /// Clock value cannot be represented safely on the wire.
    InvalidTimestamp,
    /// A selected project is no longer registered.
    ProjectNotFound,
    /// A selected session no longer exists.
    SessionNotFound,
    /// The observed revision is stale.
    Conflict,
    /// An existing row cannot advance beyond the JavaScript-safe range.
    RevisionExhausted,
    /// Persisted organization metadata violates the typed contract.
    CorruptData,
    /// The metadata database could not complete the operation.
    Storage(StorageError),
}

impl OrganizationStorageError {
    /// Returns a stable IPC error without underlying SQL or parameters.
    #[must_use]
    pub fn to_api_error(&self) -> ApiError {
        let (code, action) = match self {
            Self::InvalidInput(_) | Self::InvalidTimestamp => (
                "invalid_payload",
                "Correct the organization values and retry",
            ),
            Self::ProjectNotFound => ("project_not_found", "Select a registered project"),
            Self::SessionNotFound => ("session_not_found", "Select an existing session"),
            Self::Conflict => (
                "organization_conflict",
                "Reload current organization values before saving again",
            ),
            Self::RevisionExhausted => (
                "organization_revision_exhausted",
                "The organization revision cannot be advanced",
            ),
            Self::CorruptData => (
                "organization_corrupt_data",
                "Restore a known-good database backup",
            ),
            Self::Storage(error) => return error.to_api_error(),
        };
        ApiError::new(code, self.to_string()).with_action(action)
    }
}

impl fmt::Display for OrganizationStorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(error) => error.fmt(formatter),
            Self::InvalidTimestamp => {
                formatter.write_str("Organization timestamp is outside the supported range")
            }
            Self::ProjectNotFound => formatter.write_str("The selected project no longer exists"),
            Self::SessionNotFound => formatter.write_str("The selected session no longer exists"),
            Self::Conflict => {
                formatter.write_str("Organization values changed since they were loaded")
            }
            Self::RevisionExhausted => {
                formatter.write_str("The organization revision cannot be advanced")
            }
            Self::CorruptData => {
                formatter.write_str("Stored organization values violate the supported contract")
            }
            Self::Storage(error) => fmt::Display::fmt(error, formatter),
        }
    }
}

impl fmt::Debug for OrganizationStorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

impl Error for OrganizationStorageError {}

impl From<StorageError> for OrganizationStorageError {
    fn from(value: StorageError) -> Self {
        Self::Storage(value)
    }
}

impl From<rusqlite::Error> for OrganizationStorageError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Storage(StorageError::Database(value))
    }
}

impl From<OrganizationValidationError> for OrganizationStorageError {
    fn from(value: OrganizationValidationError) -> Self {
        Self::InvalidInput(value)
    }
}

impl Storage {
    /// Reads one consistent batch in request order without persisting defaults.
    ///
    /// At most two set-based queries read the requested project/session groups;
    /// missing entities fail the complete batch. No process fields are modified.
    ///
    /// # Errors
    ///
    /// Returns an error for missing entities, corrupt rows, or database failure.
    pub fn get_organization(
        &self,
        request: &OrganizationGetRequest,
    ) -> Result<OrganizationGetResponse, OrganizationStorageError> {
        self.with_connection_mut("get organization", |connection| {
            Ok(get_organization(connection, request))
        })?
    }

    /// Replaces complete organization values with an atomic revision comparison.
    ///
    /// The first explicit save creates revision one. Clock rollback never
    /// decreases an existing timestamp. Pin/archive/workflow changes have no
    /// effect on session status, processes, worktrees, or saved knowledge.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid input, missing entities, stale/exhausted
    /// revisions, corrupt rows, or database failure. Failures roll back fully.
    pub fn save_organization(
        &self,
        request: &OrganizationSaveRequest,
        now: i64,
    ) -> Result<OrganizationEntry, OrganizationStorageError> {
        request.validate()?;
        if !(0..=9_007_199_254_740_991).contains(&now) {
            return Err(OrganizationStorageError::InvalidTimestamp);
        }
        self.with_connection_mut("save organization", |connection| {
            Ok(save_organization(connection, request, now))
        })?
    }
}

fn get_organization(
    connection: &mut Connection,
    request: &OrganizationGetRequest,
) -> Result<OrganizationGetResponse, OrganizationStorageError> {
    let transaction = connection.transaction()?;
    let mut projects = Vec::new();
    let mut sessions = Vec::new();
    for target in request.targets.as_slice() {
        match target {
            OrganizationTarget::Project { id } => projects.push(id.to_string()),
            OrganizationTarget::Session { id } => sessions.push(id.to_string()),
        }
    }
    let mut found = read_group(&transaction, Group::Project, &projects)?;
    found.extend(read_group(&transaction, Group::Session, &sessions)?);
    let entries = request
        .targets
        .as_slice()
        .iter()
        .map(|target| found.remove(target).ok_or_else(|| missing(*target)))
        .collect::<Result<Vec<_>, _>>()?;
    transaction.commit()?;
    Ok(OrganizationGetResponse { entries })
}

fn save_organization(
    connection: &mut Connection,
    request: &OrganizationSaveRequest,
    now: i64,
) -> Result<OrganizationEntry, OrganizationStorageError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (group, id) = match request.target {
        OrganizationTarget::Project { id } => (Group::Project, id.to_string()),
        OrganizationTarget::Session { id } => (Group::Session, id.to_string()),
    };
    let current = read_group(&transaction, group, std::slice::from_ref(&id))?
        .remove(&request.target)
        .ok_or_else(|| missing(request.target))?;
    if current.revision != request.expected_revision {
        return Err(OrganizationStorageError::Conflict);
    }
    if current.revision == MAX_ORGANIZATION_REVISION {
        return Err(OrganizationStorageError::RevisionExhausted);
    }
    let entry = OrganizationEntry {
        target: request.target,
        pinned: request.pinned,
        archived: request.archived,
        workflow: request.workflow,
        revision: current.revision + 1,
        updated_at_ms: Some(
            current
                .updated_at_ms
                .map_or(now, |updated| now.max(updated)),
        ),
    };
    entry.validate()?;
    let revision =
        i64::try_from(entry.revision).map_err(|_| OrganizationStorageError::RevisionExhausted)?;
    let previous =
        i64::try_from(current.revision).map_err(|_| OrganizationStorageError::RevisionExhausted)?;
    let changed = match (group, current.revision == 0) {
        (Group::Project, true) => transaction.execute(
            "INSERT INTO project_organization (project_id, pinned, archived, revision, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, entry.pinned, entry.archived, revision, entry.updated_at_ms],
        )?,
        (Group::Project, false) => transaction.execute(
            "UPDATE project_organization SET pinned = ?2, archived = ?3, revision = ?4, updated_at_ms = ?5 WHERE project_id = ?1 AND revision = ?6",
            params![id, entry.pinned, entry.archived, revision, entry.updated_at_ms, previous],
        )?,
        (Group::Session, true) => transaction.execute(
            "INSERT INTO session_organization (session_id, pinned, archived, workflow, revision, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, entry.pinned, entry.archived, entry.workflow.map(workflow_name), revision, entry.updated_at_ms],
        )?,
        (Group::Session, false) => transaction.execute(
            "UPDATE session_organization SET pinned = ?2, archived = ?3, workflow = ?4, revision = ?5, updated_at_ms = ?6 WHERE session_id = ?1 AND revision = ?7",
            params![id, entry.pinned, entry.archived, entry.workflow.map(workflow_name), revision, entry.updated_at_ms, previous],
        )?,
    };
    if changed != 1 {
        return Err(OrganizationStorageError::Conflict);
    }
    transaction.commit()?;
    Ok(entry)
}

#[derive(Clone, Copy)]
enum Group {
    Project,
    Session,
}

fn read_group(
    connection: &Connection,
    group: Group,
    ids: &[String],
) -> Result<BTreeMap<OrganizationTarget, OrganizationEntry>, OrganizationStorageError> {
    let mut found = BTreeMap::new();
    if ids.is_empty() {
        return Ok(found);
    }
    // Only generated placeholder punctuation enters SQL; every ID is bound.
    let placeholders = std::iter::repeat_n("?", ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = match group {
        Group::Project => format!(
            "SELECT p.id, COALESCE(o.pinned, 0), COALESCE(o.archived, 0), NULL, COALESCE(o.revision, 0), o.updated_at_ms
             FROM projects p LEFT JOIN project_organization o ON o.project_id = p.id WHERE p.id IN ({placeholders})"
        ),
        Group::Session => format!(
            "SELECT s.id, COALESCE(o.pinned, 0), COALESCE(o.archived, 0), COALESCE(o.workflow, 'backlog'), COALESCE(o.revision, 0), o.updated_at_ms
             FROM sessions s LEFT JOIN session_organization o ON o.session_id = s.id WHERE s.id IN ({placeholders})"
        ),
    };
    let mut statement = connection.prepare(&sql)?;
    let mut rows = statement.query(params_from_iter(ids))?;
    while let Some(row) = rows.next()? {
        let entry = decode_entry(row, group)?;
        found.insert(entry.target, entry);
    }
    Ok(found)
}

fn decode_entry(
    row: &Row<'_>,
    group: Group,
) -> Result<OrganizationEntry, OrganizationStorageError> {
    let id: String = row.get(0)?;
    let target = match group {
        Group::Project => OrganizationTarget::Project {
            id: id
                .parse()
                .map_err(|_| OrganizationStorageError::CorruptData)?,
        },
        Group::Session => OrganizationTarget::Session {
            id: id
                .parse()
                .map_err(|_| OrganizationStorageError::CorruptData)?,
        },
    };
    let workflow: Option<String> = row.get(3)?;
    let revision: i64 = row.get(4)?;
    let entry = OrganizationEntry {
        target,
        pinned: decode_bool(row.get(1)?)?,
        archived: decode_bool(row.get(2)?)?,
        workflow: workflow.as_deref().map(decode_workflow).transpose()?,
        revision: u64::try_from(revision).map_err(|_| OrganizationStorageError::CorruptData)?,
        updated_at_ms: row.get(5)?,
    };
    entry
        .validate()
        .map_err(|_| OrganizationStorageError::CorruptData)?;
    Ok(entry)
}

fn decode_bool(value: i64) -> Result<bool, OrganizationStorageError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(OrganizationStorageError::CorruptData),
    }
}

fn missing(target: OrganizationTarget) -> OrganizationStorageError {
    match target {
        OrganizationTarget::Project { .. } => OrganizationStorageError::ProjectNotFound,
        OrganizationTarget::Session { .. } => OrganizationStorageError::SessionNotFound,
    }
}

const fn workflow_name(workflow: OrganizationWorkflow) -> &'static str {
    match workflow {
        OrganizationWorkflow::Backlog => "backlog",
        OrganizationWorkflow::InProgress => "in_progress",
        OrganizationWorkflow::InReview => "in_review",
        OrganizationWorkflow::Blocked => "blocked",
        OrganizationWorkflow::Done => "done",
    }
}

fn decode_workflow(value: &str) -> Result<OrganizationWorkflow, OrganizationStorageError> {
    match value {
        "backlog" => Ok(OrganizationWorkflow::Backlog),
        "in_progress" => Ok(OrganizationWorkflow::InProgress),
        "in_review" => Ok(OrganizationWorkflow::InReview),
        "blocked" => Ok(OrganizationWorkflow::Blocked),
        "done" => Ok(OrganizationWorkflow::Done),
        _ => Err(OrganizationStorageError::CorruptData),
    }
}
