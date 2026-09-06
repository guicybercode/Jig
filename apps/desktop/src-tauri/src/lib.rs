mod browser;
mod daemon_bridge;

use browser::{
    BrowserSurfaceHost, browser_surface_close, browser_surface_focus, browser_surface_go_back,
    browser_surface_go_forward, browser_surface_navigate, browser_surface_open,
    browser_surface_reload, browser_surface_update,
};
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
        .manage(BrowserSurfaceHost::default())
        .manage(DaemonBridge::default())
        .invoke_handler(tauri::generate_handler![
            daemon_request,
            daemon_terminal_subscribe,
            daemon_terminal_unsubscribe,
            browser_surface_open,
            browser_surface_navigate,
            browser_surface_update,
            browser_surface_reload,
            browser_surface_go_back,
            browser_surface_go_forward,
            browser_surface_focus,
            browser_surface_close
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use cli_master_core::{APPLICATION_VERSION, PROTOCOL_V1, wire};
    use serde_json::json;

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

        assert_eq!(
            tauri_config["bundle"]["externalBin"],
            json!(["binaries/cli-masterd"])
        );
        assert_eq!(
            tauri_config["bundle"]["targets"],
            json!(["app", "dmg", "appimage"])
        );
        assert_eq!(
            tauri_config["bundle"]["macOS"]["signingIdentity"],
            json!(null)
        );
        assert_eq!(
            tauri_config["bundle"]["macOS"]["hardenedRuntime"],
            json!(true)
        );

        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/default.json"))
                .expect("desktop capability should parse");
        assert_eq!(capability["webviews"], json!(["main"]));
        assert!(capability.get("windows").is_none());
        assert!(capability.get("remote").is_none());

        let permissions = capability["permissions"]
            .as_array()
            .expect("desktop permissions should be an array");
        for permission in [
            "allow-daemon-request",
            "allow-daemon-terminal-subscribe",
            "allow-daemon-terminal-unsubscribe",
            "allow-browser-surface-open",
            "allow-browser-surface-navigate",
            "allow-browser-surface-update",
            "allow-browser-surface-reload",
            "allow-browser-surface-go-back",
            "allow-browser-surface-go-forward",
            "allow-browser-surface-focus",
            "allow-browser-surface-close",
        ] {
            assert!(permissions.contains(&json!(permission)));
        }

        let csp = tauri_config["app"]["security"]["csp"]
            .as_str()
            .expect("desktop CSP should be configured");
        assert!(csp.contains("object-src 'none'"));
        assert!(csp.contains("frame-src 'none'"));
        assert!(csp.contains("frame-ancestors 'none'"));
        assert!(!csp.contains("ws://"));
        assert!(
            tauri_config["app"]["security"]["devCsp"]
                .as_str()
                .expect("desktop development CSP should be configured")
                .contains("ws://localhost:1420")
        );

        let cargo_manifest = include_str!("../Cargo.toml");
        assert!(
            cargo_manifest.contains("tauri = { version = \"=2.11.5\", features = [\"unstable\"] }")
        );
    }
}
