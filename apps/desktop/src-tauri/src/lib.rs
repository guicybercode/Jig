//! Tauri desktop process for CLI Master.
//!
//! The window process is a typed bridge. It forwards versioned wire envelopes
//! to `cli-masterd` and relays daemon events. Live sessions belong to the
//! daemon and survive closing the window.

mod bridge;

use bridge::DaemonBridge;
use bridge::commands::{app_quit, daemon_invoke, daemon_reconnect, daemon_status, protocol_info};
use tauri::{Manager, RunEvent};

/// Starts the Tauri desktop process.
///
/// # Panics
///
/// Panics when Tauri cannot initialize or run the desktop application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(true)
        .try_init()
        .ok();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            protocol_info,
            daemon_invoke,
            daemon_status,
            daemon_reconnect,
            app_quit
        ])
        .setup(|app| {
            let bridge = bridge::commands::start_bridge(app.handle());
            app.manage(bridge);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app_handle, event| {
            if let RunEvent::ExitRequested { .. } = event {
                if let Some(bridge) = app_handle.try_state::<DaemonBridge>() {
                    bridge.shutdown();
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use cli_master_core::{PROTOCOL_V1, wire};

    #[test]
    fn rust_catalog_matches_desktop_mirror() {
        let catalog: serde_json::Value =
            serde_json::from_str(include_str!("../../../../protocol/catalog.json"))
                .expect("desktop protocol mirror should parse");

        assert_eq!(catalog["protocolVersion"], PROTOCOL_V1);
        assert_eq!(catalog["methods"], json!(wire::method::ALL));
        assert_eq!(catalog["events"], json!(wire::event_name::ALL));

        let exposed = protocol_info();
        assert_eq!(exposed.methods, wire::method::ALL);
        assert_eq!(exposed.events, wire::event_name::ALL);
    }
}
