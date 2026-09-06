use cli_master_core::ApiError;
use rustix::io::Errno;

pub(super) fn io_error(error: impl Into<std::io::Error>) -> ApiError {
    let error = error.into();
    match error.raw_os_error().map(Errno::from_raw_os_error) {
        Some(Errno::NOENT) => api("file_not_found", "The selected file no longer exists."),
        Some(Errno::NOTDIR) => api(
            "file_not_directory",
            "A selected path component is not a directory.",
        ),
        Some(Errno::LOOP) => api(
            "file_symlink_not_allowed",
            "Symbolic links cannot be followed by the editor.",
        ),
        Some(Errno::ACCESS | Errno::PERM) => api(
            "file_permission_denied",
            "The file operation is not permitted.",
        ),
        _ => api("file_io_error", "The file operation could not complete."),
    }
}

pub(super) fn api(code: &str, message: &str) -> ApiError {
    ApiError::new(code, message).with_action(
        "Keep the editor draft, refresh the selected file, and retry after resolving the error.",
    )
}

pub(super) fn target_changed() -> ApiError {
    api(
        "file_target_changed",
        "The registered directory changed or is no longer available for this operation.",
    )
}

pub(super) fn metadata_unsupported() -> ApiError {
    api(
        "file_metadata_unsupported",
        "This file has metadata that cannot be safely preserved by the editor.",
    )
}

pub(super) fn invalid_input() -> ApiError {
    api(
        "invalid_input",
        "The file request contains an invalid target, path, text, revision, or limit.",
    )
}

pub(super) fn conflict(revision: Option<&str>) -> ApiError {
    let error = api(
        "file_conflict",
        "The file changed after it was read. Reload before saving.",
    );
    match revision {
        Some(revision) => error.with_detail("currentRevision", revision),
        None => error,
    }
}
