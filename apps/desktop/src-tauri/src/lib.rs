mod daemon_bridge;

use daemon_bridge::{
    DaemonBridge, daemon_request, daemon_terminal_subscribe, daemon_terminal_unsubscribe,
};

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
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(DaemonBridge::default())
        .invoke_handler(tauri::generate_handler![
            daemon_request,
            daemon_terminal_subscribe,
            daemon_terminal_unsubscribe
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
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
