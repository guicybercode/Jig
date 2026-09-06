use std::ffi::OsStr;
use std::os::fd::OwnedFd;
use std::os::unix::ffi::OsStrExt;

use cli_master_core::ApiError;
use cli_master_core::wire::{
    FileEntry, FileEntryKind, FileListRequest, FileListResponse, FileName, FilePath,
};
use rustix::fs::{AtFlags, Dir, FileType, fstat, statat};
use rustix::io::Errno;

use super::descriptor::{Fingerprint, identity, open_directory, revalidate_namespace};
use super::error::{api, io_error, target_changed};
use super::{FileTargetAccess, LocalFileService, now_ms};

const MAX_ENUMERATED_ENTRIES: usize = 10_000;
const MAX_PAGE_BYTES: usize = 512 * 1_024;
const PAGE_OVERHEAD_BYTES: usize = 16 * 1_024;

pub(super) fn list(
    service: &LocalFileService,
    request: &FileListRequest,
    access: &impl FileTargetAccess,
) -> Result<FileListResponse, ApiError> {
    let resolved = access.resolve_target(&request.target, false)?;
    let (root, root_identity) = service.open_target(&request.target, &resolved)?;
    let directory = open_directory(&root, request.path_base64.as_bytes())?;
    let directory_identity = identity(&fstat(&directory).map_err(io_error)?);
    let names = enumerate_names(&directory)?;
    let mut entries = Vec::new();
    let mut last_name = None;
    let mut next_after_name_base64 = None;
    let mut page_bytes = PAGE_OVERHEAD_BYTES;
    for name in names {
        if request
            .after_name_base64
            .as_ref()
            .is_some_and(|cursor| name.as_slice() <= cursor.as_bytes())
        {
            continue;
        }
        let stat = match statat(
            &directory,
            OsStr::from_bytes(&name),
            AtFlags::SYMLINK_NOFOLLOW,
        ) {
            Ok(stat) => stat,
            Err(Errno::NOENT) => continue,
            Err(error) => return Err(io_error(error)),
        };
        let mut path = request.path_base64.as_bytes().to_vec();
        if !path.is_empty() {
            path.push(b'/');
        }
        path.extend_from_slice(&name);
        let kind = match FileType::from_raw_mode(stat.st_mode) {
            FileType::RegularFile => FileEntryKind::File,
            FileType::Directory => FileEntryKind::Directory,
            FileType::Symlink => FileEntryKind::Symlink,
            _ => FileEntryKind::Other,
        };
        let entry = FileEntry {
            path_base64: FilePath::try_from_bytes(path).map_err(|_| {
                api(
                    "file_listing_too_large",
                    "A directory entry exceeds the supported relative-path limit.",
                )
            })?,
            display_name: display_name(&name),
            kind,
            size_bytes: if kind == FileEntryKind::File {
                u64::try_from(stat.st_size).ok()
            } else {
                None
            },
            modified_at_ms: Fingerprint::from_stat(&stat).modified_at_ms(),
        };
        let entry_bytes = serde_json::to_vec(&entry)
            .map_err(|_| api("file_io_error", "The directory entry could not be encoded."))?
            .len()
            + 1;
        if entries.len() >= usize::from(request.limit) || page_bytes + entry_bytes > MAX_PAGE_BYTES
        {
            next_after_name_base64 = last_name;
            break;
        }
        page_bytes += entry_bytes;
        entries.push(entry);
        last_name = Some(FileName::try_from_bytes(name).map_err(|_| {
            api(
                "file_io_error",
                "The directory cursor could not be encoded.",
            )
        })?);
    }
    if access.resolve_target(&request.target, false)?.root != resolved.root {
        return Err(target_changed());
    }
    revalidate_namespace(
        &resolved.root,
        root_identity,
        request.path_base64.as_bytes(),
        directory_identity,
    )?;
    Ok(FileListResponse {
        entries,
        next_after_name_base64,
        observed_at_ms: now_ms()?,
    })
}

fn enumerate_names(directory: &OwnedFd) -> Result<Vec<Vec<u8>>, ApiError> {
    let mut names = Vec::new();
    for entry in Dir::read_from(directory).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let name = entry.file_name().to_bytes();
        if matches!(name, b"." | b"..") {
            continue;
        }
        if names.len() == MAX_ENUMERATED_ENTRIES {
            return Err(api(
                "file_listing_too_large",
                "This directory exceeds the 10,000-entry enumeration limit.",
            ));
        }
        names.push(name.to_vec());
    }
    names.sort_unstable();
    names.dedup();
    Ok(names)
}

/// Valid Unicode stays readable; undecodable/control bytes are explicit escapes.
fn display_name(bytes: &[u8]) -> String {
    let mut display = String::new();
    let mut remaining = bytes;
    while !remaining.is_empty() {
        match std::str::from_utf8(remaining) {
            Ok(text) => {
                append_text(&mut display, text);
                break;
            }
            Err(error) => {
                let (valid, invalid) = remaining.split_at(error.valid_up_to());
                if let Ok(text) = std::str::from_utf8(valid) {
                    append_text(&mut display, text);
                }
                let invalid_length = error.error_len().unwrap_or(invalid.len());
                for byte in &invalid[..invalid_length] {
                    use std::fmt::Write;
                    let _ = write!(display, "\\x{byte:02x}");
                }
                remaining = &invalid[invalid_length..];
            }
        }
    }
    display
}

fn append_text(display: &mut String, text: &str) {
    for character in text.chars() {
        if character.is_control() {
            display.extend(character.escape_default());
        } else {
            display.push(character);
        }
    }
}
