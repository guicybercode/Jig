mod daemon_bridge;

use daemon_bridge::{DaemonBridge, daemon_request};

/// Starts the Tauri desktop process.
///
/// # Panics
///
/// Panics when Tauri cannot initialize or run the desktop application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(DaemonBridge::default())
        .invoke_handler(tauri::generate_handler![daemon_request])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
