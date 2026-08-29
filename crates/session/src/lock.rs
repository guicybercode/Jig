use std::collections::HashSet;
use std::hash::Hash;
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use cli_master_core::WorktreeId;

use crate::error::{SagaError, SagaErrorKind};

/// In-memory exclusive set used as a destination or mutation guard.
#[derive(Debug, Default)]
pub(crate) struct ExclusiveSet<K> {
    held: Mutex<HashSet<K>>,
}

impl<K> ExclusiveSet<K>
where
    K: Clone + Eq + Hash,
{
    pub(crate) fn try_acquire(&self, key: K) -> Option<ExclusiveGuard<'_, K>> {
        let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        if !held.insert(key.clone()) {
            return None;
        }
        Some(ExclusiveGuard { key, set: self })
    }
}

/// RAII lease that releases a key when dropped.
#[derive(Debug)]
pub(crate) struct ExclusiveGuard<'a, K>
where
    K: Clone + Eq + Hash,
{
    key: K,
    set: &'a ExclusiveSet<K>,
}

impl<K> Drop for ExclusiveGuard<'_, K>
where
    K: Clone + Eq + Hash,
{
    fn drop(&mut self) {
        self.set
            .held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.key);
    }
}

pub(crate) type DestinationLocks = ExclusiveSet<PathBuf>;
pub(crate) type MutationGuards = ExclusiveSet<WorktreeId>;

pub(crate) fn lock_destination(
    locks: &DestinationLocks,
    destination: PathBuf,
) -> Result<ExclusiveGuard<'_, PathBuf>, SagaError> {
    locks.try_acquire(destination.clone()).ok_or_else(|| {
        SagaError::new(
            SagaErrorKind::ConcurrentCreate,
            format!(
                "Another create or removal already holds destination {}",
                destination.display()
            ),
            "Wait for the in-flight operation to finish, then retry with a freshly generated plan",
        )
        .with_path(destination)
    })
}

pub(crate) fn lock_mutation(
    guards: &MutationGuards,
    worktree_id: WorktreeId,
) -> Result<ExclusiveGuard<'_, WorktreeId>, SagaError> {
    guards.try_acquire(worktree_id).ok_or_else(|| {
        SagaError::new(
            SagaErrorKind::MutationInProgress,
            format!("A mutation guard is already held for worktree {worktree_id}"),
            "Wait for the in-flight removal to finish before mutating this worktree",
        )
        .with_worktree_id(worktree_id)
    })
}
