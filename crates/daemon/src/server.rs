use std::env;
use std::fs;
use std::io;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use cli_master_core::wire::{self, EmptyRequest, HelloResponse, StateSnapshotResponse};
use cli_master_core::{
    ApiError, DaemonInstanceId, EnvelopeKind, EventEnvelope, PROTOCOL_V1, Project, RequestEnvelope,
    RequestId, ResponseEnvelope, Session, Worktree,
    wire::{
        AgentCustomCreateRequest, AgentDetectRequest, AgentRecord, DiagnosticsResponse,
        EmptyResponse, OutputCursor, OutputSequence, ProjectAddRequest, ProjectRemoveRequest,
        ProjectRenameRequest, PtyOutputBase64, SessionCreateRequest, SessionDeleteRequest,
        SessionExitedEvent, SessionListRequest, SessionOutputEvent, SessionOutputGapEvent,
        SessionRenameRequest, SessionReplayCompleteEvent, SessionResizeRequest,
        SessionRestartRequest, SessionStartRequest, SessionStatusChangedEvent, SessionStopRequest,
        SessionSubscribeRequest, SessionWriteRequest, event_name, method,
    },
};
use cli_master_session::{SessionEvent, SessionSubscription, StatusChangeReason};
use cli_master_storage::Storage;
use futures_util::{SinkExt, StreamExt};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::net::{UnixListener, UnixStream};
use tokio::task::JoinSet;
use tokio_util::codec::LengthDelimitedCodec;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::lock::InstanceLock;
use crate::projects::ProjectRegistry;
use crate::sessions::{SessionRegistry, encode_base64};
use crate::{DaemonConfig, DaemonError};

/// Largest accepted JSON frame, excluding the four-byte length prefix.
pub const MAX_FRAME_LENGTH: usize = 1024 * 1024;

/// Successful `system.hello` response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HelloResponse {
    /// IPC protocol version spoken by this daemon.
    pub protocol_version: u16,
    /// Semantic version of the daemon executable.
    pub daemon_version: String,
    /// Unique identifier regenerated for each daemon process lifetime.
    pub instance_id: Uuid,
}

/// Durable state returned by `state.snapshot` during initial client sync.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateSnapshot {
    /// Applied `SQLite` schema migration version.
    pub schema_version: u32,
    /// Registered projects. Empty until project persistence is wired in.
    pub projects: Vec<Project>,
    /// Available agent definitions. Empty until registry persistence is wired in.
    pub agents: Vec<AgentRecord>,
    /// Known sessions. Empty until the session manager is wired in.
    pub sessions: Vec<Session>,
    /// Managed worktrees. Empty until Git orchestration is wired in.
    pub worktrees: Vec<Worktree>,
}

struct ServerState {
    hello: HelloResponse,
    schema_version: u32,
    diagnostics: DiagnosticsResponse,
    projects: ProjectRegistry,
    sessions: SessionRegistry,
    event_sequence: AtomicU64,
}

/// Bound, single-instance local daemon.
///
/// Construct with [`Self::bind`], then call [`Self::run`] until the supplied
/// cancellation token is cancelled. Binding performs all startup invariants:
/// private directories, exclusive lock, database migration, and socket mode.
pub struct Daemon {
    config: DaemonConfig,
    listener: UnixListener,
    socket_owner: SocketOwner,
    _instance_lock: InstanceLock,
    state: Arc<ServerState>,
}

impl Daemon {
    /// Prepares storage and binds the configured Unix domain socket.
    ///
    /// # Errors
    ///
    /// Returns an error when paths cannot be secured, another daemon owns the
    /// lock, migrations fail, or the socket cannot be bound.
    pub fn bind(config: DaemonConfig) -> Result<Self, DaemonError> {
        ensure_private_directory(config.data_directory())?;
        ensure_private_directory(config.runtime_directory())?;
        let log_path = config.data_directory().join("logs");
        ensure_private_directory(&log_path)?;

        let instance_lock = InstanceLock::acquire(config.lock_path())?;
        remove_stale_socket(config.socket_path())?;

        let mut storage = Storage::open(config.database_path())?;
        storage.migrate()?;
        let schema_version = storage.schema_version()?;

        let listener = UnixListener::bind(config.socket_path())
            .map_err(|error| DaemonError::io("bind daemon socket", config.socket_path(), error))?;
        fs::set_permissions(config.socket_path(), fs::Permissions::from_mode(0o600)).map_err(
            |error| DaemonError::io("secure daemon socket", config.socket_path(), error),
        )?;
        let socket_owner = SocketOwner::new(config.socket_path())?;
        let instance_id = Uuid::now_v7();
        let diagnostics = DiagnosticsResponse {
            daemon_version: env!("CARGO_PKG_VERSION").to_owned(),
            protocol_version: PROTOCOL_V1,
            schema_version,
            daemon_instance_id: DaemonInstanceId::from_uuid(instance_id),
            data_path: config.data_directory().to_path_buf(),
            runtime_path: config.runtime_directory().to_path_buf(),
            log_path,
            effective_path: effective_executable_paths(),
            recent_issues: Vec::new(),
        };
        let session_storage = Storage::open(config.database_path())?;
        let state = Arc::new(ServerState {
            hello: HelloResponse {
                protocol_version: PROTOCOL_V1,
                daemon_version: env!("CARGO_PKG_VERSION").to_owned(),
                instance_id,
            },
            schema_version,
            diagnostics,
            projects: ProjectRegistry::new(storage),
            sessions: SessionRegistry::new(
                session_storage,
                DaemonInstanceId::from_uuid(instance_id),
            )
            .map_err(|error| {
                DaemonError::initialization(
                    "terminal sessions",
                    format!("{}: {}", error.code, error.message),
                )
            })?,
            event_sequence: AtomicU64::new(0),
        });

        info!(
            %instance_id,
            socket = %config.socket_path().display(),
            database = %config.database_path().display(),
            schema_version,
            "daemon bound"
        );

        Ok(Self {
            config,
            listener,
            socket_owner,
            _instance_lock: instance_lock,
            state,
        })
    }

    /// Returns the daemon lifetime identifier clients receive in `system.hello`.
    #[must_use]
    pub fn instance_id(&self) -> DaemonInstanceId {
        self.state.hello.instance_id
    }

    /// Returns the bound socket path.
    #[must_use]
    pub fn socket_path(&self) -> &Path {
        self.config.socket_path()
    }

    /// Accepts clients until cancellation, then closes all connections and
    /// removes the socket if it is still the inode created by this instance.
    ///
    /// # Errors
    ///
    /// Returns an error if accepting a new local client fails unexpectedly.
    pub async fn run(mut self, cancellation: CancellationToken) -> Result<(), DaemonError> {
        let mut clients = JoinSet::new();

        loop {
            tokio::select! {
                () = cancellation.cancelled() => {
                    info!(instance_id = %self.state.hello.instance_id, "daemon shutdown requested");
                    break;
                }
                accepted = self.listener.accept() => {
                    let (stream, _) = accepted.map_err(|error| {
                        DaemonError::io("accept daemon connection", self.config.socket_path(), error)
                    })?;
                    let state = Arc::clone(&self.state);
                    let client_cancellation = cancellation.child_token();
                    clients.spawn(async move {
                        serve_client(stream, state, client_cancellation).await;
                    });
                }
                completed = clients.join_next(), if !clients.is_empty() => {
                    if let Some(Err(error)) = completed {
                        warn!(%error, "daemon client task failed");
                    }
                }
            }
        }

        cancellation.cancel();
        while let Some(result) = clients.join_next().await {
            if let Err(error) = result {
                warn!(%error, "daemon client task failed during shutdown");
            }
        }
        self.state.sessions.shutdown();
        self.socket_owner.remove_if_owned();
        info!(instance_id = %self.state.hello.instance_id, "daemon stopped");
        Ok(())
    }
}

async fn serve_client(
    stream: UnixStream,
    state: Arc<ServerState>,
    cancellation: CancellationToken,
) {
    if let Err(error) = validate_peer(&stream) {
        warn!(%error, "rejecting daemon client with invalid peer credentials");
        return;
    }
    let mut framed = LengthDelimitedCodec::builder()
        .max_frame_length(MAX_FRAME_LENGTH)
        .new_framed(stream);

    loop {
        let frame = tokio::select! {
            () = cancellation.cancelled() => break,
            frame = framed.next() => frame,
        };

        let Some(frame) = frame else {
            break;
        };
        let bytes = match frame {
            Ok(bytes) => bytes,
            Err(error) => {
                warn!(%error, max_frame_length = MAX_FRAME_LENGTH, "closing client with invalid frame");
                break;
            }
        };

        let request = match decode_request(&bytes) {
            Ok(request) => request,
            Err(failure) => {
                let Some(request_id) = failure.request_id else {
                    warn!(error_code = %failure.error.code, "closing uncorrelatable invalid request");
                    break;
                };
                let response: ResponseEnvelope<Value> =
                    ResponseEnvelope::failure(request_id, failure.error);
                if !send_envelope(&mut framed, &response).await {
                    break;
                }
                continue;
            }
        };

        if request.kind == EnvelopeKind::Request
            && request.version == PROTOCOL_V1
            && request.method == method::SESSION_SUBSCRIBE
        {
            let request_id = request.request_id;
            let subscription = decode_payload::<SessionSubscribeRequest>(request.payload)
                .and_then(|payload| state.sessions.subscribe(payload));
            let response = match &subscription {
                Ok(_) => ResponseEnvelope::success(
                    request_id,
                    serde_json::to_value(EmptyResponse::default()).unwrap_or(Value::Null),
                ),
                Err(error) => ResponseEnvelope::failure(request_id, error.clone()),
            };
            if !send_envelope(&mut framed, &response).await {
                break;
            }
            if let Ok(subscription) = subscription {
                stream_session_events(&mut framed, &state, subscription, &cancellation).await;
                break;
            }
            continue;
        }

        let response = dispatch(request, &state);

        if !send_envelope(&mut framed, &response).await {
            break;
        }
    }
}

async fn send_envelope(
    framed: &mut tokio_util::codec::Framed<UnixStream, LengthDelimitedCodec>,
    envelope: &impl Serialize,
) -> bool {
    let encoded = match serde_json::to_vec(envelope) {
        Ok(encoded) => encoded,
        Err(error) => {
            warn!(%error, "could not encode daemon envelope");
            return false;
        }
    };
    if let Err(error) = framed.send(encoded.into()).await {
        debug!(%error, "client disconnected before daemon envelope completed");
        return false;
    }
    true
}

async fn stream_session_events(
    framed: &mut tokio_util::codec::Framed<UnixStream, LengthDelimitedCodec>,
    state: &ServerState,
    mut subscription: SessionSubscription,
    cancellation: &CancellationToken,
) {
    let snapshot = &subscription.snapshot;
    let latest_sequence = snapshot.next_sequence.saturating_sub(1);
    if snapshot.gap {
        let event = SessionOutputGapEvent {
            session_id: snapshot.session.id,
            requested_cursor: OutputCursor::new(0),
            first_available_sequence: OutputSequence::new(
                snapshot
                    .first_available_sequence
                    .unwrap_or(snapshot.next_sequence),
            ),
            latest_sequence: OutputSequence::new(latest_sequence),
        };
        if !send_event(framed, state, event_name::SESSION_OUTPUT_GAP, event).await {
            return;
        }
    }
    for output in &snapshot.output {
        let event = SessionOutputEvent {
            session_id: output.session_id,
            base64: PtyOutputBase64::try_new(encode_base64(&output.bytes))
                .expect("session output chunks satisfy the wire limit"),
            output_sequence: OutputSequence::new(output.sequence),
            replay: true,
        };
        if !send_event(framed, state, event_name::SESSION_OUTPUT, event).await {
            return;
        }
    }
    if !send_event(
        framed,
        state,
        event_name::SESSION_REPLAY_COMPLETE,
        SessionReplayCompleteEvent {
            session_id: snapshot.session.id,
            output_sequence: OutputSequence::new(latest_sequence),
        },
    )
    .await
    {
        return;
    }

    loop {
        let received = tokio::select! {
            () = cancellation.cancelled() => return,
            received = subscription.receiver.recv() => received,
        };
        let event = match received {
            Ok(event) => event,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                let latest = state.sessions.get(snapshot.session.id).ok();
                let latest_sequence = latest_sequence;
                let gap = SessionOutputGapEvent {
                    session_id: snapshot.session.id,
                    requested_cursor: OutputCursor::new(latest_sequence),
                    first_available_sequence: OutputSequence::new(
                        latest_sequence.saturating_add(1),
                    ),
                    latest_sequence: OutputSequence::new(latest_sequence),
                };
                if !send_event(framed, state, event_name::SESSION_OUTPUT_GAP, gap).await {
                    return;
                }
                drop(latest);
                continue;
            }
        };
        let sent = match event {
            SessionEvent::Output(output) => {
                send_event(
                    framed,
                    state,
                    event_name::SESSION_OUTPUT,
                    SessionOutputEvent {
                        session_id: output.session_id,
                        base64: PtyOutputBase64::try_new(encode_base64(&output.bytes))
                            .expect("session output chunks satisfy the wire limit"),
                        output_sequence: OutputSequence::new(output.sequence),
                        replay: false,
                    },
                )
                .await
            }
            SessionEvent::StatusChanged {
                session_id,
                previous,
                current,
                occurred_at_ms,
                reason,
            } => {
                let _ = state.sessions.persist_current(session_id);
                send_event(
                    framed,
                    state,
                    event_name::SESSION_STATUS_CHANGED,
                    SessionStatusChangedEvent {
                        session_id,
                        previous_status: previous,
                        status: current,
                        changed_at_ms: occurred_at_ms,
                        reason_code: Some(status_reason(reason).to_owned()),
                    },
                )
                .await
            }
            SessionEvent::Exited {
                session_id,
                status,
                exit_code,
                occurred_at_ms,
            } => {
                let _ = state.sessions.persist_current(session_id);
                send_event(
                    framed,
                    state,
                    event_name::SESSION_EXITED,
                    SessionExitedEvent {
                        session_id,
                        exit_code: Some(exit_code),
                        status,
                        exited_at_ms: occurred_at_ms,
                    },
                )
                .await
            }
            SessionEvent::IoFailure { session_id, .. } => {
                let _ = state.sessions.persist_current(session_id);
                true
            }
        };
        if !sent {
            return;
        }
    }
}

async fn send_event(
    framed: &mut tokio_util::codec::Framed<UnixStream, LengthDelimitedCodec>,
    state: &ServerState,
    event_name: &'static str,
    payload: impl Serialize,
) -> bool {
    let sequence = state.event_sequence.fetch_add(1, Ordering::Relaxed) + 1;
    send_envelope(framed, &EventEnvelope::v1(event_name, sequence, payload)).await
}

const fn status_reason(reason: StatusChangeReason) -> &'static str {
    match reason {
        StatusChangeReason::Activity => "activity",
        StatusChangeReason::IdleTimeout => "idle_timeout",
        StatusChangeReason::ProcessExited => "process_exited",
        StatusChangeReason::StopRequested => "stop_requested",
        StatusChangeReason::SupervisionLost => "supervision_lost",
    }
}

#[cfg(target_os = "linux")]
fn validate_peer(stream: &UnixStream) -> Result<(), io::Error> {
    let credentials = rustix::net::sockopt::socket_peercred(stream).map_err(io::Error::from)?;
    if credentials.uid != rustix::process::geteuid() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "peer effective user does not own the daemon",
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
#[allow(
    clippy::unnecessary_wraps,
    reason = "keeps the security check call site identical across supported targets"
)]
fn validate_peer(_stream: &UnixStream) -> Result<(), io::Error> {
    // rustix exposes safe SO_PEERCRED access on Linux, but no safe getpeereid
    // wrapper on macOS. There the 0700 runtime directory and 0600 socket are
    // the explicit access-control boundary while workspace unsafe code remains
    // forbidden.
    Ok(())
}

fn dispatch(request: RequestEnvelope<Value>, state: &ServerState) -> ResponseEnvelope<Value> {
    if request.kind != EnvelopeKind::Request {
        return ResponseEnvelope::failure(
            request.request_id,
            ApiError::new(
                "invalid_envelope_kind",
                "Daemon commands must use the request envelope kind",
            )
            .with_detail("receivedKind", format!("{:?}", request.kind).to_lowercase())
            .with_detail("expectedKind", "request"),
        );
    }
    if request.version != PROTOCOL_V1 {
        return ResponseEnvelope::failure(
            request.request_id,
            ApiError::new(
                "unsupported_protocol_version",
                "The requested IPC protocol version is not supported",
            )
            .with_action("Update Jig so the desktop and daemon versions match")
            .with_detail("receivedVersion", request.version)
            .with_detail("supportedVersion", PROTOCOL_V1),
        );
    }

    let result = match request.method.as_str() {
        method::SYSTEM_HELLO => encode_response(&state.hello),
        method::STATE_SNAPSHOT => state.projects.snapshot().and_then(|projects| {
            let agents = state.sessions.agents()?;
            let sessions = state.sessions.sessions()?;
            encode_response(StateSnapshot {
                schema_version: state.schema_version,
                projects,
                agents,
                sessions,
                worktrees: Vec::new(),
            })
        }),
        method::PROJECT_ADD => decode_payload(request.payload)
            .and_then(|payload: ProjectAddRequest| state.projects.add(payload))
            .and_then(encode_response),
        method::PROJECT_LIST => state.projects.list().and_then(encode_response),
        method::PROJECT_RENAME => decode_payload(request.payload)
            .and_then(|payload: ProjectRenameRequest| state.projects.rename(&payload))
            .and_then(encode_response),
        method::PROJECT_REMOVE => decode_payload(request.payload)
            .and_then(|payload: ProjectRemoveRequest| state.projects.remove(payload))
            .and_then(encode_response),
        method::AGENT_LIST => state.sessions.list_agents().and_then(encode_response),
        method::AGENT_DETECT => decode_payload(request.payload)
            .and_then(|payload: AgentDetectRequest| state.sessions.detect_agents(&payload))
            .and_then(encode_response),
        method::AGENT_CUSTOM_CREATE => decode_payload(request.payload)
            .and_then(|payload: AgentCustomCreateRequest| {
                state.sessions.create_custom_agent(payload)
            })
            .and_then(encode_response),
        method::SESSION_CREATE => decode_payload(request.payload)
            .and_then(|payload: SessionCreateRequest| state.sessions.create(payload))
            .and_then(encode_response),
        method::SESSION_LIST => decode_payload(request.payload)
            .and_then(|payload: SessionListRequest| state.sessions.list_sessions(payload))
            .and_then(encode_response),
        method::SESSION_RENAME => decode_payload(request.payload)
            .and_then(|payload: SessionRenameRequest| state.sessions.rename(payload))
            .and_then(encode_response),
        method::SESSION_START => decode_payload(request.payload)
            .and_then(|payload: SessionStartRequest| state.sessions.start(payload))
            .and_then(encode_response),
        method::SESSION_RESTART => decode_payload(request.payload)
            .and_then(|payload: SessionRestartRequest| state.sessions.restart(payload))
            .and_then(encode_response),
        method::SESSION_STOP => decode_payload(request.payload)
            .and_then(|payload: SessionStopRequest| state.sessions.stop(payload))
            .and_then(encode_response),
        method::SESSION_DELETE => decode_payload(request.payload)
            .and_then(|payload: SessionDeleteRequest| state.sessions.delete(payload))
            .and_then(encode_response),
        method::SESSION_WRITE => decode_payload(request.payload)
            .and_then(|payload: SessionWriteRequest| state.sessions.write(payload))
            .and_then(encode_response),
        method::SESSION_RESIZE => decode_payload(request.payload)
            .and_then(|payload: SessionResizeRequest| state.sessions.resize(payload))
            .and_then(encode_response),
        method::SESSION_UNSUBSCRIBE => encode_response(EmptyResponse::default()),
        method::DIAGNOSTICS_GET => encode_response(&state.diagnostics),
        _ => {
            return ResponseEnvelope::failure(
                request.request_id,
                ApiError::new("method_not_found", "The requested daemon method is unknown")
                    .with_detail("method", request.method),
            );
        }
    };

    match result {
        Ok(value) => ResponseEnvelope::success(request.request_id, value),
        Err(error) => ResponseEnvelope::failure(request.request_id, error),
    }
}

fn decode_payload<T>(payload: Value) -> Result<T, ApiError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(payload).map_err(|error| {
        ApiError::new("invalid_payload", "The request details are invalid.")
            .with_action("Review the submitted values and try again.")
            .with_detail("reason", error.to_string())
    })
}

fn encode_response(value: impl Serialize) -> Result<Value, ApiError> {
    serde_json::to_value(value).map_err(|error| {
        ApiError::new("internal_error", "The daemon could not encode its response")
            .with_detail("reason", error.to_string())
    })
}

fn effective_executable_paths() -> Vec<PathBuf> {
    let Some(value) = env::var_os("PATH") else {
        return Vec::new();
    };

    let mut paths = Vec::new();
    for path in env::split_paths(&value).filter(|path| path.is_absolute()) {
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    paths
}

struct RequestFailure {
    request_id: Option<RequestId>,
    error: ApiError,
}

fn decode_request(bytes: &[u8]) -> Result<RequestEnvelope<Value>, RequestFailure> {
    let value: Value = serde_json::from_slice(bytes).map_err(|error| RequestFailure {
        request_id: None,
        error: ApiError::new("invalid_json", "Request frame is not valid JSON")
            .with_detail("reason", error.to_string()),
    })?;
    let request_id = value
        .get("requestId")
        .and_then(Value::as_str)
        .and_then(|raw| raw.parse().ok());

    serde_json::from_value(value).map_err(|error| RequestFailure {
        request_id,
        error: ApiError::new(
            "invalid_request",
            "Request does not match the IPC envelope schema",
        )
        .with_detail("reason", error.to_string()),
    })
}

fn ensure_private_directory(path: &Path) -> Result<(), DaemonError> {
    fs::create_dir_all(path)
        .map_err(|error| DaemonError::io("create private directory", path, error))?;
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| DaemonError::io("inspect private directory", path, error))?;
    if !metadata.file_type().is_dir() {
        return Err(DaemonError::io(
            "secure private directory",
            path,
            io::Error::new(io::ErrorKind::InvalidInput, "path is not a directory"),
        ));
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| DaemonError::io("secure private directory", path, error))
}

fn remove_stale_socket(path: &Path) -> Result<(), DaemonError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(DaemonError::io("inspect daemon socket", path, error)),
    };
    if !metadata.file_type().is_socket() {
        return Err(DaemonError::SocketPathOccupied {
            path: path.to_path_buf(),
        });
    }
    fs::remove_file(path)
        .map_err(|error| DaemonError::io("remove stale daemon socket", path, error))
}

struct SocketOwner {
    path: PathBuf,
    device: u64,
    inode: u64,
    removed: bool,
}

impl SocketOwner {
    fn new(path: &Path) -> Result<Self, DaemonError> {
        let metadata = fs::symlink_metadata(path)
            .map_err(|error| DaemonError::io("inspect bound daemon socket", path, error))?;
        Ok(Self {
            path: path.to_path_buf(),
            device: metadata.dev(),
            inode: metadata.ino(),
            removed: false,
        })
    }

    fn remove_if_owned(&mut self) {
        if self.removed {
            return;
        }
        self.removed = true;
        let Ok(metadata) = fs::symlink_metadata(&self.path) else {
            return;
        };
        if metadata.file_type().is_socket()
            && metadata.dev() == self.device
            && metadata.ino() == self.inode
        {
            if let Err(error) = fs::remove_file(&self.path) {
                warn!(%error, socket = %self.path.display(), "could not remove owned daemon socket");
            }
        }
    }
}

impl Drop for SocketOwner {
    fn drop(&mut self) {
        self.remove_if_owned();
    }
}

#[cfg(test)]
mod tests {
    use cli_master_core::ResponsePayload;
    use serde_json::json;
    use tempfile::TempDir;

    use super::*;

    #[tokio::test]
    async fn dispatch_rejects_non_request_kind() {
        let temporary = TempDir::new().expect("temporary directory should exist");
        let daemon = Daemon::bind(DaemonConfig::from_paths(
            temporary.path().join("data"),
            temporary.path().join("run"),
        ))
        .expect("daemon should bind");
        let request_id = RequestId::new();
        let response = dispatch(
            RequestEnvelope {
                kind: EnvelopeKind::Event,
                version: PROTOCOL_V1,
                request_id,
                method: "system.hello".to_owned(),
                payload: json!({}),
            },
            &daemon.state,
        );

        match response.payload {
            ResponsePayload::Error { error } => assert_eq!(error.code, "invalid_envelope_kind"),
            ResponsePayload::Success { .. } => panic!("wrong kind must fail"),
        }
    }
}
