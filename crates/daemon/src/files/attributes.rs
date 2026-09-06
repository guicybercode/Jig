//! Only explicitly supported, bounded attributes may survive atomic replacement.

use std::fs::File;

use cli_master_core::ApiError;
use rustix::fs::flistxattr;

use super::error::metadata_unsupported;

#[derive(Eq, PartialEq)]
pub(super) struct SupportedAttributes {
    #[cfg(target_os = "macos")]
    provenance: Option<Vec<u8>>,
}

#[cfg(target_os = "linux")]
pub(super) fn read_supported(file: &File) -> Result<SupportedAttributes, ApiError> {
    let mut names = [0_u8; 1];
    match flistxattr(file, &mut names[..]) {
        Ok(0) => Ok(SupportedAttributes {}),
        Ok(_) | Err(_) => Err(metadata_unsupported()),
    }
}

#[cfg(target_os = "macos")]
const PROVENANCE_NAME: &str = "com.apple.provenance";
#[cfg(target_os = "macos")]
const MAX_PROVENANCE_BYTES: usize = 4_096;

#[cfg(target_os = "macos")]
pub(super) fn read_supported(file: &File) -> Result<SupportedAttributes, ApiError> {
    let mut names = [0_u8; PROVENANCE_NAME.len() + 1];
    let length = flistxattr(file, &mut names[..]).map_err(|_| metadata_unsupported())?;
    if length == 0 {
        return Ok(SupportedAttributes { provenance: None });
    }
    // The list must contain exactly this one NUL-terminated name. A longer
    // list fails at the syscall bound, including resource forks and ACL xattrs.
    if length != names.len()
        || &names[..PROVENANCE_NAME.len()] != PROVENANCE_NAME.as_bytes()
        || names[PROVENANCE_NAME.len()] != 0
    {
        return Err(metadata_unsupported());
    }
    let mut value = vec![0_u8; MAX_PROVENANCE_BYTES];
    let length = rustix::fs::fgetxattr(file, PROVENANCE_NAME, &mut value[..])
        .map_err(|_| metadata_unsupported())?;
    value.truncate(length);
    Ok(SupportedAttributes {
        provenance: Some(value),
    })
}

pub(super) fn preserve(source: &File, destination: &File) -> Result<(), ApiError> {
    let original = read_supported(source)?;
    let staged = read_supported(destination)?;
    if original != staged {
        #[cfg(target_os = "macos")]
        if let Some(value) = original.provenance.as_ref() {
            rustix::fs::fsetxattr(
                destination,
                PROVENANCE_NAME,
                value,
                rustix::fs::XattrFlags::empty(),
            )
            .map_err(|_| metadata_unsupported())?;
        }
        // Darwin may report success while retaining an OS-owned provenance
        // value. Never infer preservation from the write syscall alone.
        if original != read_supported(destination)? {
            return Err(metadata_unsupported());
        }
    }
    verify_preserved(source, destination)
}

pub(super) fn verify_preserved(source: &File, destination: &File) -> Result<(), ApiError> {
    if read_supported(source)? != read_supported(destination)? {
        return Err(metadata_unsupported());
    }
    Ok(())
}
