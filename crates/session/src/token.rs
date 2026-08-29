use std::collections::HashMap;
use std::fmt::{self, Write};
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
#[derive(Default)]
pub(crate) struct TokenStore {
    inner: Mutex<HashMap<String, TokenRecord>>,
}

impl fmt::Debug for TokenStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self
            .inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len();
        formatter
            .debug_struct("TokenStore")
            .field("token_count", &count)
            .finish()
    }
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
        let value = mint_token_value().map_err(|()| {
            SagaError::new(
                SagaErrorKind::InvalidToken,
                "Could not generate a secure worktree removal token",
                "Retry worktree.prepare_remove",
            )
            .with_worktree_id(worktree_id)
        })?;
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
        if record.expires_at_ms <= now_ms {
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

fn mint_token_value() -> Result<String, ()> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| ())?;
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut value, "{byte:02x}").expect("writing to a String cannot fail");
    }
    Ok(value)
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use cli_master_git::{RepositoryStatus, StatusCounts, WorktreeInfo};

    use super::*;

    fn preparation() -> RemovalPreparation {
        RemovalPreparation {
            repository_root: PathBuf::from("/tmp/repository"),
            managed_root: PathBuf::from("/tmp/managed"),
            worktree: WorktreeInfo {
                path: PathBuf::from("/tmp/managed/worktree"),
                head: Some("0123456789abcdef".to_owned()),
                branch: Some("feature".to_owned()),
                detached: false,
                locked: false,
                prunable: false,
            },
            status: RepositoryStatus {
                branch: Some("feature".to_owned()),
                files: Vec::new(),
                counts: StatusCounts::default(),
                has_staged: false,
                has_tracked_changes: false,
                has_untracked: false,
            },
            ignored_paths: Vec::new(),
            assume_unchanged_paths: Vec::new(),
            skip_worktree_paths: Vec::new(),
            running: false,
            in_use: false,
            blockers: Vec::new(),
            can_remove: true,
        }
    }

    #[test]
    fn minted_tokens_are_random_fixed_length_hex() {
        let first = mint_token_value().expect("secure randomness should be available");
        let second = mint_token_value().expect("secure randomness should be available");

        assert_ne!(first, second);
        for token in [first, second] {
            assert_eq!(token.len(), 64);
            assert!(token.bytes().all(|byte| byte.is_ascii_hexdigit()));
        }
    }

    #[test]
    fn token_store_debug_never_exposes_token_values() {
        let store = TokenStore::default();
        let (token, _) = store
            .issue(WorktreeId::new(), None, preparation(), 1_000)
            .expect("token should be issued");

        let debug = format!("{store:?}");

        assert!(debug.contains("token_count: 1"));
        assert!(!debug.contains(token.as_str()));
    }

    #[test]
    fn token_expires_at_its_exact_deadline() {
        let store = TokenStore::default();
        let worktree_id = WorktreeId::new();
        let (token, expires_at_ms) = store
            .issue(worktree_id, None, preparation(), 1_000)
            .expect("token should be issued");

        let error = store
            .take(&token, worktree_id, expires_at_ms)
            .expect_err("token must not remain valid at its expiry instant");

        assert_eq!(error.kind(), SagaErrorKind::InvalidToken);
    }
}
