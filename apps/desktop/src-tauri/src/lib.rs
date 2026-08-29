use cli_master_core::{PROTOCOL_V1, wire};
use serde::Serialize;

/// Desktop-side protocol catalog. This is not `system.hello`.
///
/// The Tauri process is a typed bridge. Live sessions belong to `cli-masterd`.
/// Until the Tauri bridge connects to the daemon, the UI can still read the
/// frozen method list.
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
        methods: wire::method::ALL.to_vec(),
        events: wire::event_name::ALL.to_vec(),
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

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
