use cli_master_core::{APPLICATION_VERSION, ApiError, PROTOCOL_V1, ResponseEnvelope, wire};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, State};

use super::client::{BridgeOptions, DaemonBridge};
use super::relay::TauriRelay;
use super::status::DaemonStatus;
use super::{DAEMON_SIDECAR_EXTERNAL_BIN, DAEMON_SIDECAR_NAME};

/// Desktop-side protocol catalog. This is not `system.hello`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolInfo {
    pub(crate) application_version: &'static str,
    pub(crate) protocol_version: u16,
    pub(crate) daemon_sidecar: &'static str,
    pub(crate) sidecar_external_bin: &'static str,
    pub(crate) methods: Vec<&'static str>,
    pub(crate) events: Vec<&'static str>,
}

/// Frozen method and event names for the webview. Live negotiation uses
/// `daemon_invoke("system.hello", {})`.
#[tauri::command]
pub fn protocol_info() -> ProtocolInfo {
    ProtocolInfo {
        application_version: APPLICATION_VERSION,
        protocol_version: PROTOCOL_V1,
        daemon_sidecar: DAEMON_SIDECAR_NAME,
        sidecar_external_bin: DAEMON_SIDECAR_EXTERNAL_BIN,
        methods: wire::method::ALL.to_vec(),
        events: wire::event_name::ALL.to_vec(),
    }
}

/// Forwards one versioned wire request to `cli-masterd`.
///
/// Transport failures become [`ApiError`]. Daemon application errors remain
/// inside the returned envelope.
#[tauri::command]
#[allow(
    clippy::needless_pass_by_value,
    reason = "Tauri injects State by value"
)]
pub async fn daemon_invoke(
    bridge: State<'_, DaemonBridge>,
    method: String,
    payload: Value,
) -> Result<ResponseEnvelope<Value>, ApiError> {
    bridge.invoke(method, payload).await.map_err(Into::into)
}

/// Latest daemon connection status.
#[tauri::command]
#[allow(
    clippy::needless_pass_by_value,
    reason = "Tauri injects State by value"
)]
pub fn daemon_status(bridge: State<'_, DaemonBridge>) -> DaemonStatus {
    bridge.status()
}

/// Asks the bridge to connect again after backoff exhaustion or incompatibility.
#[tauri::command]
#[allow(
    clippy::needless_pass_by_value,
    reason = "Tauri injects State by value"
)]
pub fn daemon_reconnect(bridge: State<'_, DaemonBridge>) {
    bridge.request_reconnect();
}

/// Exits the desktop process without stopping daemon-owned sessions.
#[tauri::command]
#[allow(
    clippy::needless_pass_by_value,
    reason = "Tauri injects State and AppHandle by value"
)]
pub fn app_quit(app: AppHandle, bridge: State<'_, DaemonBridge>) {
    bridge.shutdown();
    app.exit(0);
}

/// Starts the reconnecting bridge used by generic Tauri commands.
#[must_use]
pub fn start_bridge(app: &AppHandle) -> DaemonBridge {
    let options = BridgeOptions::discover().unwrap_or_else(|error| {
        tracing::warn!(
            error_code = error.code(),
            "could not discover daemon socket paths"
        );
        fallback_options()
    });
    let relay = std::sync::Arc::new(TauriRelay::new(app.clone()));
    DaemonBridge::spawn(options, relay)
}

fn fallback_options() -> BridgeOptions {
    use std::path::PathBuf;
    use std::time::Duration;

    use super::backoff::BoundedBackoff;
    use super::client::Timeouts;
    use super::sidecar::SidecarMode;

    BridgeOptions {
        socket_path: PathBuf::from("/tmp/cli-master-missing/daemon.sock"),
        timeouts: Timeouts::production(),
        backoff: BoundedBackoff::with_limits(Duration::from_millis(100), Duration::from_secs(2), 8),
        sidecar: SidecarMode::SpawnIfMissing,
    }
}
