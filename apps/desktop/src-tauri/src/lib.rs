//! Tauri desktop bridge. Process ownership stays with `cli-masterd`.

#![allow(clippy::needless_pass_by_value)]

mod bridge;

use std::sync::Mutex;

use cli_master_core::ApiError;
use serde_json::Value;
use tauri::Manager;

use crate::bridge::Bridge;

struct AppState {
    bridge: Mutex<Option<Bridge>>,
}

fn connected_bridge(state: &AppState) -> Result<Bridge, ApiError> {
    let mut slot = state
        .bridge
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(existing) = slot.as_ref() {
        return Ok(existing.clone());
    }
    let created = Bridge::connect()?;
    *slot = Some(created.clone());
    Ok(created)
}

#[tauri::command]
fn daemon_request(
    state: tauri::State<'_, AppState>,
    method: String,
    payload: Value,
) -> Result<Value, ApiError> {
    connected_bridge(&state)?.request(&method, payload)
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
        .manage(AppState {
            bridge: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![daemon_request])
        .setup(|app| {
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let state = handle.state::<AppState>();
                let _ = connected_bridge(&state);
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
