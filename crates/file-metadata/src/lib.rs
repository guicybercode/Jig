//! Descriptor-based extended ACL inspection for the local file editor.
//!
//! The Darwin implementation is the narrow FFI exception accepted in ADR 0006.
//! This crate never reopens paths, changes metadata, closes the caller's file
//! descriptor, or launches a subprocess.

#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::io;
use std::os::fd::AsFd;

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod darwin;

/// Reports whether the pinned filesystem object has an extended ACL.
///
/// The result is an observation, not a lock: another process may change metadata
/// immediately afterward. Explicit and inherited entries count as present, as
/// does an allocated empty ACL, which may carry ACL-level inheritance policy.
///
/// # Errors
///
/// Returns the operating system's error if the ACL cannot be inspected. On
/// platforms other than macOS, returns [`io::ErrorKind::Unsupported`]; callers
/// must use their platform-specific ACL/xattr inspection instead of treating
/// unavailable inspection as evidence that no ACL exists.
pub fn has_extended_acl(fd: impl AsFd) -> io::Result<bool> {
    #[cfg(target_os = "macos")]
    {
        darwin::has_extended_acl(fd.as_fd())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = fd;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "descriptor ACL inspection is implemented for macOS only",
        ))
    }
}
