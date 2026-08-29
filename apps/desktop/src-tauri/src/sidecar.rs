//! Locate the bundled `cli-masterd` sidecar next to the desktop executable.
//!
//! Tauri copies `bundle.externalBin` beside the packaged app binary on Linux
//! AppImage and inside `Contents/MacOS` on macOS. This module does not start
//! the daemon or speak the IPC protocol.

use std::path::{Path, PathBuf};

/// File name of the session daemon sidecar, without a target triple.
pub const DAEMON_SIDECAR_NAME: &str = "cli-masterd";

/// Relative path used in `tauri.conf.json` `bundle.externalBin`.
pub const DAEMON_SIDECAR_EXTERNAL_BIN: &str = "binaries/cli-masterd";

/// Name of the staged sidecar artifact for a Rust target triple.
#[must_use]
pub fn sidecar_artifact_name(target_triple: &str) -> String {
    format!("{DAEMON_SIDECAR_NAME}-{target_triple}")
}

/// Candidate paths for a sidecar sitting next to the desktop executable.
#[must_use]
pub fn daemon_candidates(current_exe: &Path) -> Vec<PathBuf> {
    let Some(directory) = current_exe.parent() else {
        return Vec::new();
    };
    vec![directory.join(DAEMON_SIDECAR_NAME)]
}

/// Returns the first candidate that exists as a regular file.
#[must_use]
pub fn resolve_bundled_daemon(current_exe: &Path) -> Option<PathBuf> {
    daemon_candidates(current_exe)
        .into_iter()
        .find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::{
        DAEMON_SIDECAR_EXTERNAL_BIN, DAEMON_SIDECAR_NAME, daemon_candidates,
        resolve_bundled_daemon, sidecar_artifact_name,
    };

    #[test]
    fn sidecar_names_are_stable() {
        assert_eq!(DAEMON_SIDECAR_NAME, "cli-masterd");
        assert_eq!(DAEMON_SIDECAR_EXTERNAL_BIN, "binaries/cli-masterd");
        assert_eq!(
            sidecar_artifact_name("x86_64-unknown-linux-gnu"),
            "cli-masterd-x86_64-unknown-linux-gnu"
        );
        assert_eq!(
            sidecar_artifact_name("aarch64-apple-darwin"),
            "cli-masterd-aarch64-apple-darwin"
        );
    }

    #[test]
    fn sibling_file_is_resolved_and_missing_file_is_not() {
        let root = std::env::temp_dir().join(format!(
            "cli-master-sidecar-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("temp sidecar directory");
        let exe = root.join("CLI Master");
        let daemon = root.join(DAEMON_SIDECAR_NAME);
        std::fs::write(&exe, []).expect("desktop placeholder");
        assert_eq!(daemon_candidates(&exe), vec![daemon.clone()]);
        assert_eq!(resolve_bundled_daemon(&exe), None);
        std::fs::write(&daemon, []).expect("daemon placeholder");
        assert_eq!(
            resolve_bundled_daemon(&exe).as_deref(),
            Some(daemon.as_path())
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
