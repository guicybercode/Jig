//! Minimal bindings verified against Apple's SDK `sys/acl.h` and `sys/errno.h`.
//! See `../SAFETY.md` for the ownership and return-value audit.

use std::ffi::{c_int, c_uint, c_void};
use std::io;
use std::os::fd::{AsRawFd, BorrowedFd};
use std::ptr::{self, NonNull};

// acl_type_t is a C enum whose defined values are nonnegative; its Darwin ABI
// representation is unsigned int. acl_t and acl_entry_t are opaque pointers.
const ACL_TYPE_EXTENDED: c_uint = 0x0000_0100;
const ACL_FIRST_ENTRY: c_int = 0;
const ENOENT: c_int = 2;
const EINVAL: c_int = 22;

unsafe extern "C" {
    fn acl_get_fd_np(fd: c_int, acl_type: c_uint) -> *mut c_void;
    fn acl_get_entry(acl: *mut c_void, entry_id: c_int, entry: *mut *mut c_void) -> c_int;
    fn acl_free(object: *mut c_void) -> c_int;
}

/// Owns exactly one allocation returned by `acl_get_fd_np`.
struct OwnedAcl(NonNull<c_void>);

impl Drop for OwnedAcl {
    fn drop(&mut self) {
        // SAFETY: this non-null pointer came from acl_get_fd_np and is freed
        // exactly once. No ACL entry pointer escapes or survives this owner.
        let _ = unsafe { acl_free(self.0.as_ptr()) };
    }
}

pub(super) fn has_extended_acl(fd: BorrowedFd<'_>) -> io::Result<bool> {
    // SAFETY: BorrowedFd keeps the descriptor valid for this call. The supported
    // ACL type is fixed, and the returned ACL is a separately owned allocation.
    let acl = unsafe { acl_get_fd_np(fd.as_raw_fd(), ACL_TYPE_EXTENDED) };
    let Some(acl) = NonNull::new(acl) else {
        let error = io::Error::last_os_error();
        // Darwin's filesec_get_property(FILESEC_ACL) reports ENOENT when the
        // descriptor's security metadata has no ACL property. No path is used.
        return if error.raw_os_error() == Some(ENOENT) {
            Ok(false)
        } else {
            Err(error)
        };
    };
    let acl = OwnedAcl(acl);
    let mut entry = ptr::null_mut();
    // SAFETY: the owned ACL remains live, ACL_FIRST_ENTRY is a valid selector,
    // and entry is initialized writable storage for the borrowed output pointer.
    let result = unsafe { acl_get_entry(acl.0.as_ptr(), ACL_FIRST_ENTRY, &raw mut entry) };
    if result == 0 {
        return Ok(true);
    }

    // Capture errno before OwnedAcl::drop calls another C function. Darwin
    // returns -1/EINVAL when the first entry of a valid ACL does not exist.
    // Unlike Linux's ACL API, Darwin does not return zero for end-of-list.
    let error = io::Error::last_os_error();
    if result == -1 && error.raw_os_error() == Some(EINVAL) {
        // An existing but empty ACL can still have ACL-level inheritance flags.
        // Preserve that distinction from a missing ACL property: the editor
        // must reject replacement rather than discard uninspected policy.
        Ok(true)
    } else {
        Err(error)
    }
}
