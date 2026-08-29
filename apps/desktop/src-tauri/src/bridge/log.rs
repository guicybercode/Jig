use cli_master_core::RequestId;

use super::status::DaemonStatus;

/// Logs a forwarded request without recording the payload.
pub fn request_sent(method: &str, request_id: RequestId, frame_bytes: usize) {
    tracing::info!(
        method,
        request_id = %request_id,
        frame_bytes,
        "forwarded daemon request"
    );
}

/// Logs a completed round trip using only the error code when present.
pub fn request_finished(method: &str, request_id: RequestId, error_code: Option<&str>) {
    tracing::info!(
        method,
        request_id = %request_id,
        error_code,
        "daemon request finished"
    );
}

/// Logs a status transition. Status details are already non-secret.
pub fn status_changed(status: &DaemonStatus) {
    match status {
        DaemonStatus::Connecting => tracing::info!(state = "connecting", "daemon bridge status"),
        DaemonStatus::Ready {
            protocol_version,
            daemon_version,
            instance_id,
        } => tracing::info!(
            state = "ready",
            protocol_version,
            daemon_version,
            instance_id = %instance_id,
            "daemon bridge status"
        ),
        DaemonStatus::Reconnecting { attempt } => {
            tracing::info!(state = "reconnecting", attempt, "daemon bridge status");
        }
        DaemonStatus::Unavailable { detail } => {
            tracing::warn!(state = "unavailable", detail, "daemon bridge status");
        }
        DaemonStatus::Incompatible {
            protocol_version,
            daemon_version,
            detail,
        } => tracing::warn!(
            state = "incompatible",
            protocol_version,
            daemon_version,
            detail,
            "daemon bridge status"
        ),
    }
}

/// Logs sidecar spawn using the discovery source, not environment contents.
pub fn sidecar_spawned(source: &'static str, pid: u32) {
    tracing::info!(binary_source = source, pid, "spawned cli-masterd sidecar");
}
