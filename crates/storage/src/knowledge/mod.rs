//! Durable, explicitly saved local prompts and context with optimistic writes.

use std::{error::Error, fmt};

use cli_master_core::{
    ApiError, ProjectId,
    knowledge::{
        KnowledgeBody, KnowledgeDeleteRequest, KnowledgeEntry, KnowledgeId, KnowledgeKind,
        KnowledgeListRequest, KnowledgeListResponse, KnowledgeSaveRequest, KnowledgeTitle,
        KnowledgeValidationError, MAX_KNOWLEDGE_REVISION,
    },
};
use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};

use crate::{Storage, StorageError};

/// Maximum rows returned in one knowledge page.
pub const MAX_KNOWLEDGE_PAGE_ENTRIES: usize = 50;
/// Maximum serialized page size, leaving space for the socket response envelope.
pub const MAX_KNOWLEDGE_PAGE_BYTES: usize = 512 * 1_024;

const ENTRY_COLUMNS: &str =
    "id, kind, project_id, title, body, revision, created_at_ms, updated_at_ms";
// Reserve more than the fixed JSON wrapper plus a UUID next-page cursor.
const PAGE_OVERHEAD_BYTES: usize = 128;

/// Safe knowledge repository failure. Diagnostic formatting never includes text.
pub enum KnowledgeStorageError {
    /// Caller supplied a malformed request.
    InvalidInput(KnowledgeValidationError),
    /// The supplied clock value cannot be represented safely on the wire.
    InvalidTimestamp,
    /// The requested saved entry no longer exists.
    NotFound,
    /// The selected project does not exist.
    ProjectNotFound,
    /// Another writer changed the entry after the caller loaded it.
    Conflict,
    /// The entry exhausted the revision range and cannot be incremented.
    RevisionExhausted,
    /// Persisted content violates the typed contract.
    CorruptData,
    /// The underlying metadata store could not complete the operation.
    Storage(StorageError),
}

impl KnowledgeStorageError {
    /// Projects a failure into a stable error without SQL or saved content.
    #[must_use]
    pub fn to_api_error(&self) -> ApiError {
        let (code, action) = match self {
            Self::InvalidInput(_) | Self::InvalidTimestamp => {
                ("invalid_request", "Correct the request and retry")
            }
            Self::NotFound => ("knowledge_not_found", "Reload the saved library"),
            Self::ProjectNotFound => ("project_not_found", "Select an existing project"),
            Self::Conflict => (
                "knowledge_conflict",
                "Reload the entry before saving or deleting",
            ),
            Self::RevisionExhausted => (
                "knowledge_revision_exhausted",
                "Save the content as a new entry",
            ),
            Self::CorruptData => (
                "knowledge_corrupt_data",
                "Restore a known-good database backup",
            ),
            Self::Storage(error) => return error.to_api_error(),
        };
        ApiError::new(code, self.to_string()).with_action(action)
    }
}

impl fmt::Display for KnowledgeStorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(error) => error.fmt(formatter),
            Self::InvalidTimestamp => {
                formatter.write_str("Knowledge timestamp is outside the supported range")
            }
            Self::NotFound => formatter.write_str("The saved entry no longer exists"),
            Self::ProjectNotFound => formatter.write_str("The selected project no longer exists"),
            Self::Conflict => {
                formatter.write_str("The saved entry has changed since it was loaded")
            }
            Self::RevisionExhausted => {
                formatter.write_str("The saved entry cannot advance its revision")
            }
            Self::CorruptData => {
                formatter.write_str("Stored knowledge does not match the supported contract")
            }
            Self::Storage(error) => fmt::Display::fmt(error, formatter),
        }
    }
}

impl fmt::Debug for KnowledgeStorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

// Deliberately avoid exposing underlying SQLite errors through a source chain.
impl Error for KnowledgeStorageError {}

impl From<StorageError> for KnowledgeStorageError {
    fn from(value: StorageError) -> Self {
        Self::Storage(value)
    }
}

impl From<rusqlite::Error> for KnowledgeStorageError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Storage(StorageError::Database(value))
    }
}

impl From<KnowledgeValidationError> for KnowledgeStorageError {
    fn from(value: KnowledgeValidationError) -> Self {
        Self::InvalidInput(value)
    }
}

impl Storage {
    /// Lists globals or globals plus one existing project's entries in ID order.
    ///
    /// The page is bounded by row count and actual JSON size. Literal search
    /// uses `SQLite`'s built-in case folding, which is case-insensitive for ASCII.
    /// Project validation and the page read share one database snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid input, a missing project, corrupt rows, or database failure.
    pub fn list_knowledge(
        &self,
        request: &KnowledgeListRequest,
    ) -> Result<KnowledgeListResponse, KnowledgeStorageError> {
        request.validate()?;
        self.with_connection_mut("list knowledge", |connection| {
            Ok(list_knowledge(connection, request))
        })?
    }

    /// Creates an entry or atomically replaces the caller's expected revision.
    ///
    /// An update may explicitly change kind or scope. Saved text is never
    /// executed or sent to an agent by this operation. Clock rollback does not
    /// decrease the entry's update timestamp.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid input, a missing row/project, a stale or
    /// exhausted revision, or database failure. Failed writes roll back fully.
    pub fn save_knowledge(
        &self,
        request: &KnowledgeSaveRequest,
        now: i64,
    ) -> Result<KnowledgeEntry, KnowledgeStorageError> {
        request.validate()?;
        if !(0..=9_007_199_254_740_991).contains(&now) {
            return Err(KnowledgeStorageError::InvalidTimestamp);
        }
        self.with_connection_mut("save knowledge", |connection| {
            Ok(save_knowledge(connection, request, now))
        })?
    }

    /// Atomically deletes only the entry revision observed by the caller.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid input, a missing entry, a stale revision, or database failure.
    pub fn delete_knowledge(
        &self,
        request: &KnowledgeDeleteRequest,
    ) -> Result<(), KnowledgeStorageError> {
        request.validate()?;
        self.with_connection_mut("delete knowledge", |connection| {
            Ok(delete_knowledge(connection, request))
        })?
    }
}

fn list_knowledge(
    connection: &mut Connection,
    request: &KnowledgeListRequest,
) -> Result<KnowledgeListResponse, KnowledgeStorageError> {
    let transaction = connection.transaction()?;
    require_project(&transaction, request.project_id)?;
    let sql = format!(
        "SELECT {ENTRY_COLUMNS} FROM knowledge_documents
         WHERE (project_id IS NULL OR project_id = ?1)
           AND (?2 IS NULL OR kind = ?2)
           AND (?3 IS NULL OR id > ?3)
           AND (?4 = '' OR instr(lower(title), lower(?4)) > 0
                        OR instr(lower(body), lower(?4)) > 0)
         ORDER BY id ASC LIMIT 51"
    );
    let mut statement = transaction.prepare(&sql)?;
    let mut rows = statement.query(params![
        request.project_id.map(|id| id.to_string()),
        request.kind.map(kind_name),
        request.cursor.map(|id| id.to_string()),
        request.query.as_deref().unwrap_or("").trim(),
    ])?;
    let mut page = KnowledgeListResponse {
        entries: Vec::new(),
        next_cursor: None,
    };
    let mut page_bytes = PAGE_OVERHEAD_BYTES;
    while let Some(row) = rows.next()? {
        if page.entries.len() == MAX_KNOWLEDGE_PAGE_ENTRIES {
            page.next_cursor = page.entries.last().map(|entry| entry.id);
            break;
        }
        let entry = decode_entry(row)?;
        let serialized =
            serde_json::to_vec(&entry).map_err(|_| KnowledgeStorageError::CorruptData)?;
        if page_bytes + serialized.len() + 1 > MAX_KNOWLEDGE_PAGE_BYTES {
            let Some(last) = page.entries.last() else {
                return Err(KnowledgeStorageError::CorruptData);
            };
            page.next_cursor = Some(last.id);
            break;
        }
        page_bytes += serialized.len() + 1;
        page.entries.push(entry);
    }
    drop(rows);
    drop(statement);
    transaction.commit()?;
    Ok(page)
}

fn save_knowledge(
    connection: &mut Connection,
    request: &KnowledgeSaveRequest,
    now: i64,
) -> Result<KnowledgeEntry, KnowledgeStorageError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    require_project(&transaction, request.project_id)?;
    let (id, revision, created_at_ms, updated_at_ms) = match (request.id, request.expected_revision)
    {
        (Some(id), Some(expected_revision)) => {
            let current = load_entry(&transaction, id)?.ok_or(KnowledgeStorageError::NotFound)?;
            if current.revision != expected_revision {
                return Err(KnowledgeStorageError::Conflict);
            }
            if current.revision == MAX_KNOWLEDGE_REVISION {
                return Err(KnowledgeStorageError::RevisionExhausted);
            }
            (
                id,
                current.revision + 1,
                current.created_at_ms,
                now.max(current.updated_at_ms),
            )
        }
        (None, None) => (KnowledgeId::new(), 1, now, now),
        _ => return Err(KnowledgeStorageError::CorruptData),
    };
    let entry = KnowledgeEntry {
        id,
        kind: request.kind,
        project_id: request.project_id,
        title: request.title.clone(),
        body: request.body.clone(),
        revision,
        created_at_ms,
        updated_at_ms,
    };
    entry.validate()?;
    let revision_sql =
        i64::try_from(entry.revision).map_err(|_| KnowledgeStorageError::RevisionExhausted)?;
    let changed = if let Some(expected_revision) = request.expected_revision {
        let expected_sql = i64::try_from(expected_revision)
            .map_err(|_| KnowledgeStorageError::RevisionExhausted)?;
        transaction.execute(
            "UPDATE knowledge_documents SET kind = ?2, project_id = ?3, title = ?4,
             body = ?5, revision = ?6, updated_at_ms = ?7 WHERE id = ?1 AND revision = ?8",
            params![
                id.to_string(),
                kind_name(entry.kind),
                entry.project_id.map(|id| id.to_string()),
                entry.title.as_str(),
                entry.body.as_str(),
                revision_sql,
                updated_at_ms,
                expected_sql
            ],
        )?
    } else {
        transaction.execute(
            "INSERT INTO knowledge_documents
             (id, kind, project_id, title, body, revision, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id.to_string(),
                kind_name(entry.kind),
                entry.project_id.map(|id| id.to_string()),
                entry.title.as_str(),
                entry.body.as_str(),
                revision_sql,
                created_at_ms,
                updated_at_ms
            ],
        )?
    };
    if changed != 1 {
        return Err(KnowledgeStorageError::Conflict);
    }
    transaction.commit()?;
    Ok(entry)
}

fn delete_knowledge(
    connection: &mut Connection,
    request: &KnowledgeDeleteRequest,
) -> Result<(), KnowledgeStorageError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = load_entry(&transaction, request.id)?.ok_or(KnowledgeStorageError::NotFound)?;
    if current.revision != request.expected_revision {
        return Err(KnowledgeStorageError::Conflict);
    }
    let revision = i64::try_from(request.expected_revision)
        .map_err(|_| KnowledgeStorageError::RevisionExhausted)?;
    let changed = transaction.execute(
        "DELETE FROM knowledge_documents WHERE id = ?1 AND revision = ?2",
        params![request.id.to_string(), revision],
    )?;
    if changed != 1 {
        return Err(KnowledgeStorageError::Conflict);
    }
    transaction.commit()?;
    Ok(())
}

fn require_project(
    connection: &Connection,
    project: Option<ProjectId>,
) -> Result<(), KnowledgeStorageError> {
    if let Some(id) = project {
        let exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE id = ?1)",
            [id.to_string()],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(KnowledgeStorageError::ProjectNotFound);
        }
    }
    Ok(())
}

fn load_entry(
    connection: &Connection,
    id: KnowledgeId,
) -> Result<Option<KnowledgeEntry>, KnowledgeStorageError> {
    let sql = format!("SELECT {ENTRY_COLUMNS} FROM knowledge_documents WHERE id = ?1");
    connection
        .query_row(&sql, [id.to_string()], |row| Ok(decode_entry(row)))
        .optional()?
        .transpose()
}

fn decode_entry(row: &Row<'_>) -> Result<KnowledgeEntry, KnowledgeStorageError> {
    let id: String = row.get(0)?;
    let kind: String = row.get(1)?;
    let project: Option<String> = row.get(2)?;
    let revision: i64 = row.get(5)?;
    let entry = KnowledgeEntry {
        id: id.parse().map_err(|_| KnowledgeStorageError::CorruptData)?,
        kind: match kind.as_str() {
            "prompt" => KnowledgeKind::Prompt,
            "context" => KnowledgeKind::Context,
            _ => return Err(KnowledgeStorageError::CorruptData),
        },
        project_id: project
            .map(|id| id.parse())
            .transpose()
            .map_err(|_| KnowledgeStorageError::CorruptData)?,
        title: KnowledgeTitle::try_new(row.get::<_, String>(3)?)
            .map_err(|_| KnowledgeStorageError::CorruptData)?,
        body: KnowledgeBody::try_new(row.get::<_, String>(4)?)
            .map_err(|_| KnowledgeStorageError::CorruptData)?,
        revision: u64::try_from(revision).map_err(|_| KnowledgeStorageError::CorruptData)?,
        created_at_ms: row.get(6)?,
        updated_at_ms: row.get(7)?,
    };
    entry
        .validate()
        .map_err(|_| KnowledgeStorageError::CorruptData)?;
    Ok(entry)
}

const fn kind_name(kind: KnowledgeKind) -> &'static str {
    match kind {
        KnowledgeKind::Prompt => "prompt",
        KnowledgeKind::Context => "context",
    }
}
