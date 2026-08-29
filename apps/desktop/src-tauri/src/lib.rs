use cli_master_core::{IpcEvent, IpcMethod, PROTOCOL_V1};
use serde::Serialize;

/// Desktop-side protocol catalog. This is not `system.hello`.
///
/// The Tauri process is a typed bridge. Live sessions belong to `cli-masterd`.
/// Until the daemon crate lands, the UI can still read the frozen method list.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProtocolInfo {
    protocol_version: u16,
    methods: Vec<&'static str>,
    events: Vec<&'static str>,
}

#[tauri::command]
fn protocol_info() -> ProtocolInfo {
    ProtocolInfo {
        protocol_version: PROTOCOL_V1,
        methods: IpcMethod::ALL.iter().map(IpcMethod::as_str).collect(),
        events: IpcEvent::ALL.iter().map(IpcEvent::as_str).collect(),
    }
}

/// Starts the Tauri desktop process.
///
/// # Panics
///
/// Panics when Tauri cannot initialize or run the desktop application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![protocol_info])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
