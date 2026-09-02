use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const APP_COMMANDS: &[&str] = &[
    "daemon_request",
    "daemon_terminal_subscribe",
    "daemon_terminal_unsubscribe",
    "browser_surface_open",
    "browser_surface_navigate",
    "browser_surface_update",
    "browser_surface_reload",
    "browser_surface_go_back",
    "browser_surface_go_forward",
    "browser_surface_focus",
    "browser_surface_close",
];

fn main() {
    ensure_sidecar();
    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(APP_COMMANDS));
    tauri_build::try_build(attributes).expect("failed to build Tauri application metadata");
}

/// Tauri refuses to compile when `bundle.externalBin` is missing. Debug
/// clippy/test can use a stub; release packaging must stage a real `cli-masterd`.
fn ensure_sidecar() {
    let Ok(target) = env::var("TARGET") else {
        return;
    };
    let dest = PathBuf::from("binaries").join(format!("cli-masterd-{target}"));
    println!("cargo:rerun-if-changed={}", dest.display());
    if dest.is_file() {
        return;
    }

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let workspace_root = Path::new(&manifest_dir).join("../../..");
    for profile in ["debug", "release"] {
        let src = workspace_root
            .join("target")
            .join(profile)
            .join("cli-masterd");
        if src.is_file() {
            let _ = fs::create_dir_all("binaries");
            if fs::copy(&src, &dest).is_ok() {
                return;
            }
        }
    }

    if env::var("PROFILE").ok().as_deref() != Some("release") {
        let _ = fs::create_dir_all("binaries");
        let _ = fs::write(&dest, []);
    }
}
