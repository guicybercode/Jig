mod sidecar;

use cli_master_core::{APPLICATION_VERSION, PROTOCOL_V1, wire};
use serde::Serialize;
use sidecar::{DAEMON_SIDECAR_EXTERNAL_BIN, DAEMON_SIDECAR_NAME, resolve_bundled_daemon};

/// Desktop-side protocol catalog. This is not `system.hello`.
///
/// The Tauri process is a typed bridge. Live sessions belong to `cli-masterd`.
/// Until the Tauri bridge connects to the daemon, the UI can still read the
/// frozen method list, the application version, and the sidecar file name.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProtocolInfo {
    application_version: &'static str,
    protocol_version: u16,
    daemon_sidecar: &'static str,
    sidecar_external_bin: &'static str,
    methods: Vec<&'static str>,
    events: Vec<&'static str>,
}

#[tauri::command]
fn protocol_info() -> ProtocolInfo {
    ProtocolInfo {
        application_version: APPLICATION_VERSION,
        protocol_version: PROTOCOL_V1,
        daemon_sidecar: DAEMON_SIDECAR_NAME,
        sidecar_external_bin: DAEMON_SIDECAR_EXTERNAL_BIN,
        methods: wire::method::ALL.to_vec(),
        events: wire::event_name::ALL.to_vec(),
    }
}

/// Resolves the packaged `cli-masterd` next to the desktop executable.
///
/// This is a filesystem lookup. It does not replace `system.hello`.
#[tauri::command]
fn bundled_daemon_path() -> Option<String> {
    let executable = std::env::current_exe().ok()?;
    resolve_bundled_daemon(&executable).map(|path| path.display().to_string())
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
        .invoke_handler(tauri::generate_handler![protocol_info, bundled_daemon_path])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::sidecar;
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
        assert_eq!(exposed.daemon_sidecar, sidecar::DAEMON_SIDECAR_NAME);
        assert_eq!(
            exposed.sidecar_external_bin,
            sidecar::DAEMON_SIDECAR_EXTERNAL_BIN
        );
        assert_eq!(exposed.methods, wire::method::ALL);
        assert_eq!(exposed.events, wire::event_name::ALL);

        assert_eq!(
            tauri_config["bundle"]["externalBin"],
            json!([sidecar::DAEMON_SIDECAR_EXTERNAL_BIN])
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
