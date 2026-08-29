use crate::error::StorageError;

mod agents;
mod projects;
mod sessions;
mod settings;
mod worktrees;

pub(crate) use agents::AgentRepository;
pub(crate) use projects::ProjectRepository;
pub(crate) use sessions::SessionRepository;
pub(crate) use settings::SettingsRepository;
pub(crate) use worktrees::WorktreeRepository;

pub(crate) fn remap_constraint(error: StorageError, conflict: StorageError) -> StorageError {
    if matches!(
        error.kind(),
        crate::error::StorageErrorKind::Sqlite {
            code: Some(rusqlite::ErrorCode::ConstraintViolation),
            ..
        }
    ) {
        conflict
    } else {
        error
    }
}
