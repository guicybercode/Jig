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
    use cli_master_core::{APPLICATION_VERSION, PROTOCOL_V1, wire};
    use serde_json::json;

    use super::*;

    #[test]
    fn rust_catalog_matches_desktop_mirror() {
        let catalog: serde_json::Value =
            serde_json::from_str(include_str!("../../../../protocol/catalog.json"))
                .expect("desktop protocol mirror should parse");

        assert_eq!(catalog["protocolVersion"], PROTOCOL_V1);
        assert_eq!(catalog["applicationVersion"], APPLICATION_VERSION);
        assert_eq!(catalog["methods"], json!(wire::method::ALL));
        assert_eq!(catalog["events"], json!(wire::event_name::ALL));

        let root_package: serde_json::Value =
            serde_json::from_str(include_str!("../../../../package.json"))
                .expect("root package.json should parse");
        let desktop_package: serde_json::Value =
            serde_json::from_str(include_str!("../../package.json"))
                .expect("desktop package.json should parse");
        let tauri_config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json"))
                .expect("tauri.conf.json should parse");

        assert_eq!(root_package["version"], APPLICATION_VERSION);
        assert_eq!(desktop_package["version"], APPLICATION_VERSION);
        assert_eq!(tauri_config["version"], APPLICATION_VERSION);

        let exposed = protocol_info();
        assert_eq!(exposed.application_version, APPLICATION_VERSION);
        assert_eq!(exposed.daemon_sidecar, bridge::DAEMON_SIDECAR_NAME);
        assert_eq!(
            exposed.sidecar_external_bin,
            bridge::DAEMON_SIDECAR_EXTERNAL_BIN
        );
        assert_eq!(exposed.methods, wire::method::ALL);
        assert_eq!(exposed.events, wire::event_name::ALL);

        assert_eq!(
            tauri_config["bundle"]["externalBin"],
            json!([bridge::DAEMON_SIDECAR_EXTERNAL_BIN])
        );
        assert_eq!(
            tauri_config["bundle"]["targets"],
            json!(["app", "dmg", "appimage"])
        );
        assert_eq!(
            tauri_config["bundle"]["macOS"]["signingIdentity"],
            json!(null)
        );
    }
}
