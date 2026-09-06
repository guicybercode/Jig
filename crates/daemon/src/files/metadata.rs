//! Metadata that this atomic-replacement slice can preserve without elevation.

use std::fs::File;

use cli_master_core::ApiError;
use rustix::fs::{Gid, Mode, Uid, fchmod, fchown, fstat};

use super::attributes;
use super::descriptor::Fingerprint;
use super::error::metadata_unsupported;

pub(super) fn inspect(file: &File, fingerprint: &Fingerprint) -> Result<(), ApiError> {
    // Replacing a multiply-linked inode would silently disconnect its siblings.
    // Special mode bits and platform flags are outside this plain-text slice.
    if fingerprint.links != 1 || fingerprint.mode & 0o7000 != 0 || fingerprint.flags != 0 {
        return Err(metadata_unsupported());
    }
    attributes::read_supported(file)?;
    inspect_platform(file)
}

pub(super) fn apply(
    source: &File,
    destination: &File,
    fingerprint: &Fingerprint,
) -> Result<(), ApiError> {
    // Source metadata is checked again to observe ACL/xattr changes during staging.
    inspect(source, fingerprint)?;
    let current = fstat(destination).map_err(|_| metadata_unsupported())?;
    if current.st_uid != fingerprint.uid || current.st_gid != fingerprint.gid {
        fchown(
            destination,
            Some(Uid::from_raw(fingerprint.uid)),
            Some(Gid::from_raw(fingerprint.gid)),
        )
        .map_err(|_| metadata_unsupported())?;
    }
    #[allow(
        clippy::useless_conversion,
        reason = "RawMode is u16 on macOS and u32 on Linux"
    )]
    let permissions = rustix::fs::RawMode::try_from(fingerprint.mode & 0o777)
        .map_err(|_| metadata_unsupported())?;
    fchmod(destination, Mode::from_raw_mode(permissions)).map_err(|_| metadata_unsupported())?;
    let copied = Fingerprint::from_stat(&fstat(destination).map_err(|_| metadata_unsupported())?);
    if copied.uid != fingerprint.uid
        || copied.gid != fingerprint.gid
        || copied.mode & 0o7777 != fingerprint.mode & 0o7777
    {
        return Err(metadata_unsupported());
    }
    attributes::preserve(source, destination)?;
    // A parent may have supplied inherited ACLs or attributes to the new inode.
    // Never publish an inode whose access policy differs silently from the old file.
    inspect(destination, &copied)
}

#[cfg(target_os = "macos")]
fn inspect_platform(file: &File) -> Result<(), ApiError> {
    if cli_master_file_metadata::has_extended_acl(file).map_err(|_| metadata_unsupported())? {
        return Err(metadata_unsupported());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn inspect_platform(file: &File) -> Result<(), ApiError> {
    use rustix::fs::ioctl_getflags;
    use rustix::io::Errno;

    // EXTENTS describes the regular ext4 allocation representation, not an
    // authored inode policy. Every other flag requires explicit preservation.
    const EXTENTS: u32 = 0x0008_0000;
    match ioctl_getflags(file) {
        Ok(flags) if flags.bits() & !EXTENTS == 0 => Ok(()),
        Err(Errno::NOTTY | Errno::OPNOTSUPP) => Ok(()),
        _ => Err(metadata_unsupported()),
    }
}
