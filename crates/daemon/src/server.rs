use std::fs;
use std::io;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use cli_master_core::wire::{self, EmptyRequest, HelloResponse, StateSnapshotResponse};
use cli_master_core::{
    ApiError, DaemonInstanceId, EnvelopeKind, PROTOCOL_V1, RequestEnvelope, RequestId,
    ResponseEnvelope,
};
use cli_master_storage::{RecoveryContext, Storage};
use futures_util::{SinkExt, StreamExt};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::net::{UnixListener, UnixStream};
use tokio::task::JoinSet;
use tokio_util::codec::LengthDelimitedCodec;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::lock::InstanceLock;
use crate::{DaemonConfig, DaemonError};

/// Largest accepted JSON frame, excluding the four-byte length prefix.
pub const MAX_FRAME_LENGTH: usize = 1024 * 1024;

#[derive(Debug)]
struct ServerState {
    hello: HelloResponse,
    schema_version: u32,
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
    storage: Storage,
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

        let instance_lock = InstanceLock::acquire(config.lock_path())?;
        let instance_id = DaemonInstanceId::new();
        let instance_id_text = instance_id.to_string();
        let storage = Storage::open_migrated(config.database_path())?;
        let reconciliation = storage.reconcile_sessions(&RecoveryContext {
            current_daemon_instance_id: &instance_id_text,
            live_session_ids: &[],
            updated_at_ms: unix_epoch_ms()?,
        })?;
        let recovered_sessions = reconciliation
            .iter()
            .filter(|event| event.previous_status != event.new_status)
            .count();
        let schema_version = storage.schema_version()?;

        remove_stale_socket(config.socket_path())?;
        let listener = UnixListener::bind(config.socket_path())
            .map_err(|error| DaemonError::io("bind daemon socket", config.socket_path(), error))?;
        fs::set_permissions(config.socket_path(), fs::Permissions::from_mode(0o600)).map_err(
            |error| DaemonError::io("secure daemon socket", config.socket_path(), error),
        )?;
        let socket_owner = SocketOwner::new(config.socket_path())?;
        let state = Arc::new(ServerState {
            hello: HelloResponse {
                protocol_version: PROTOCOL_V1,
                daemon_version: env!("CARGO_PKG_VERSION").to_owned(),
                instance_id,
            },
            schema_version,
        });

        info!(
            %instance_id,
            socket = %config.socket_path().display(),
            database = %config.database_path().display(),
            schema_version,
            recovered_sessions,
            "daemon bound"
        );

        Ok(Self {
            config,
            listener,
            socket_owner,
            _instance_lock: instance_lock,
            storage,
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
        self.socket_owner.remove_if_owned();
        self.storage.close()?;
        info!(instance_id = %self.state.hello.instance_id, "daemon stopped");
        Ok(())
    }
}

fn unix_epoch_ms() -> Result<i64, DaemonError> {
    let elapsed = SystemTime::now().duration_since(UNIX_EPOCH)?;
    i64::try_from(elapsed.as_millis()).map_err(|_| DaemonError::TimestampOverflow)
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

        let response = match decode_request(&bytes) {
            Ok(request) => dispatch(request, &state),
            Err(failure) => {
                let Some(request_id) = failure.request_id else {
                    warn!(error_code = %failure.error.code, "closing uncorrelatable invalid request");
                    break;
                };
                ResponseEnvelope::failure(request_id, failure.error)
            }
        };

        let encoded = match serde_json::to_vec(&response) {
            Ok(encoded) => encoded,
            Err(error) => {
                warn!(%error, "could not encode daemon response");
                break;
            }
        };
        if let Err(error) = framed.send(encoded.into()).await {
            debug!(%error, "client disconnected before response completed");
            break;
        }
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
            .with_action("Update CLI Master so the desktop and daemon versions match")
            .with_detail("receivedVersion", request.version)
            .with_detail("supportedVersion", PROTOCOL_V1),
        );
    }

    let result = match request.method.as_str() {
        wire::method::SYSTEM_HELLO => {
            if let Err(error) = decode_payload::<EmptyRequest>(request.payload) {
                return invalid_payload(request.request_id, &error);
            }
            serde_json::to_value(&state.hello)
        }
        wire::method::STATE_SNAPSHOT => {
            if let Err(error) = decode_payload::<EmptyRequest>(request.payload) {
                return invalid_payload(request.request_id, &error);
            }
            serde_json::to_value(StateSnapshotResponse {
                schema_version: state.schema_version,
                projects: Vec::new(),
                agents: Vec::new(),
                sessions: Vec::new(),
                worktrees: Vec::new(),
            })
        }
        method if wire::method::is_supported(method) => {
            return ResponseEnvelope::failure(
                request.request_id,
                ApiError::new(
                    "method_not_implemented",
                    "The requested daemon method is part of the Beta contract but is not implemented",
                )
                .with_detail("method", method),
            );
        }
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
        Err(error) => ResponseEnvelope::failure(
            request.request_id,
            ApiError::new("internal_error", "The daemon could not encode its response")
                .with_detail("reason", error.to_string()),
        ),
    }
}

fn decode_payload<T: DeserializeOwned>(payload: Value) -> Result<T, serde_json::Error> {
    serde_json::from_value(payload)
}

fn invalid_payload(request_id: RequestId, error: &serde_json::Error) -> ResponseEnvelope<Value> {
    ResponseEnvelope::failure(
        request_id,
        ApiError::new(
            "invalid_payload",
            "Request payload does not match the method contract",
        )
        .with_detail("reason", error.to_string()),
    )
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

    use super::*;

    #[test]
    fn dispatch_rejects_non_request_kind() {
        let request_id = RequestId::new();
        let response = dispatch(
            RequestEnvelope {
                kind: EnvelopeKind::Event,
                version: PROTOCOL_V1,
                request_id,
                method: "system.hello".to_owned(),
                payload: json!({}),
            },
            &ServerState {
                hello: HelloResponse {
                    protocol_version: PROTOCOL_V1,
                    daemon_version: "test".to_owned(),
                    instance_id: DaemonInstanceId::new(),
                },
                schema_version: 1,
            },
        );

        match response.payload {
            ResponsePayload::Error { error } => assert_eq!(error.code, "invalid_envelope_kind"),
            ResponsePayload::Success { .. } => panic!("wrong kind must fail"),
        }
    }
}
