# Darwin ACL boundary

ADR [0006](../../docs/adr/0006-local-files-and-editor.md) accepts this narrow
exception because the editor must inspect metadata on its already-open file.
All public APIs are safe Rust. Only the private `darwin` module permits unsafe
code; the workspace and other crates retain `unsafe_code = "forbid"`.

The three bindings match `sys/acl.h` in Apple's installed macOS SDK:
`acl_get_fd_np(int, acl_type_t) -> acl_t`,
`acl_get_entry(acl_t, int, acl_entry_t *) -> int`, and
`acl_free(void *) -> int`. Opaque object and entry pointers are never
dereferenced by Rust. The positive `acl_type_t` enum uses C unsigned-int ABI;
`ACL_TYPE_EXTENDED` is `0x100`, and `ACL_FIRST_ENTRY` is `0`.
`sys/errno.h` defines `ENOENT = 2` and `EINVAL = 22` on Darwin.

- A borrowed `AsFd` descriptor remains valid for the entire inspection. The
  helper neither closes it nor constructs a pathname from it.
- A successful `acl_get_fd_np` allocates an independent ACL. `OwnedAcl` owns
  and frees it exactly once, including error returns. Its pointer is private;
  it has no clone operation or manual `Send`/`Sync` implementation.
- `acl_get_entry` receives a live ACL and writable pointer-sized output. The
  borrowed entry is never read or exported. Darwin returns **0 for success**,
  and **-1/EINVAL for no first entry** in a valid allocated ACL.
  Both mean an ACL is present: even an allocated empty ACL may carry ACL-level
  inheritance flags, so the helper conservatively preserves that policy.
- No-ACL files can instead return NULL/ENOENT from `acl_get_fd_np` because
  `FILESEC_ACL` is absent. Only this documented absence maps to `false` at
  acquisition; other OS errors propagate. Errno is captured immediately,
  before the RAII destructor can call `acl_free`.
- This is an observation, not synchronization with other writers. Atomic save
  still requires the daemon's metadata and revision rechecks. This helper
  does not copy metadata or assert that a later replacement is safe.

These semantics follow Apple's primary
[get-ACL documentation](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man3/acl_get.3.html),
[entry documentation](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man3/acl_get_entry.3.html),
[ACL file implementation](https://github.com/apple-oss-distributions/Libc/blob/main/posix1e/acl_file.c),
[entry implementation](https://github.com/apple-oss-distributions/Libc/blob/main/posix1e/acl_entry.c), and
[security-property implementation](https://github.com/apple-oss-distributions/Libc/blob/main/gen/filesec.c).
The constants and signatures were also checked against
`/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk/usr/include/sys/acl.h`
and `sys/errno.h` on the development host.

Integration tests use the safe public API on real temporary files, covering
no ACL, explicit ACL, inheritance, ACL removal, and renamed/replaced paths.
An ACL that denies reading security metadata verifies error propagation.
Only test fixture setup invokes `/bin/chmod`, directly with an argv array;
the library has no subprocess or filesystem-path dependency. On Linux the
helper returns `Unsupported`; the daemon's safe rustix xattr checks own Linux
ACL detection.
