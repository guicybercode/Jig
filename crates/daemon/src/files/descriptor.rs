use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::Read;
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path};

use cli_master_core::ApiError;
use cli_master_core::wire::{FilePath, FileRevision, MAX_FILE_TEXT_BYTES};
use rustix::fs::{AtFlags, FileType, Mode, OFlags, Stat, fstat, open, openat, statat};
use sha2::{Digest, Sha256};

use super::error::{api, conflict, io_error, target_changed};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct Identity {
    pub device: i128,
    pub inode: u128,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Fingerprint {
    pub identity: Identity,
    pub size: i128,
    pub modified_seconds: i128,
    pub modified_nanos: i128,
    pub changed_seconds: i128,
    pub changed_nanos: i128,
    pub links: u128,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub flags: u32,
}

impl Fingerprint {
    #[allow(
        clippy::useless_conversion,
        reason = "Unix stat field widths differ between Linux and macOS"
    )]
    pub(super) fn from_stat(stat: &Stat) -> Self {
        Self {
            identity: identity(stat),
            size: stat.st_size.into(),
            modified_seconds: stat.st_mtime.into(),
            modified_nanos: stat.st_mtime_nsec.into(),
            changed_seconds: stat.st_ctime.into(),
            changed_nanos: stat.st_ctime_nsec.into(),
            links: stat.st_nlink.into(),
            mode: stat.st_mode.into(),
            uid: stat.st_uid,
            gid: stat.st_gid,
            #[cfg(target_os = "macos")]
            flags: stat.st_flags,
            #[cfg(not(target_os = "macos"))]
            flags: 0,
        }
    }

    pub(super) fn modified_at_ms(&self) -> Option<i64> {
        self.modified_seconds
            .checked_mul(1_000)?
            .checked_add(self.modified_nanos.checked_div(1_000_000)?)?
            .try_into()
            .ok()
    }

    pub(super) fn revision(&self, bytes: &[u8]) -> Result<FileRevision, ApiError> {
        let mut hash = Sha256::new();
        hash.update(b"cli-master-file-revision-v1\0");
        hash.update(self.identity.device.to_be_bytes());
        hash.update(self.identity.inode.to_be_bytes());
        for value in [
            self.size,
            self.modified_seconds,
            self.modified_nanos,
            self.changed_seconds,
            self.changed_nanos,
        ] {
            hash.update(value.to_be_bytes());
        }
        hash.update(self.links.to_be_bytes());
        for value in [self.mode, self.uid, self.gid, self.flags] {
            hash.update(value.to_be_bytes());
        }
        hash.update(bytes);
        FileRevision::try_new(format!("v1:{:x}", hash.finalize()))
            .map_err(|_| api("file_io_error", "The file revision could not be encoded."))
    }
}

pub(super) fn identity(stat: &Stat) -> Identity {
    Identity {
        device: stat.st_dev.into(),
        inode: stat.st_ino.into(),
    }
}

pub(super) fn directory_flags() -> OFlags {
    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC
}

/// Walk even the registered absolute root by descriptor; an intermediate
/// symlink cannot redirect a root open after canonical-path validation.
pub(super) fn open_root(path: &Path) -> Result<OwnedFd, ApiError> {
    if !path.is_absolute() || path.canonicalize().map_err(|_| target_changed())? != path {
        return Err(target_changed());
    }
    let mut directory = open("/", directory_flags(), Mode::empty()).map_err(io_error)?;
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(name) => {
                directory = openat(&directory, name, directory_flags(), Mode::empty())
                    .map_err(|_| target_changed())?;
            }
            _ => return Err(target_changed()),
        }
    }
    Ok(directory)
}

pub(super) fn open_directory(root: &impl AsFd, bytes: &[u8]) -> Result<OwnedFd, ApiError> {
    let mut directory = openat(root, ".", directory_flags(), Mode::empty()).map_err(io_error)?;
    if !bytes.is_empty() {
        for component in bytes.split(|byte| *byte == b'/') {
            let name = OsStr::from_bytes(component);
            let observed = statat(&directory, name, AtFlags::SYMLINK_NOFOLLOW).map_err(io_error)?;
            if FileType::from_raw_mode(observed.st_mode) == FileType::Symlink {
                return Err(api(
                    "file_symlink_not_allowed",
                    "Symbolic links cannot be followed by the editor.",
                ));
            }
            directory =
                openat(&directory, name, directory_flags(), Mode::empty()).map_err(io_error)?;
        }
    }
    Ok(directory)
}

pub(super) fn split_file_path(path: &FilePath) -> Result<(&[u8], OsString), ApiError> {
    let bytes = path.as_bytes();
    if bytes.is_empty() {
        return Err(super::error::invalid_input());
    }
    match bytes.iter().rposition(|byte| *byte == b'/') {
        Some(index) => Ok((
            &bytes[..index],
            OsString::from_vec(bytes[index + 1..].to_vec()),
        )),
        None => Ok((&[], OsString::from_vec(bytes.to_vec()))),
    }
}

pub(super) struct TextFile {
    pub file: File,
    pub fingerprint: Fingerprint,
    pub text: String,
    pub revision: FileRevision,
}

pub(super) fn read_leaf(parent: &impl AsFd, name: &OsStr) -> Result<TextFile, ApiError> {
    let observed = statat(parent, name, AtFlags::SYMLINK_NOFOLLOW).map_err(io_error)?;
    require_regular(&observed)?;
    let fd = openat(
        parent,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(io_error)?;
    let before = fstat(&fd).map_err(io_error)?;
    require_regular(&before)?;
    let fingerprint = Fingerprint::from_stat(&before);
    if fingerprint.size
        > i128::try_from(MAX_FILE_TEXT_BYTES).map_err(|_| super::error::invalid_input())?
    {
        return Err(api(
            "file_too_large",
            "The selected file exceeds the 128 KiB text limit.",
        ));
    }
    let mut file = File::from(fd);
    let mut bytes = Vec::new();
    (&mut file)
        .take(u64::try_from(MAX_FILE_TEXT_BYTES + 1).map_err(|_| super::error::invalid_input())?)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() > MAX_FILE_TEXT_BYTES {
        return Err(api(
            "file_too_large",
            "The selected file exceeds the 128 KiB text limit.",
        ));
    }
    let after = Fingerprint::from_stat(&fstat(&file).map_err(io_error)?);
    if before.st_ino != observed.st_ino || before.st_dev != observed.st_dev || fingerprint != after
    {
        return Err(conflict(None));
    }
    if bytes.contains(&0) {
        return Err(api(
            "file_not_text",
            "The selected file contains binary data.",
        ));
    }
    let text = String::from_utf8(bytes).map_err(|_| {
        api(
            "file_not_text",
            "The selected file is not valid UTF-8 text.",
        )
    })?;
    let revision = fingerprint.revision(text.as_bytes())?;
    Ok(TextFile {
        file,
        fingerprint,
        text,
        revision,
    })
}

pub(super) fn require_regular(stat: &Stat) -> Result<(), ApiError> {
    match FileType::from_raw_mode(stat.st_mode) {
        FileType::RegularFile => Ok(()),
        FileType::Symlink => Err(api(
            "file_symlink_not_allowed",
            "Symbolic links cannot be edited.",
        )),
        _ => Err(api(
            "file_not_regular",
            "The selected object is not a regular file.",
        )),
    }
}

pub(super) fn revalidate_namespace(
    root_path: &Path,
    root_identity: Identity,
    parent_bytes: &[u8],
    parent_identity: Identity,
) -> Result<(), ApiError> {
    let root = open_root(root_path)?;
    if identity(&fstat(&root).map_err(io_error)?) != root_identity {
        return Err(target_changed());
    }
    let parent = open_directory(&root, parent_bytes).map_err(|_| target_changed())?;
    if identity(&fstat(&parent).map_err(io_error)?) != parent_identity {
        return Err(target_changed());
    }
    Ok(())
}
