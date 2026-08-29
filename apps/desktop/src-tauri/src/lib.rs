use cli_master_core::DiagnosticsReport;
use cli_master_safety::{Logger, StructuredLog, collect_diagnostics};

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {name}! You've been greeted from Rust!")
}

#[tauri::command]
fn diagnostics_get() -> DiagnosticsReport {
    collect_diagnostics()
}

#[tauri::command]
fn diagnostics_export() -> String {
    let report = collect_diagnostics();
    Logger::global().write(&StructuredLog::new(
        cli_master_safety::LogLevel::Info,
        "diagnostics",
        "diagnostics.export",
        "user exported a sanitized diagnostics snapshot",
    ));
    report.to_export_text()
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
        .invoke_handler(tauri::generate_handler![
            greet,
            diagnostics_get,
            diagnostics_export
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
