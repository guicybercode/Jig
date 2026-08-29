use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use cli_master_core::wire::ConfirmationToken;
use cli_master_core::{SessionId, WorktreeId};
use cli_master_git::RemovalPreparation;

use crate::error::{SagaError, SagaErrorKind};

/// How long a confirmation token remains valid in this daemon lifetime.
pub const TOKEN_TTL_MS: i64 = 5 * 60 * 1_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TokenRecord {
    pub(crate) worktree_id: WorktreeId,
    pub(crate) session_id: Option<SessionId>,
    pub(crate) preparation: RemovalPreparation,
    pub(crate) expires_at_ms: i64,
}

/// In-memory, process-local confirmation tokens. Restart invalidates every token.
#[derive(Debug, Default)]
pub(crate) struct TokenStore {
    inner: Mutex<HashMap<String, TokenRecord>>,
}

impl TokenStore {
    pub(crate) fn issue(
        &self,
        worktree_id: WorktreeId,
        session_id: Option<SessionId>,
        preparation: RemovalPreparation,
        now_ms: i64,
    ) -> Result<(ConfirmationToken, i64), SagaError> {
        let expires_at_ms = now_ms.saturating_add(TOKEN_TTL_MS);
        let value = mint_token_value(worktree_id, now_ms);
        let token = ConfirmationToken::try_new(value.clone()).map_err(|error| {
            SagaError::new(
                SagaErrorKind::InvalidInput,
                error.to_string(),
                "Retry worktree.prepare_remove",
            )
        })?;
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        inner.retain(|_, record| record.worktree_id != worktree_id);
        inner.insert(
            value,
            TokenRecord {
                worktree_id,
                session_id,
                preparation,
                expires_at_ms,
            },
        );
        Ok((token, expires_at_ms))
    }

    pub(crate) fn take(
        &self,
        token: &ConfirmationToken,
        worktree_id: WorktreeId,
        now_ms: i64,
    ) -> Result<TokenRecord, SagaError> {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(record) = inner.remove(token.as_str()) else {
            return Err(invalid_token(worktree_id, "confirmation token is unknown"));
        };
        if record.worktree_id != worktree_id {
            inner.insert(token.as_str().to_owned(), record);
            return Err(invalid_token(
                worktree_id,
                "confirmation token is bound to a different worktree",
            ));
        }
        if record.expires_at_ms < now_ms {
            return Err(invalid_token(worktree_id, "confirmation token has expired"));
        }
        Ok(record)
    }

    pub(crate) fn discard_for(&self, worktree_id: WorktreeId) {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|_, record| record.worktree_id != worktree_id);
    }
}

fn mint_token_value(worktree_id: WorktreeId, now_ms: i64) -> String {
    let mut value = format!(
        "{}{:x}",
        worktree_id.as_uuid().simple(),
        now_ms.unsigned_abs()
    );
    value.retain(|character| character.is_ascii_alphanumeric());
    if value.len() < 16 {
        value.push_str("confirmation");
    }
    value.truncate(256);
    value
}

fn invalid_token(worktree_id: WorktreeId, detail: &str) -> SagaError {
    SagaError::new(
        SagaErrorKind::InvalidToken,
        format!("Worktree removal token is not valid: {detail}"),
        "Call worktree.prepare_remove again and use the new token without bypassing dirty or in-use blockers",
    )
    .with_worktree_id(worktree_id)
}

pub(crate) fn now_ms() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_millis(),
    )
    .expect("timestamp should fit in i64")
}
