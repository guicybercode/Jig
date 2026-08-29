//! Local daemon: SQLite, Git, agents, and PTY sessions.

#![warn(missing_docs)]
#![allow(clippy::needless_pass_by_value)]

mod frame;
mod paths;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use cli_master_agents::{
    AgentRegistry, CustomAgentAdapter, CustomAgentDefinition, DetectionResult, LaunchContext,
    LaunchEnvironment,
};
use cli_master_core::ipc::{self, codes};
use cli_master_core::{
    AgentId, AgentInfo, AgentRemoveRequest, AgentSource, ApiError, CustomAgentRequest, DaemonHello,
    Diagnostics, GitDiff, GitInspectRequest, PROTOCOL_V1, Project, ProjectAddRequest, ProjectId,
    ProjectRemoveRequest, ProjectRenameRequest, Session, SessionCreateRequest, SessionId,
    SessionIdRequest, SessionRenameRequest, SessionResizeRequest, SessionStatus,
    SessionSubscribeResponse, SessionWriteRequest, StateSnapshot, Worktree, WorktreeCreateRequest,
    WorktreeId, WorktreeIdRequest, WorktreeRemovalPlan, WorktreeRemoveRequest, WorktreeState,
};
use cli_master_git::{GitError, GitService, branch_name, worktree_dir_name};
use cli_master_session::{SessionError, SessionManager};
use cli_master_storage::{SessionRecord, Storage, StorageError, WorktreeRecord, now_rfc3339};
use serde_json::{Value, json};

pub use frame::{MAX_FRAME_BYTES, read_frame, write_frame};
pub use paths::AppPaths;

/// Application version reported by `system.hello`.
pub const APP_VERSION: &str = "0.1.0-beta.1";

/// In-process daemon used by the Unix socket server and tests.
pub struct Daemon {
    paths: AppPaths,
    storage: Mutex<Storage>,
    registry: Mutex<AgentRegistry>,
    git: GitService,
    sessions: Arc<SessionManager>,
    instance_id: String,
    environment: LaunchEnvironment,
    removal_tokens: Mutex<HashMap<String, RemovalToken>>,
}

struct RemovalToken {
    worktree_id: WorktreeId,
    is_dirty: bool,
    in_use: bool,
}

impl Daemon {
    /// Opens storage, seeds agents, and reconciles stale live rows.
    ///
    /// # Errors
    ///
    /// Returns an error when Git, SQLite, or directories cannot be initialized.
    pub fn open(paths: AppPaths) -> Result<Self, ApiError> {
        let git = GitService::from_environment().map_err(git_error)?;
        let mut storage = Storage::open(&paths.database).map_err(storage_error)?;
        storage.migrate().map_err(storage_error)?;
        storage.seed_builtin_agents().map_err(storage_error)?;
        let instance_id = uuid::Uuid::now_v7().to_string();
        storage
            .reconcile_unknown_sessions(&instance_id)
            .map_err(storage_error)?;

        let mut registry = AgentRegistry::new();
        load_custom_agents(&storage, &mut registry)?;

        Ok(Self {
            paths,
            storage: Mutex::new(storage),
            registry: Mutex::new(registry),
            git,
            sessions: Arc::new(SessionManager::new()),
            instance_id,
            environment: LaunchEnvironment::from_current_process_path(),
            removal_tokens: Mutex::new(HashMap::new()),
        })
    }

    /// Returns daemon identity.
    #[must_use]
    pub fn hello(&self) -> DaemonHello {
        DaemonHello {
            protocol_version: PROTOCOL_V1,
            app_version: APP_VERSION.to_owned(),
            instance_id: self.instance_id.clone(),
            platform: current_platform().to_owned(),
        }
    }

    /// Dispatches a version 1 request payload.
    ///
    /// # Errors
    ///
    /// Returns an actionable API error for unknown methods and domain failures.
    pub fn dispatch(&self, method: &str, payload: Value) -> Result<Value, ApiError> {
        match method {
            ipc::SYSTEM_HELLO => serde_json::to_value(self.hello()).map_err(json_error),
            ipc::STATE_SNAPSHOT => serde_json::to_value(self.snapshot()?).map_err(json_error),
            ipc::PROJECT_ADD => self.project_add(parse(payload)?),
            ipc::PROJECT_LIST => serde_json::to_value(self.project_list()?).map_err(json_error),
            ipc::PROJECT_RENAME => self.project_rename(parse(payload)?),
            ipc::PROJECT_REMOVE => self.project_remove(parse(payload)?),
            ipc::AGENT_LIST | ipc::AGENT_DETECT => {
                serde_json::to_value(self.agent_list()?).map_err(json_error)
            }
            ipc::AGENT_CUSTOM_CREATE => self.agent_custom_create(parse(payload)?),
            ipc::AGENT_CUSTOM_UPDATE => self.agent_custom_update(parse(payload)?),
            ipc::AGENT_CUSTOM_REMOVE => self.agent_custom_remove(parse(payload)?),
            ipc::SESSION_CREATE => self.session_create(parse(payload)?),
            ipc::SESSION_LIST => self.session_list(payload),
            ipc::SESSION_GET => self.session_get(parse(payload)?),
            ipc::SESSION_SUBSCRIBE => self.session_subscribe(parse(payload)?),
            ipc::SESSION_UNSUBSCRIBE => Ok(json!({ "ok": true })),
            ipc::SESSION_WRITE => self.session_write(parse(payload)?),
            ipc::SESSION_RESIZE => self.session_resize(parse(payload)?),
            ipc::SESSION_STOP => self.session_stop(parse(payload)?, false),
            ipc::SESSION_KILL => self.session_stop(parse(payload)?, true),
            ipc::SESSION_RESTART => self.session_restart(parse(payload)?),
            ipc::SESSION_RENAME => self.session_rename(parse(payload)?),
            ipc::SESSION_DELETE => self.session_delete(parse(payload)?),
            ipc::GIT_STATUS => self.git_status(parse(payload)?),
            ipc::GIT_DIFF => self.git_diff(parse(payload)?),
            ipc::WORKTREE_CREATE => self.worktree_create(parse(payload)?),
            ipc::WORKTREE_LIST => self.worktree_list(payload),
            ipc::WORKTREE_PREPARE_REMOVE => self.worktree_prepare_remove(parse(payload)?),
            ipc::WORKTREE_REMOVE => self.worktree_remove(parse(payload)?),
            ipc::DIAGNOSTICS_GET | ipc::DIAGNOSTICS_EXPORT => {
                serde_json::to_value(self.diagnostics()).map_err(json_error)
            }
            _ => Err(ApiError::new(codes::METHOD_NOT_FOUND, "Unknown method")
                .with_action("Use a documented Beta v0.1 method name")
                .with_detail("method", method)),
        }
    }

    /// Builds the current metadata snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error when storage or Git inspection fails.
    pub fn snapshot(&self) -> Result<StateSnapshot, ApiError> {
        Ok(StateSnapshot {
            daemon: self.hello(),
            projects: self.project_list()?,
            agents: self.agent_list()?,
            sessions: self.all_sessions()?,
            worktrees: self.all_worktrees()?,
        })
    }

    /// Live PTY manager for subscribers.
    #[must_use]
    pub fn session_manager(&self) -> Arc<SessionManager> {
        Arc::clone(&self.sessions)
    }

    /// Resolved filesystem locations.
    #[must_use]
    pub fn paths(&self) -> &AppPaths {
        &self.paths
    }

    fn project_add(&self, request: ProjectAddRequest) -> Result<Value, ApiError> {
        let selected =
            fs::canonicalize(&request.path).map_err(|error| path_error(&request.path, error))?;
        if !selected.is_dir() {
            return Err(
                ApiError::new(codes::PATH_UNREADABLE, "Selected path is not a directory")
                    .with_action("Choose an existing Git repository directory"),
            );
        }
        let root = self.git.repository_root(&selected).map_err(git_error)?;
        let storage = self.storage();
        if let Some(existing) = storage.project_id_for_path(&root).map_err(storage_error)? {
            return Err(ApiError::new(
                codes::PROJECT_DUPLICATE,
                "This repository is already added",
            )
            .with_detail("projectId", existing.to_string()));
        }
        let name = request.name.unwrap_or_else(|| {
            root.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Project")
                .to_owned()
        });
        if name.trim().is_empty() {
            return Err(ApiError::new(
                codes::INVALID_REQUEST,
                "Project name must not be empty",
            ));
        }
        let id = ProjectId::new();
        storage
            .insert_project(id, name.trim(), &root)
            .map_err(storage_error)?;
        drop(storage);
        serde_json::to_value(self.project_dto(id)?).map_err(json_error)
    }

    fn project_list(&self) -> Result<Vec<Project>, ApiError> {
        let records = self.storage().list_projects().map_err(storage_error)?;
        records
            .into_iter()
            .map(|record| Ok(self.enrich_project(record.into_project().map_err(storage_error)?)))
            .collect()
    }

    fn project_rename(&self, request: ProjectRenameRequest) -> Result<Value, ApiError> {
        if request.name.trim().is_empty() {
            return Err(ApiError::new(
                codes::INVALID_REQUEST,
                "Project name must not be empty",
            ));
        }
        self.storage()
            .rename_project(request.project_id, request.name.trim())
            .map_err(storage_error)?;
        serde_json::to_value(self.project_dto(request.project_id)?).map_err(json_error)
    }

    fn project_remove(&self, request: ProjectRemoveRequest) -> Result<Value, ApiError> {
        let storage = self.storage();
        if storage
            .project_has_sessions(request.project_id)
            .map_err(storage_error)?
            || storage
                .project_has_worktrees(request.project_id)
                .map_err(storage_error)?
        {
            return Err(ApiError::new(
                codes::PROJECT_IN_USE,
                "Remove sessions and worktrees before removing the project",
            )
            .with_action("Stop and delete sessions, then remove worktrees"));
        }
        storage
            .delete_project(request.project_id)
            .map_err(storage_error)?;
        Ok(json!({ "removed": true, "projectId": request.project_id }))
    }

    fn agent_list(&self) -> Result<Vec<AgentInfo>, ApiError> {
        let records = self.storage().list_agents().map_err(storage_error)?;
        let mut agents = Vec::new();
        for record in records {
            let detected = record.enabled
                && matches!(
                    self.environment.detect(&record.executable),
                    DetectionResult::Found { .. }
                );
            let args = record.args().map_err(storage_error)?;
            agents.push(AgentInfo {
                id: record.id,
                display_name: record.name,
                source: record.source,
                enabled: record.enabled,
                detected,
                executable: record.executable,
                args,
            });
        }
        Ok(agents)
    }

    fn agent_custom_create(&self, request: CustomAgentRequest) -> Result<Value, ApiError> {
        let key = request.key.unwrap_or_else(|| AgentId::new().to_string());
        let definition = CustomAgentDefinition::try_from_parts(
            key,
            request.display_name,
            request.executable,
            request.args,
            std::collections::BTreeMap::new(),
        )
        .map_err(|error| {
            ApiError::new(codes::AGENT_INVALID, error.to_string())
                .with_action("Use an absolute path or a bare executable name")
        })?;
        let id = AgentId::from_key(definition.key())
            .map_err(|error| ApiError::new(codes::AGENT_INVALID, error.to_string()))?;
        let args_json = serde_json::to_string(definition.args()).map_err(json_error)?;
        self.storage()
            .insert_custom_agent(
                &id,
                definition.display_name(),
                definition.executable(),
                &args_json,
                "{}",
            )
            .map_err(storage_error)?;
        self.reload_registry()?;
        serde_json::to_value(self.created_agent(&id)?).map_err(json_error)
    }

    fn agent_custom_update(&self, request: CustomAgentRequest) -> Result<Value, ApiError> {
        let key = request
            .key
            .ok_or_else(|| ApiError::new(codes::INVALID_REQUEST, "Custom agent key is required"))?;
        let definition = CustomAgentDefinition::try_from_parts(
            &key,
            request.display_name,
            request.executable,
            request.args,
            std::collections::BTreeMap::new(),
        )
        .map_err(|error| ApiError::new(codes::AGENT_INVALID, error.to_string()))?;
        let id = AgentId::from_key(&key)
            .map_err(|error| ApiError::new(codes::AGENT_INVALID, error.to_string()))?;
        let record = self
            .storage()
            .get_agent(&id)
            .map_err(storage_error)?
            .ok_or_else(|| ApiError::new(codes::AGENT_INVALID, "Agent was not found"))?;
        if record.source != AgentSource::Custom {
            return Err(ApiError::new(
                codes::AGENT_BUILTIN_READONLY,
                "Built-in agents cannot be edited",
            ));
        }
        let args_json = serde_json::to_string(definition.args()).map_err(json_error)?;
        self.storage()
            .update_custom_agent(
                &id,
                definition.display_name(),
                definition.executable(),
                &args_json,
                "{}",
            )
            .map_err(storage_error)?;
        self.reload_registry()?;
        serde_json::to_value(self.created_agent(&id)?).map_err(json_error)
    }

    fn agent_custom_remove(&self, request: AgentRemoveRequest) -> Result<Value, ApiError> {
        let storage = self.storage();
        let record = storage
            .get_agent(&request.agent_id)
            .map_err(storage_error)?
            .ok_or_else(|| ApiError::new(codes::AGENT_INVALID, "Agent was not found"))?;
        if record.source != AgentSource::Custom {
            return Err(ApiError::new(
                codes::AGENT_BUILTIN_READONLY,
                "Built-in agents cannot be removed",
            ));
        }
        if storage
            .agent_in_use(&request.agent_id)
            .map_err(storage_error)?
        {
            return Err(ApiError::new(
                codes::AGENT_IN_USE,
                "This agent is still referenced by a session",
            ));
        }
        storage
            .delete_custom_agent(&request.agent_id)
            .map_err(storage_error)?;
        drop(storage);
        self.reload_registry()?;
        Ok(json!({ "removed": true }))
    }

    fn session_create(&self, request: SessionCreateRequest) -> Result<Value, ApiError> {
        let project = self.project_dto(request.project_id)?;
        let root = project
            .repository_root
            .clone()
            .unwrap_or_else(|| project.path.clone());
        if !root.is_dir() {
            return Err(path_error(
                &root,
                std::io::Error::new(std::io::ErrorKind::NotFound, "project directory is missing"),
            ));
        }
        let agent = self
            .storage()
            .get_agent(&request.agent_id)
            .map_err(storage_error)?
            .ok_or_else(|| {
                ApiError::new(codes::AGENT_INVALID, "Agent was not found")
                    .with_action("Detect agents or register a custom executable")
            })?;
        if !agent.enabled {
            return Err(ApiError::new(codes::AGENT_INVALID, "Agent is disabled"));
        }

        let session_id = SessionId::new();
        let name = request
            .name
            .unwrap_or_else(|| format!("{} session", agent.name));
        let (cwd, worktree_id) = if request.create_worktree {
            let created = self.create_managed_worktree(
                request.project_id,
                &root,
                request.worktree_slug.as_deref().unwrap_or(&name),
                None,
            )?;
            (created.path, Some(created.id))
        } else {
            (root.clone(), None)
        };

        let now = now_rfc3339();
        self.storage()
            .insert_session(&SessionRecord {
                id: session_id,
                project_id: request.project_id,
                agent_id: request.agent_id.clone(),
                name: name.clone(),
                cwd: cwd.clone(),
                status: SessionStatus::Starting,
                runtime_pid: None,
                daemon_instance_id: Some(self.instance_id.clone()),
                exit_code: None,
                error_code: None,
                created_at: now.clone(),
                updated_at: now,
                last_activity_at: None,
            })
            .map_err(storage_error)?;
        if let Some(worktree_id) = worktree_id {
            self.storage()
                .update_worktree(worktree_id, WorktreeState::Active, Some(session_id))
                .map_err(storage_error)?;
        }

        let command = self.build_command(&request.agent_id, &cwd)?;
        match self
            .sessions
            .start(session_id, &command, request.cols, request.rows)
        {
            Ok(pid) => {
                self.storage()
                    .update_session_runtime(
                        session_id,
                        SessionStatus::Running,
                        Some(i64::from(pid)),
                        Some(&self.instance_id),
                        None,
                        None,
                    )
                    .map_err(storage_error)?;
            }
            Err(error) => {
                let api = session_error(error);
                self.storage()
                    .update_session_runtime(
                        session_id,
                        SessionStatus::Failed,
                        None,
                        Some(&self.instance_id),
                        None,
                        Some(&api.code),
                    )
                    .map_err(storage_error)?;
                return Err(api);
            }
        }
        serde_json::to_value(self.session_dto(session_id)?).map_err(json_error)
    }

    fn session_list(&self, payload: Value) -> Result<Value, ApiError> {
        let project_id = payload
            .get("projectId")
            .and_then(Value::as_str)
            .and_then(|value| value.parse().ok());
        let sessions = if let Some(project_id) = project_id {
            self.sessions_for_project(project_id)?
        } else {
            self.all_sessions()?
        };
        serde_json::to_value(sessions).map_err(json_error)
    }

    fn session_get(&self, request: SessionIdRequest) -> Result<Value, ApiError> {
        serde_json::to_value(self.session_dto(request.session_id)?).map_err(json_error)
    }

    fn session_subscribe(&self, request: SessionIdRequest) -> Result<Value, ApiError> {
        let session = self.session_dto(request.session_id)?;
        if let Some(snapshot) = self.sessions.replay(request.session_id) {
            return serde_json::to_value(SessionSubscribeResponse {
                session,
                last_sequence: snapshot.last_sequence,
                replay_base64: BASE64.encode(snapshot.bytes),
            })
            .map_err(json_error);
        }
        serde_json::to_value(SessionSubscribeResponse {
            session,
            last_sequence: 0,
            replay_base64: String::new(),
        })
        .map_err(json_error)
    }

    fn session_write(&self, request: SessionWriteRequest) -> Result<Value, ApiError> {
        let bytes = BASE64
            .decode(request.bytes_base64.as_bytes())
            .map_err(|error| {
                ApiError::new(
                    codes::INVALID_REQUEST,
                    format!("Invalid base64 input: {error}"),
                )
            })?;
        self.sessions
            .write(request.session_id, &bytes)
            .map_err(session_error)?;
        Ok(json!({ "ok": true }))
    }

    fn session_resize(&self, request: SessionResizeRequest) -> Result<Value, ApiError> {
        self.sessions
            .resize(request.session_id, request.cols, request.rows)
            .map_err(session_error)?;
        Ok(json!({ "ok": true }))
    }

    fn session_stop(&self, request: SessionIdRequest, force: bool) -> Result<Value, ApiError> {
        let result = if force {
            self.sessions.kill(request.session_id)
        } else {
            self.sessions.stop(request.session_id)
        };
        match result {
            Ok(()) => {}
            Err(SessionError::NotRunning) => {
                return Err(session_error(SessionError::NotRunning));
            }
            Err(error) => return Err(session_error(error)),
        }
        self.storage()
            .update_session_runtime(
                request.session_id,
                SessionStatus::Exited,
                None,
                Some(&self.instance_id),
                Some(0),
                None,
            )
            .map_err(storage_error)?;
        serde_json::to_value(self.session_dto(request.session_id)?).map_err(json_error)
    }

    fn session_restart(&self, request: SessionIdRequest) -> Result<Value, ApiError> {
        let record = self
            .storage()
            .get_session(request.session_id)
            .map_err(storage_error)?
            .ok_or_else(|| ApiError::new(codes::SESSION_NOT_FOUND, "Session was not found"))?;
        if self.sessions.is_live(request.session_id) {
            return Err(ApiError::new(
                codes::SESSION_ALREADY_RUNNING,
                "Stop the session before restarting it",
            ));
        }
        let command = self.build_command(&record.agent_id, &record.cwd)?;
        let pid = self
            .sessions
            .start(request.session_id, &command, 80, 24)
            .map_err(session_error)?;
        self.storage()
            .update_session_runtime(
                request.session_id,
                SessionStatus::Running,
                Some(i64::from(pid)),
                Some(&self.instance_id),
                None,
                None,
            )
            .map_err(storage_error)?;
        serde_json::to_value(self.session_dto(request.session_id)?).map_err(json_error)
    }

    fn session_rename(&self, request: SessionRenameRequest) -> Result<Value, ApiError> {
        if request.name.trim().is_empty() {
            return Err(ApiError::new(
                codes::INVALID_REQUEST,
                "Session name must not be empty",
            ));
        }
        self.storage()
            .rename_session(request.session_id, request.name.trim())
            .map_err(storage_error)?;
        serde_json::to_value(self.session_dto(request.session_id)?).map_err(json_error)
    }

    fn session_delete(&self, request: SessionIdRequest) -> Result<Value, ApiError> {
        if self.sessions.is_live(request.session_id) {
            return Err(ApiError::new(
                codes::SESSION_STILL_RUNNING,
                "Stop the session before deleting its metadata",
            ));
        }
        let worktree = self
            .storage()
            .worktree_for_session(request.session_id)
            .map_err(storage_error)?;
        if let Some(worktree) = worktree {
            self.storage()
                .update_worktree(worktree.id, worktree.state, None)
                .map_err(storage_error)?;
        }
        self.storage()
            .delete_session(request.session_id)
            .map_err(storage_error)?;
        Ok(json!({ "deleted": true }))
    }

    fn git_status(&self, request: GitInspectRequest) -> Result<Value, ApiError> {
        let path = self.git_target_path(request.project_id, request.worktree_id)?;
        let status = self.git.status(&path).map_err(git_error)?;
        serde_json::to_value(status).map_err(json_error)
    }

    fn git_diff(&self, request: GitInspectRequest) -> Result<Value, ApiError> {
        let path = self.git_target_path(request.project_id, request.worktree_id)?;
        let diff = self.git.diff(&path).map_err(git_error)?;
        serde_json::to_value(GitDiff {
            text: diff.text,
            truncated: diff.truncated,
        })
        .map_err(json_error)
    }

    fn worktree_create(&self, request: WorktreeCreateRequest) -> Result<Value, ApiError> {
        let project = self.project_dto(request.project_id)?;
        let root = project.repository_root.unwrap_or(project.path);
        let slug = request.slug.unwrap_or_else(|| "session".to_owned());
        let created = self.create_managed_worktree(request.project_id, &root, &slug, None)?;
        serde_json::to_value(created).map_err(json_error)
    }

    fn worktree_list(&self, payload: Value) -> Result<Value, ApiError> {
        let project_id = payload
            .get("projectId")
            .and_then(Value::as_str)
            .and_then(|value| value.parse().ok());
        let worktrees = if let Some(project_id) = project_id {
            self.worktrees_for_project(project_id)?
        } else {
            self.all_worktrees()?
        };
        serde_json::to_value(worktrees).map_err(json_error)
    }

    fn worktree_prepare_remove(&self, request: WorktreeIdRequest) -> Result<Value, ApiError> {
        let record = self
            .storage()
            .get_worktree(request.worktree_id)
            .map_err(storage_error)?
            .ok_or_else(|| ApiError::new(codes::INVALID_REQUEST, "Worktree was not found"))?;
        let in_use = record
            .session_id
            .is_some_and(|id| self.sessions.is_live(id));
        let status = self.git.status(&record.path).map_err(git_error)?;
        let token = uuid::Uuid::now_v7().to_string();
        self.removal_tokens
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(
                token.clone(),
                RemovalToken {
                    worktree_id: request.worktree_id,
                    is_dirty: status.is_dirty,
                    in_use,
                },
            );
        serde_json::to_value(WorktreeRemovalPlan {
            worktree_id: request.worktree_id,
            in_use,
            is_dirty: status.is_dirty,
            confirmation_token: token,
        })
        .map_err(json_error)
    }

    fn worktree_remove(&self, request: WorktreeRemoveRequest) -> Result<Value, ApiError> {
        let token = self
            .removal_tokens
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&request.confirmation_token);
        let Some(token) = token else {
            return Err(ApiError::new(
                codes::WORKTREE_CONFIRMATION_REQUIRED,
                "Removal confirmation expired or does not match",
            )
            .with_action("Inspect the worktree again, then confirm removal"));
        };
        if token.worktree_id != request.worktree_id {
            return Err(ApiError::new(
                codes::WORKTREE_CONFIRMATION_REQUIRED,
                "Confirmation token is for a different worktree",
            ));
        }
        if token.in_use {
            return Err(ApiError::new(
                codes::WORKTREE_IN_USE,
                "Stop the session before removing this worktree",
            ));
        }
        let record = self
            .storage()
            .get_worktree(request.worktree_id)
            .map_err(storage_error)?
            .ok_or_else(|| ApiError::new(codes::INVALID_REQUEST, "Worktree was not found"))?;
        if record
            .session_id
            .is_some_and(|id| self.sessions.is_live(id))
        {
            return Err(ApiError::new(
                codes::WORKTREE_IN_USE,
                "Stop the session before removing this worktree",
            ));
        }
        if token.is_dirty && !request.allow_dirty {
            return Err(
                ApiError::new(codes::WORKTREE_DIRTY, "Worktree has uncommitted changes")
                    .with_action("Commit or stash changes, or confirm a dirty removal"),
            );
        }
        let project = self.project_dto(record.project_id)?;
        let root = project.repository_root.unwrap_or(project.path);
        self.git
            .remove_worktree(&root, &record.path, request.allow_dirty)
            .map_err(git_error)?;
        self.storage()
            .delete_worktree(request.worktree_id)
            .map_err(storage_error)?;
        Ok(json!({ "removed": true }))
    }

    fn diagnostics(&self) -> Diagnostics {
        Diagnostics {
            daemon: self.hello(),
            database_path: self.paths.database.clone(),
            log_dir: self.paths.log_dir.clone(),
            socket_path: self.paths.socket.clone(),
            search_paths: self.environment.search_paths().to_vec(),
            live_session_count: u32::try_from(self.sessions.live_count()).unwrap_or(u32::MAX),
            recent_log_lines: Vec::new(),
        }
    }

    fn create_managed_worktree(
        &self,
        project_id: ProjectId,
        repository: &Path,
        slug: &str,
        session_id: Option<SessionId>,
    ) -> Result<Worktree, ApiError> {
        let suffix = suffix();
        let branch = branch_name(slug, &suffix);
        let directory = worktree_dir_name(slug, &suffix);
        let path = self
            .paths
            .worktrees
            .join(project_id.to_string())
            .join(directory);
        let now = now_rfc3339();
        let id = WorktreeId::new();
        self.storage()
            .insert_worktree(&WorktreeRecord {
                id,
                project_id,
                session_id,
                path: path.clone(),
                branch: branch.clone(),
                state: WorktreeState::Creating,
                created_at: now.clone(),
                updated_at: now,
            })
            .map_err(storage_error)?;
        self.git
            .create_worktree(repository, &self.paths.worktrees, &path, &branch)
            .map_err(|error| {
                let _ = self.storage().delete_worktree(id);
                git_error(error)
            })?;
        self.storage()
            .update_worktree(id, WorktreeState::Active, session_id)
            .map_err(storage_error)?;
        self.worktree_dto(id)
    }

    fn build_command(
        &self,
        agent_id: &AgentId,
        cwd: &Path,
    ) -> Result<cli_master_core::CommandSpec, ApiError> {
        let registry = self.registry();
        let adapter = registry
            .get(agent_id.as_str())
            .ok_or_else(|| ApiError::new(codes::AGENT_INVALID, "Agent is not registered"))?;
        let context = LaunchContext::new(cwd, self.environment.clone());
        cli_master_agents::AgentAdapter::build_command(adapter, &context).map_err(|error| {
            ApiError::new(codes::AGENT_EXECUTABLE_NOT_FOUND, error.to_string())
                .with_action("Install the CLI or fix the custom executable path")
        })
    }

    fn git_target_path(
        &self,
        project_id: ProjectId,
        worktree_id: Option<WorktreeId>,
    ) -> Result<PathBuf, ApiError> {
        if let Some(worktree_id) = worktree_id {
            let record = self
                .storage()
                .get_worktree(worktree_id)
                .map_err(storage_error)?
                .ok_or_else(|| ApiError::new(codes::INVALID_REQUEST, "Worktree was not found"))?;
            return Ok(record.path);
        }
        let project = self.project_dto(project_id)?;
        Ok(project.repository_root.unwrap_or(project.path))
    }

    fn project_dto(&self, id: ProjectId) -> Result<Project, ApiError> {
        let record = self
            .storage()
            .get_project(id)
            .map_err(storage_error)?
            .ok_or_else(|| ApiError::new(codes::INVALID_REQUEST, "Project was not found"))?;
        Ok(self.enrich_project(record.into_project().map_err(storage_error)?))
    }

    fn enrich_project(&self, mut project: Project) -> Project {
        if !project.path.is_dir() {
            project.current_branch = None;
            return project;
        }
        if let Ok(root) = self.git.repository_root(&project.path) {
            project.repository_root = Some(root.clone());
            project.current_branch = self.git.current_branch(&root).ok();
        }
        project
    }

    fn session_dto(&self, id: SessionId) -> Result<Session, ApiError> {
        let record = self
            .storage()
            .get_session(id)
            .map_err(storage_error)?
            .ok_or_else(|| ApiError::new(codes::SESSION_NOT_FOUND, "Session was not found"))?;
        self.session_from_record(record)
    }

    fn session_from_record(&self, record: SessionRecord) -> Result<Session, ApiError> {
        let worktree = self
            .storage()
            .worktree_for_session(record.id)
            .map_err(storage_error)?;
        let mut status = record.status;
        if matches!(
            status,
            SessionStatus::Running | SessionStatus::Idle | SessionStatus::Starting
        ) && !self.sessions.is_live(record.id)
            && record.daemon_instance_id.as_deref() == Some(self.instance_id.as_str())
        {
            status = SessionStatus::Exited;
        }
        let branch = worktree
            .as_ref()
            .map(|tree| tree.branch.clone())
            .or_else(|| self.git.current_branch(&record.cwd).ok());
        let mut session = record
            .into_session(
                worktree.as_ref().map(|tree| tree.id),
                worktree.as_ref().map(|tree| tree.path.clone()),
                branch,
            )
            .map_err(storage_error)?;
        session.status = status;
        if self.sessions.is_live(session.id) {
            session.status = SessionStatus::Running;
        }
        Ok(session)
    }

    fn all_sessions(&self) -> Result<Vec<Session>, ApiError> {
        let records = self.storage().list_sessions(None).map_err(storage_error)?;
        records
            .into_iter()
            .map(|record| self.session_from_record(record))
            .collect()
    }

    fn sessions_for_project(&self, project_id: ProjectId) -> Result<Vec<Session>, ApiError> {
        let records = self
            .storage()
            .list_sessions(Some(project_id))
            .map_err(storage_error)?;
        records
            .into_iter()
            .map(|record| self.session_from_record(record))
            .collect()
    }

    fn worktree_dto(&self, id: WorktreeId) -> Result<Worktree, ApiError> {
        let record = self
            .storage()
            .get_worktree(id)
            .map_err(storage_error)?
            .ok_or_else(|| ApiError::new(codes::INVALID_REQUEST, "Worktree was not found"))?;
        self.worktree_from_record(record)
    }

    fn worktree_from_record(&self, record: WorktreeRecord) -> Result<Worktree, ApiError> {
        let dirty = self
            .git
            .status(&record.path)
            .is_ok_and(|status| status.is_dirty);
        record.into_worktree(dirty).map_err(storage_error)
    }

    fn all_worktrees(&self) -> Result<Vec<Worktree>, ApiError> {
        let records = self.storage().list_worktrees(None).map_err(storage_error)?;
        records
            .into_iter()
            .map(|record| self.worktree_from_record(record))
            .collect()
    }

    fn worktrees_for_project(&self, project_id: ProjectId) -> Result<Vec<Worktree>, ApiError> {
        let records = self
            .storage()
            .list_worktrees(Some(project_id))
            .map_err(storage_error)?;
        records
            .into_iter()
            .map(|record| self.worktree_from_record(record))
            .collect()
    }

    fn created_agent(&self, id: &AgentId) -> Result<AgentInfo, ApiError> {
        self.agent_list()?
            .into_iter()
            .find(|agent| agent.id == *id)
            .ok_or_else(|| ApiError::new(codes::AGENT_INVALID, "Agent was not found"))
    }

    fn reload_registry(&self) -> Result<(), ApiError> {
        let mut registry = AgentRegistry::new();
        {
            let storage = self.storage();
            load_custom_agents(&storage, &mut registry)?;
        }
        *self.registry() = registry;
        Ok(())
    }

    fn storage(&self) -> std::sync::MutexGuard<'_, Storage> {
        self.storage
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn registry(&self) -> std::sync::MutexGuard<'_, AgentRegistry> {
        self.registry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

fn current_platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

fn parse<T: serde::de::DeserializeOwned>(payload: Value) -> Result<T, ApiError> {
    serde_json::from_value(payload).map_err(|error| {
        ApiError::new(codes::INVALID_REQUEST, "Request payload is invalid")
            .with_detail("reason", error.to_string())
    })
}

fn json_error(error: serde_json::Error) -> ApiError {
    ApiError::new(codes::INVALID_REQUEST, error.to_string())
}

fn storage_error(error: StorageError) -> ApiError {
    match error {
        StorageError::NotFound(kind) => {
            ApiError::new(codes::INVALID_REQUEST, format!("{kind} was not found"))
        }
        StorageError::Database(ref inner) if inner.to_string().contains("UNIQUE") => {
            ApiError::new(codes::PROJECT_DUPLICATE, error.to_string())
        }
        other => ApiError::new(codes::DATABASE_UNAVAILABLE, other.to_string())
            .with_action("Check that the data directory is writable"),
    }
}

fn git_error(error: GitError) -> ApiError {
    match error {
        GitError::NotFound => ApiError::new(
            codes::GIT_NOT_FOUND,
            "Git is not installed or not executable",
        )
        .with_action("Install Git and ensure it is on PATH"),
        GitError::NotARepository { path } => ApiError::new(
            codes::GIT_NOT_A_REPOSITORY,
            format!("Not a Git repository: {}", path.display()),
        )
        .with_action("Select a directory inside a Git working tree"),
        GitError::Unreadable { path, source } => path_error(&path, source),
        GitError::AlreadyExists { target } => {
            ApiError::new(codes::WORKTREE_EXISTS, format!("{target} already exists"))
                .with_action("Choose a different session name")
        }
        GitError::Dirty { path } => ApiError::new(
            codes::WORKTREE_DIRTY,
            format!("Worktree has uncommitted changes: {}", path.display()),
        )
        .with_action("Commit or stash changes, or confirm a dirty removal"),
        GitError::PathEscaped { path } => ApiError::new(
            codes::WORKTREE_PATH_INVALID,
            format!(
                "Worktree path is outside the managed root: {}",
                path.display()
            ),
        ),
        GitError::CommandFailed { message, .. } => {
            ApiError::new(codes::GIT_COMMAND_FAILED, message)
                .with_action("Retry the Git operation or inspect the repository on disk")
        }
    }
}

fn path_error(path: &Path, source: std::io::Error) -> ApiError {
    let code = if source.kind() == std::io::ErrorKind::NotFound {
        codes::PROJECT_MOVED
    } else {
        codes::PATH_UNREADABLE
    };
    ApiError::new(code, format!("Cannot read {}: {source}", path.display()))
        .with_action("Confirm the directory exists and is readable")
}

fn session_error(error: SessionError) -> ApiError {
    match error {
        SessionError::AlreadyRunning => ApiError::new(
            codes::SESSION_ALREADY_RUNNING,
            "This session is already starting or running",
        ),
        SessionError::NotRunning => {
            ApiError::new(codes::SESSION_NOT_RUNNING, "This session is not running")
        }
        SessionError::Spawn(message) => ApiError::new(codes::AGENT_EXECUTABLE_NOT_FOUND, message)
            .with_action("Install the agent CLI or register a custom executable"),
        SessionError::Write(message) | SessionError::Resize(message) => {
            ApiError::new(codes::INVALID_REQUEST, message)
        }
    }
}

fn load_custom_agents(storage: &Storage, registry: &mut AgentRegistry) -> Result<(), ApiError> {
    for record in storage.list_agents().map_err(storage_error)? {
        if record.source != AgentSource::Custom {
            continue;
        }
        let args = record.args().map_err(storage_error)?;
        let definition = CustomAgentDefinition::try_from_parts(
            record.id.as_str(),
            record.name,
            record.executable,
            args,
            std::collections::BTreeMap::new(),
        )
        .map_err(|error| ApiError::new(codes::AGENT_INVALID, error.to_string()))?;
        let _ = registry.register(CustomAgentAdapter::new(definition));
    }
    Ok(())
}

fn suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(1, |duration| duration.subsec_nanos());
    format!("{:x}", nanos % 0x1_0000)
}
