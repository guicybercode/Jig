use std::fs;
use std::io;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use cli_master_core::wire::HelloResponse;
use cli_master_core::{DaemonInstanceId, PROTOCOL_V1};
use cli_master_storage::{RecoveryContext, Storage};
use tokio::net::UnixListener;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::client::serve_client;
use crate::lock::InstanceLock;
use crate::{DaemonConfig, DaemonError, EventBus};

/// Largest accepted JSON frame, excluding the four-byte length prefix.
pub const MAX_FRAME_LENGTH: usize = 1024 * 1024;

#[derive(Debug)]
pub(crate) struct ServerState {
    pub(crate) hello: HelloResponse,
    pub(crate) schema_version: u32,
    pub(crate) config: DaemonConfig,
    pub(crate) events: EventBus,
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
        config.prepare_private_directories()?;

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
            config: config.clone(),
            events: EventBus::new(crate::DiagnosticLog::default()),
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

    /// Session event bus used by `SessionManager` and tests.
    #[must_use]
    pub fn events(&self) -> &EventBus {
        &self.state.events
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
