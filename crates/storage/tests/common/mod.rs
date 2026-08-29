#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cli_master_core::{
    AgentId, AgentSource, Project, ProjectId, SessionId, SessionStatus, WorktreeId,
};
use cli_master_storage::{StoredAgent, StoredSession, StoredWorktree, WorktreeState};

pub const CREATED_AT_MS: i64 = 1_787_941_200_000;
pub const CREATED_AT: &str = "2026-08-28T18:20:00Z";
pub const OPENED_AT: &str = "2026-08-28T18:20:00.010Z";

pub fn project(name: &str, path: impl Into<PathBuf>) -> Project {
    Project {
        id: ProjectId::new(),
        name: name.to_owned(),
        path: path.into(),
        created_at: CREATED_AT.to_owned(),
        last_opened_at: CREATED_AT.to_owned(),
    }
}

pub fn agent(source: AgentSource, name: &str) -> StoredAgent {
    StoredAgent {
        id: match source {
            AgentSource::BuiltIn => {
                AgentId::parse_str(AgentId::CODEX).expect("built-in ID should be valid")
            }
            AgentSource::Custom => AgentId::new_custom(),
        },
        source,
        display_name: name.to_owned(),
        executable: "codex".to_owned(),
        args: vec!["--quiet".to_owned()],
        env: BTreeMap::new(),
        enabled: true,
        created_at_ms: CREATED_AT_MS,
        updated_at_ms: CREATED_AT_MS,
    }
}

pub fn session(
    project_id: ProjectId,
    agent_id: &AgentId,
    status: SessionStatus,
    daemon_instance_id: Option<&str>,
    runtime_pid: Option<u32>,
) -> StoredSession {
    StoredSession {
        id: SessionId::new(),
        project_id,
        agent_id: agent_id.clone(),
        name: "Session".to_owned(),
        cwd: PathBuf::from("/tmp/cli-master-session"),
        status,
        runtime_pid,
        daemon_instance_id: daemon_instance_id.map(str::to_owned),
        exit_code: None,
        error_code: None,
        created_at_ms: CREATED_AT_MS,
        updated_at_ms: CREATED_AT_MS,
        last_activity_at_ms: Some(CREATED_AT_MS),
    }
}

pub fn worktree(
    project_id: ProjectId,
    session_id: Option<SessionId>,
    path: &Path,
    branch: &str,
) -> StoredWorktree {
    StoredWorktree {
        id: WorktreeId::new(),
        project_id,
        session_id,
        path: path.to_path_buf(),
        branch: branch.to_owned(),
        state: WorktreeState::Creating,
        is_dirty: false,
        created_at_ms: CREATED_AT_MS,
        updated_at_ms: CREATED_AT_MS,
    }
}
