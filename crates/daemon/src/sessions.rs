use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use cli_master_core::wire::{
    AgentCommand, AgentCustomCreateRequest, AgentDetectRequest, AgentDetectResponse,
    AgentDetection, AgentListResponse, AgentRecord, EmptyResponse, SessionCreateRequest,
    SessionDeleteRequest, SessionIsolation, SessionListRequest, SessionListResponse,
    SessionRenameRequest, SessionResizeRequest, SessionRestartRequest, SessionStartRequest,
    SessionStopRequest, SessionSubscribeRequest, SessionWriteRequest,
};
use cli_master_core::{
    AgentId, AgentSource, ApiError, DaemonInstanceId, Project, Session, SessionId, SessionStatus,
};
use cli_master_session::{
    SessionError, SessionManager, SessionSnapshot, SessionSubscription, TerminalSize,
};
use cli_master_storage::{SessionRuntimeUpdate, Storage, StorageError, StoredAgent, StoredSession};
use uuid::Uuid;

const INITIAL_COLUMNS: u16 = 100;
const INITIAL_ROWS: u16 = 30;
const SHELL_AGENT_ID: u128 = 0x018f_0000_0000_7000_8000_0000_0000_0001;
const CODEX_AGENT_ID: u128 = 0x018f_0000_0000_7000_8000_0000_0000_0002;
const CLAUDE_AGENT_ID: u128 = 0x018f_0000_0000_7000_8000_0000_0000_0003;
const OPENCODE_AGENT_ID: u128 = 0x018f_0000_0000_7000_8000_0000_0000_0004;

/// Owns durable session metadata and the daemon's live PTY processes.
pub(super) struct SessionRegistry {
    storage: Arc<Mutex<Storage>>,
    manager: SessionManager,
    daemon_instance_id: DaemonInstanceId,
}

impl SessionRegistry {
    pub(super) fn new(
        storage: Storage,
        daemon_instance_id: DaemonInstanceId,
    ) -> Result<Self, ApiError> {
        let registry = Self {
            storage: Arc::new(Mutex::new(storage)),
            manager: SessionManager::default(),
            daemon_instance_id,
        };
        registry.seed_builtin_agents()?;
        let now = unix_timestamp_ms()?;
        registry
            .storage()?
            .recover_stale_sessions_for_daemon(&daemon_instance_id.to_string(), now)
            .map_err(storage_error)?;
        Ok(registry)
    }

    pub(super) fn agents(&self) -> Result<Vec<AgentRecord>, ApiError> {
        self.storage()?
            .list_agents()
            .map_err(storage_error)?
            .into_iter()
            .map(agent_record)
            .collect()
    }

    pub(super) fn list_agents(&self) -> Result<AgentListResponse, ApiError> {
        self.agents().map(|agents| AgentListResponse { agents })
    }

    pub(super) fn detect_agents(
        &self,
        request: &AgentDetectRequest,
    ) -> Result<AgentDetectResponse, ApiError> {
        let requested = request.agent_ids();
        let detections = self
            .storage()?
            .list_agents()
            .map_err(storage_error)?
            .into_iter()
            .filter(|agent| {
                agent.enabled && (requested.is_empty() || requested.contains(&agent.id))
            })
            .map(|agent| {
                let executable_path = resolve_executable(&agent.executable);
                AgentDetection {
                    agent_id: agent.id,
                    available: executable_path.is_some(),
                    executable_path,
                    error_code: None,
                }
            })
            .collect();
        Ok(AgentDetectResponse { detections })
    }

    pub(super) fn create_custom_agent(
        &self,
        request: AgentCustomCreateRequest,
    ) -> Result<AgentRecord, ApiError> {
        let now = unix_timestamp_ms()?;
        let agent = StoredAgent {
            id: AgentId::new(),
            source: AgentSource::Custom,
            display_name: request.display_name.into_inner(),
            executable: request.command.executable().to_owned(),
            args: request.command.args().to_vec(),
            env: request.command.env().clone(),
            enabled: true,
            created_at_ms: now,
            updated_at_ms: now,
        };
        self.storage()?
            .insert_agent(&agent)
            .map_err(storage_error)?;
        agent_record(agent)
    }

    pub(super) fn sessions(&self) -> Result<Vec<Session>, ApiError> {
        self.storage()?
            .list_sessions()
            .map_err(storage_error)
            .map(|sessions| sessions.into_iter().map(stored_session).collect())
    }

    pub(super) fn list_sessions(
        &self,
        request: SessionListRequest,
    ) -> Result<SessionListResponse, ApiError> {
        let storage = self.storage()?;
        let sessions = match request.project_id {
            Some(project_id) => storage.list_sessions_for_project(project_id),
            None => storage.list_sessions(),
        }
        .map_err(storage_error)?
        .into_iter()
        .map(stored_session)
        .collect();
        Ok(SessionListResponse { sessions })
    }

    pub(super) fn create(&self, request: SessionCreateRequest) -> Result<Session, ApiError> {
        if request.isolation != SessionIsolation::Current {
            return Err(ApiError::new(
                "worktree_sessions_unavailable",
                "Isolated worktree sessions are not available in this canvas yet.",
            )
            .with_action("Choose the current project directory and try again."));
        }
        let storage = self.storage()?;
        let project = storage
            .get_project(request.project_id)
            .map_err(storage_error)?
            .ok_or_else(|| not_found("project", request.project_id))?;
        let agent = storage
            .get_agent(request.agent_id)
            .map_err(storage_error)?
            .ok_or_else(|| not_found("agent", request.agent_id))?;
        if !agent.enabled {
            return Err(ApiError::new(
                "agent_disabled",
                "The selected terminal command is disabled.",
            )
            .with_action("Enable the command in Settings and try again."));
        }
        let cwd = session_directory(&project, request.relative_directory.as_ref())?;
        let now = unix_timestamp_ms()?;
        let session = StoredSession {
            id: SessionId::new(),
            project_id: request.project_id,
            agent_id: request.agent_id,
            name: request.name.into_inner(),
            cwd,
            status: SessionStatus::Unknown,
            runtime_pid: None,
            daemon_instance_id: None,
            exit_code: None,
            error_code: None,
            created_at_ms: now,
            updated_at_ms: now,
            last_activity_at_ms: None,
        };
        storage.insert_session(&session).map_err(storage_error)?;
        Ok(stored_session(session))
    }

    pub(super) fn rename(&self, request: SessionRenameRequest) -> Result<Session, ApiError> {
        let now = unix_timestamp_ms()?;
        let storage = self.storage()?;
        storage
            .rename_session(request.session_id, request.name.as_str(), now)
            .map_err(storage_error)?;
        load_session(&storage, request.session_id)
    }

    pub(super) fn start(&self, request: SessionStartRequest) -> Result<Session, ApiError> {
        self.start_id(request.session_id)
    }

    pub(super) fn restart(&self, request: SessionRestartRequest) -> Result<Session, ApiError> {
        if let Ok(snapshot) = self.manager.snapshot(request.session_id) {
            if is_live(snapshot.status) {
                self.manager
                    .stop(request.session_id)
                    .map_err(session_error)?;
            }
            self.manager
                .remove(request.session_id)
                .map_err(session_error)?;
        }
        self.start_id(request.session_id)
    }

    pub(super) fn stop(&self, request: SessionStopRequest) -> Result<Session, ApiError> {
        let snapshot = self
            .manager
            .stop(request.session_id)
            .map_err(session_error)?;
        self.persist_snapshot(&snapshot)?;
        self.get(request.session_id)
    }

    pub(super) fn delete(&self, request: SessionDeleteRequest) -> Result<EmptyResponse, ApiError> {
        if let Ok(snapshot) = self.manager.snapshot(request.session_id) {
            if is_live(snapshot.status) {
                return Err(ApiError::new(
                    "session_still_running",
                    "Stop the terminal before deleting it.",
                )
                .with_action("Stop the session, then try deleting it again."));
            }
            self.manager
                .remove(request.session_id)
                .map_err(session_error)?;
        }
        self.storage()?
            .remove_session_metadata(request.session_id)
            .map_err(storage_error)?;
        Ok(EmptyResponse::default())
    }

    pub(super) fn write(&self, request: SessionWriteRequest) -> Result<EmptyResponse, ApiError> {
        let bytes = decode_base64(request.base64.as_str())?;
        self.manager
            .write(request.session_id, &bytes)
            .map_err(session_error)?;
        Ok(EmptyResponse::default())
    }

    pub(super) fn resize(&self, request: SessionResizeRequest) -> Result<EmptyResponse, ApiError> {
        let size =
            TerminalSize::new(request.rows.get(), request.columns.get()).map_err(session_error)?;
        self.manager
            .resize(request.session_id, size)
            .map_err(session_error)?;
        Ok(EmptyResponse::default())
    }

    pub(super) fn subscribe(
        &self,
        request: SessionSubscribeRequest,
    ) -> Result<SessionSubscription, ApiError> {
        self.manager
            .reconnect(
                request.session_id,
                request
                    .cursor
                    .map_or(0, cli_master_core::wire::OutputCursor::get),
            )
            .map_err(session_error)
    }

    pub(super) fn get(&self, session_id: SessionId) -> Result<Session, ApiError> {
        let storage = self.storage()?;
        load_session(&storage, session_id)
    }

    pub(super) fn persist_snapshot(&self, snapshot: &SessionSnapshot) -> Result<(), ApiError> {
        let update = SessionRuntimeUpdate {
            status: snapshot.status,
            runtime_pid: snapshot.pid,
            daemon_instance_id: if is_live(snapshot.status) {
                Some(self.daemon_instance_id.to_string())
            } else {
                None
            },
            exit_code: snapshot.exit_code,
            error_code: None,
            last_activity_at_ms: Some(snapshot.last_activity_at_ms),
            updated_at_ms: unix_timestamp_ms()?,
        };
        self.storage()?
            .update_session_runtime(snapshot.id, &update)
            .map_err(storage_error)
    }

    pub(super) fn persist_current(&self, session_id: SessionId) -> Result<(), ApiError> {
        let snapshot = self.manager.snapshot(session_id).map_err(session_error)?;
        self.persist_snapshot(&snapshot)
    }

    pub(super) fn shutdown(&self) {
        if let Err(error) = self.manager.shutdown() {
            tracing::warn!(%error, "could not cleanly stop all terminal sessions");
        }
    }

    fn start_id(&self, session_id: SessionId) -> Result<Session, ApiError> {
        let storage = self.storage()?;
        let stored = storage
            .get_session(session_id)
            .map_err(storage_error)?
            .ok_or_else(|| not_found("session", session_id))?;
        if let Ok(snapshot) = self.manager.snapshot(session_id) {
            if is_live(snapshot.status) {
                return Ok(stored_session(stored));
            }
            self.manager.remove(session_id).map_err(session_error)?;
        }
        let agent = storage
            .get_agent(stored.agent_id)
            .map_err(storage_error)?
            .ok_or_else(|| not_found("agent", stored.agent_id))?;
        let command = agent.command_for_cwd(&stored.cwd).map_err(storage_error)?;
        drop(storage);
        let size = TerminalSize::new(INITIAL_ROWS, INITIAL_COLUMNS).map_err(session_error)?;
        let handle = self
            .manager
            .spawn_with_id(session_id, &command, size)
            .map_err(session_error)?;
        let snapshot = self.manager.snapshot(session_id).map_err(session_error)?;
        debug_assert_eq!(handle.id, session_id);
        self.persist_snapshot(&snapshot)?;
        self.get(session_id)
    }

    fn seed_builtin_agents(&self) -> Result<(), ApiError> {
        let now = unix_timestamp_ms()?;
        let shell = login_shell();
        let builtins = [
            (SHELL_AGENT_ID, "Shell", shell, vec!["-l".to_owned()]),
            (CODEX_AGENT_ID, "Codex", "codex".to_owned(), Vec::new()),
            (CLAUDE_AGENT_ID, "Claude", "claude".to_owned(), Vec::new()),
            (
                OPENCODE_AGENT_ID,
                "OpenCode",
                "opencode".to_owned(),
                Vec::new(),
            ),
        ];
        let storage = self.storage()?;
        for (raw_id, display_name, executable, args) in builtins {
            let id = AgentId::from_uuid(Uuid::from_u128(raw_id));
            if storage.get_agent(id).map_err(storage_error)?.is_some() {
                continue;
            }
            storage
                .insert_agent(&StoredAgent {
                    id,
                    source: AgentSource::BuiltIn,
                    display_name: display_name.to_owned(),
                    executable,
                    args,
                    env: BTreeMap::new(),
                    enabled: true,
                    created_at_ms: now,
                    updated_at_ms: now,
                })
                .map_err(storage_error)?;
        }
        Ok(())
    }

    fn storage(&self) -> Result<MutexGuard<'_, Storage>, ApiError> {
        self.storage.lock().map_err(|_| {
            ApiError::new(
                "storage_unavailable",
                "Terminal storage is temporarily unavailable.",
            )
            .with_action("Restart Jig and try again.")
        })
    }
}

fn agent_record(agent: StoredAgent) -> Result<AgentRecord, ApiError> {
    Ok(AgentRecord {
        id: agent.id,
        display_name: cli_master_core::wire::DisplayName::try_new(agent.display_name)
            .map_err(validation_error)?,
        description: match agent.executable.as_str() {
            "codex" => Some("OpenAI Codex CLI".to_owned()),
            "claude" => Some("Claude Code CLI".to_owned()),
            "opencode" => Some("OpenCode CLI".to_owned()),
            _ if agent.source == AgentSource::BuiltIn => Some("Local login shell".to_owned()),
            _ => None,
        },
        source: agent.source,
        command: AgentCommand::try_new(agent.executable, agent.args, agent.env)
            .map_err(validation_error)?,
        enabled: agent.enabled,
    })
}

fn stored_session(session: StoredSession) -> Session {
    Session {
        id: session.id,
        project_id: session.project_id,
        name: session.name,
        agent_id: session.agent_id,
        cwd: session.cwd,
        pid: session.runtime_pid,
        pty_id: session.runtime_pid.map(|_| session.id.to_string()),
        branch: None,
        worktree_id: None,
        worktree_path: None,
        status: session.status,
        exit_code: session.exit_code,
        created_at_ms: session.created_at_ms,
        updated_at_ms: session.updated_at_ms,
        last_activity_at_ms: session.last_activity_at_ms,
        error_code: session.error_code,
    }
}

fn load_session(storage: &Storage, session_id: SessionId) -> Result<Session, ApiError> {
    storage
        .get_session(session_id)
        .map_err(storage_error)?
        .map(stored_session)
        .ok_or_else(|| not_found("session", session_id))
}

fn session_directory(
    project: &Project,
    relative_directory: Option<&cli_master_core::wire::RelativeDirectory>,
) -> Result<PathBuf, ApiError> {
    let root = project.repository_root.as_ref().unwrap_or(&project.path);
    let directory =
        relative_directory.map_or_else(|| root.clone(), |relative| root.join(relative.as_str()));
    if !directory.is_dir() {
        return Err(ApiError::new(
            "session_directory_unavailable",
            "The terminal working directory does not exist.",
        )
        .with_action("Choose an existing directory inside the project."));
    }
    directory.canonicalize().map_err(|error| {
        ApiError::new(
            "session_directory_unavailable",
            "The terminal working directory could not be opened.",
        )
        .with_action("Check the directory permissions and try again.")
        .with_detail("reason", error.to_string())
    })
}

fn resolve_executable(executable: &str) -> Option<PathBuf> {
    let path = Path::new(executable);
    if path.is_absolute() {
        return path.is_file().then(|| path.to_path_buf());
    }
    env::split_paths(&env::var_os("PATH")?).find_map(|directory| {
        let candidate = directory.join(executable);
        candidate.is_file().then_some(candidate)
    })
}

fn login_shell() -> String {
    env::var("SHELL")
        .ok()
        .filter(|shell| Path::new(shell).is_absolute() && Path::new(shell).is_file())
        .unwrap_or_else(|| "/bin/sh".to_owned())
}

fn is_live(status: SessionStatus) -> bool {
    matches!(
        status,
        SessionStatus::Starting | SessionStatus::Running | SessionStatus::Idle
    )
}

fn storage_error(error: StorageError) -> ApiError {
    match error {
        StorageError::NotFound { entity, id } => ApiError::new(
            format!("{entity}_not_found"),
            format!("The requested {entity} no longer exists."),
        )
        .with_detail("id", id),
        StorageError::AlreadyExists { entity } => ApiError::new(
            format!("{entity}_already_exists"),
            format!("That {entity} is already registered."),
        ),
        other => ApiError::new("storage_error", "Jig could not update terminal metadata.")
            .with_action("Try again or restart Jig.")
            .with_detail("reason", other.to_string()),
    }
}

fn session_error(error: SessionError) -> ApiError {
    let code = match error {
        SessionError::NotFound { .. } => "session_not_found",
        SessionError::NotLive { .. } | SessionError::InteractionUnavailable { .. } => {
            "session_not_running"
        }
        SessionError::InputBackpressure { .. } => "terminal_input_backpressure",
        SessionError::InputTooLarge { .. } => "terminal_input_too_large",
        SessionError::ReplayCursorAhead { .. } => "terminal_cursor_ahead",
        _ => "terminal_operation_failed",
    };
    ApiError::new(code, "The terminal operation could not be completed.")
        .with_action("Retry the terminal or restart its session.")
        .with_detail("reason", error.to_string())
}

fn validation_error(error: impl std::fmt::Display) -> ApiError {
    ApiError::new(
        "invalid_terminal_configuration",
        "The terminal configuration is invalid.",
    )
    .with_action("Review the command and try again.")
    .with_detail("reason", error.to_string())
}

fn not_found(entity: &'static str, id: impl std::fmt::Display) -> ApiError {
    ApiError::new(
        format!("{entity}_not_found"),
        format!("The requested {entity} no longer exists."),
    )
    .with_detail("id", id.to_string())
}

fn unix_timestamp_ms() -> Result<i64, ApiError> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            ApiError::new("system_clock_invalid", "The system clock is invalid.")
                .with_detail("reason", error.to_string())
        })?
        .as_millis();
    i64::try_from(millis).map_err(|_| {
        ApiError::new(
            "system_clock_invalid",
            "The system clock is outside the supported range.",
        )
    })
}

fn decode_base64(encoded: &str) -> Result<Vec<u8>, ApiError> {
    if encoded.len() % 4 != 0 {
        return Err(invalid_base64());
    }
    let mut decoded = Vec::with_capacity(encoded.len() / 4 * 3);
    for chunk in encoded.as_bytes().chunks_exact(4) {
        let a = base64_value(chunk[0]).ok_or_else(invalid_base64)?;
        let b = base64_value(chunk[1]).ok_or_else(invalid_base64)?;
        let c = if chunk[2] == b'=' {
            0
        } else {
            base64_value(chunk[2]).ok_or_else(invalid_base64)?
        };
        let d = if chunk[3] == b'=' {
            0
        } else {
            base64_value(chunk[3]).ok_or_else(invalid_base64)?
        };
        decoded.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            decoded.push((b << 4) | (c >> 2));
        }
        if chunk[3] != b'=' {
            decoded.push((c << 6) | d);
        }
    }
    Ok(decoded)
}

fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn invalid_base64() -> ApiError {
    ApiError::new(
        "invalid_terminal_input",
        "The terminal input encoding is invalid.",
    )
}

pub(super) fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        encoded.push(TABLE[(a >> 2) as usize] as char);
        encoded.push(TABLE[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
        encoded.push(if chunk.len() > 1 {
            TABLE[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        encoded.push(if chunk.len() > 2 {
            TABLE[(c & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    encoded
}
