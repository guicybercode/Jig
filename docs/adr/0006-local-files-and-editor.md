# ADR 0006: Local files and editor I/O

## Status

Accepted for the requested local runtime increment, 2026-09-05.

This decision covers the first functional slice of M29/M30 and the shared
reader needed by M07/X06. It extends the Beta domain-operation inventory with
local file editing. It does not change IPC envelope version 1, session
ownership, worktree deletion, or Linux/macOS support. In particular, the
relative file identifiers below are a scoped extension of ADR 0002; arbitrary
absolute filesystem paths remain invalid domain-operation inputs.

## Context

The daemon currently exposes registered project, session and worktree IDs,
but no file-listing or editor-save operation. S1 needs a real file tree and
editor on the canvas. S3 needs a reusable safe reader for explicitly selected
rule/skill files; a second filesystem service would create conflicting access
and overwrite policies.

The user requested files/editor after worktree integration, including real
disk effects and restart behavior. A generic Tauri filesystem command, shell
command, or Rust type without a daemon handler would not fulfill that request.
Linux filenames are byte sequences and need not be UTF-8. A check using
`canonicalize` followed by an unrelated pathname `open` can follow a replaced
symlink. Atomic replacement prevents partial files, but portable Unix rename
does not provide compare-and-swap against an external editor's last write.

## Decision

### Ownership and initial scope

Implement `file.list`, `file.read` and `file.write` in the daemon, backed by a
single internal `LocalFileService`. Core owns pure validated values and DTOs;
it performs no filesystem, hashing of open files, clock, or process I/O.
Storage resolves existing metadata IDs and remains the only SQLite owner.
No new table or migration is required for these three operations.

Add each method to Rust wire, JSON catalog and TypeScript mirrors only with
its functional handler. The desktop continues to use `daemon_invoke`; there
is no `file_read` or other domain-specific Tauri command. Operations run in
bounded blocking tasks rather than on the asynchronous socket reader.

`file.write` replaces an existing regular text file. It never creates a
missing file or parent directory, renames a user file, or deletes one. Those
remain required future file-management increments, tentatively under
`file.create`, `file.rename`, and `file.prepare_delete`/`file.delete`; they
are not advertised or implemented as stubs now. Creation will require
exclusive creation, rename a no-clobber/state policy, and deletion a reviewed
state-bound target. This sequencing does not remove M29's remaining scope.

### Target and byte-exact path contracts

Every request carries one registered target:

```text
FileTarget = { kind: "project", projectId: UUIDv7 }
           | { kind: "session", sessionId: UUIDv7 }
           | { kind: "worktree", worktreeId: UUIDv7 }
FilePath   = canonical padded base64 of repository-independent Unix path bytes
FileRevision = "v1:" + 64 lowercase hexadecimal SHA-256 characters
```

`FileTarget` has the same tagged representation as the existing Git target,
but file semantics belong to this service. A project resolves to its
registered selected directory, a session to its persisted cwd, and a worktree
to its managed root. The service must not expand a project subdirectory to
the repository root. Worktree-backed targets require an active association;
creating/orphaned/removal-pending targets are not writable. Reads may explain
recovery state, but must not substitute another directory silently.

Roots are derived from daemon metadata and validated on every operation;
clients do not echo a root path back as authority. Root opens must reject a
persisted canonical root that now resolves elsewhere. Managed worktree
identity checks reuse the runtime worktree validation before granting access.

`pathBase64` is limited to 4096 decoded bytes. Empty bytes represent the root
only for `file.list`. Nonempty paths are relative `/`-separated components;
reject leading/trailing `/`, empty components, NUL, `.` and `..`. Names with
spaces, Unicode, non-UTF-8 bytes, leading `-`, literal backslashes or colons
remain valid Unix names. There is no Windows-path normalization, Unicode
normalization, case folding or Git pathspec interpretation. The existing
`GitRelativePath` is deliberately not reused.

Responses return `pathBase64` as the exact identifier and `displayName` for
presentation. Escape control/undecodable bytes in display text; never rebuild
an operation path from that text. Raw names never become HTML or command
arguments. Non-UTF-8 names are listable and can identify an editable UTF-8
file where the underlying filesystem permits such names. APFS can reject
invalid UTF-8 filenames with EILSEQ before the service is called; Linux socket
tests exercise those names and pure contract tests cover their wire identity
on both platforms.

### Public operations

All payloads use camelCase, reject unknown fields and keep existing versioned
response envelopes. Optional fields are omitted when absent.

| Method | Request | Successful response |
| --- | --- | --- |
| `file.list` | `{ target, pathBase64, limit?, afterNameBase64? }` | `{ entries: FileEntry[], nextAfterNameBase64?, observedAtMs }` |
| `file.read` | `{ target, pathBase64 }` | `{ pathBase64, text, revision, sizeBytes, modifiedAtMs?, observedAtMs }` |
| `file.write` | `{ target, pathBase64, text, expectedRevision }` | `{ pathBase64, revision, sizeBytes, modifiedAtMs?, writtenAtMs }` |

`FileEntry` is `{ pathBase64, displayName, kind, sizeBytes?, modifiedAtMs? }`.
Kind is `file | directory | symlink | other`; it describes observed type, not
an access grant. Symlinks are visible but not traversed or edited. `other`
includes devices, sockets and FIFOs and cannot be opened through this API.

List one directory, not a recursive tree. Default page size is 100, maximum
200. Order by raw name bytes; `afterNameBase64` is a validated single filename
and is an exclusive lexicographic cursor. Bound enumeration to 10,000 entries
and the encoded response to 512 KiB, returning a continuation cursor before
exceeding the response budget. A directory exceeding the enumeration limit
returns `file_listing_too_large`, never a success implying an empty tree.
Pages are observations, not a transaction snapshot: concurrent changes before
the cursor may require refresh. Search and persistent indexing are separate
increments.

Read/write text is limited to **128 KiB of UTF-8 bytes**, including any BOM.
This bound allows worst-case JSON escaping inside the existing 1 MiB frame
with room for the envelope. Preserve bytes represented by the text exactly,
including CRLF, final newline and BOM; do not normalize automatically.
The service reads at most the limit plus one byte and rejects oversized
files, invalid UTF-8 or NUL-containing content. Binary/media previews need a
separate bounded byte/asset operation; this text slice does not claim them.
`file.list` can still show those files. The UI keeps its unsaved buffer after
every error.

All timestamp fields are Unix epoch milliseconds. File modification time is
optional if it cannot be represented; absence is not zero. Revision checks
use higher-resolution metadata internally and never depend on epoch-ms alone.

### Descriptor-relative filesystem access

Use safe APIs from the existing direct daemon dependency `rustix 1.1.4` with
feature `fs`. Its local source provides `openat`, `statat`, `fstat`, `renameat`,
`unlinkat`, directory iteration and `fsync`. Traversal and publication introduce no project `unsafe` block, shell
command, external file utility or platform-specific path syntax. Darwin ACL
inspection has the narrowly isolated exception defined below. `renameat` is the portable directory-relative replacement operation.
[rustix reference](https://docs.rs/rustix/1.1.4/rustix/fs/fn.renameat.html)

Open the validated root as a directory descriptor, then walk each path
component relative to its already-open parent with
`RDONLY | DIRECTORY | NOFOLLOW | CLOEXEC`. Open a leaf with
`RDONLY | NOFOLLOW | NONBLOCK | CLOEXEC`, then `fstat` that descriptor and
require a regular file before reading. `NONBLOCK` prevents a FIFO substituted
at the leaf from hanging the daemon. Use no-follow stat for listing entries;
never open an `other` entry as if it were ordinary text.

Keep the parent descriptor through validation, temporary-file creation and
rename. Reopening an absolute concatenated pathname after validation is not
an acceptable fallback. Revalidate root/parent device+inode identity before
publication; if metadata registration or directory identity changed, abort
with `file_target_changed` and discard only the service-owned temporary file.
For managed worktrees, hold a short operation lease against worktree removal
for the final write transaction. File reads need no process lifecycle lock.
File I/O must not delay stop of an unrelated session behind enumeration.

Descriptor-relative traversal prevents symlink substitution from redirecting
access. It does not promise that a directory descriptor's inode remains at
the same pathname if another same-user process renames that directory. The
service acts on the opened object and detects observed namespace changes;
it is not a security sandbox against another process with the same Unix UID.

### Revision checks and atomic save

Compute revision in the daemon over file bytes plus identity and change
metadata: device, inode, size, high-resolution mtime/ctime, link count and
permission mode. Encode fields canonically with a version prefix and hash
with SHA-256; the resulting `FileRevision` remains opaque to clients. Make
`sha2 0.10.9`, already resolved in the workspace lockfile, a direct daemon
dependency if used. The revision is content/identity-based, not a database
counter or process-local token, so unchanged files can be checked after a
daemon restart. It is not an authorization credential.

For read, compare descriptor metadata before and after the bounded read;
return `file_conflict` if a concurrent change was observed. For write:

1. Acquire a service mutation lock keyed by parent device/inode and raw leaf
   name. It must serialize overlapping requests even when target IDs differ.
2. Open/inspect the current leaf by descriptor, reject nonregular files and
   multiple hard links, read its bounded bytes, and require the caller's
   `expectedRevision` to match the observed revision.
3. Create a unique sibling temporary file with `CREATE | EXCL | NOFOLLOW`,
   initially mode 0600. Write the complete new text, apply the supported
   permission metadata, flush and `fsync` the temporary descriptor.
4. Reopen/reinspect the current leaf through the same parent, verify identity
   and revision again and revalidate the target/parent. On conflict leave the
   original untouched and remove only that operation's temporary file.
5. Atomically `renameat` the temporary sibling onto the existing name, then
   `fsync` the parent directory. Return a revision for the published inode.

Ownership/group and ordinary permission bits must be preserved when saving;
if this cannot be done without elevated access, reject the write. The first
slice is not a full metadata-preserving file copier: ACLs, extended attributes,
resource forks and platform flags need explicit preservation or a documented
unsupported result before accepting such a save. They must not be silently
advertised as preserved. Hard-linked files return `file_metadata_unsupported`
because replace would otherwise break the link relationship.

The initial macOS implementation preserves one explicitly supported extended
attribute, `com.apple.provenance`. Real files created on the validation host
receive this attribute automatically, including the service's temporary files.
Read its bounded value through the pinned descriptors, copy it when needed,
and verify that the source and destination attribute sets and bytes agree
before publication. If the source has no such attribute and the destination
does, reject the save unless exact absence can be established; do not assume
that a successful removal call proves absence. Reject every other extended
attribute, oversized value or inspection/copy mismatch. This is a narrow
preservation exception, not permission to silently discard unknown metadata.

The lock guarantees revision ordering among daemon writers. External editors
do not honor it. The final comparison followed by POSIX `renameat` leaves a
small external-write race: it is **optimistic conflict detection**, not a
filesystem compare-and-swap or a guarantee of zero lost updates against an
uncooperative writer. This limitation must remain in editor/save documentation
and tests must not claim to prove its absence. A later stronger design may
add recoverable versions or platform primitives under another ADR.

Failure before rename does not alter the destination. Failure after rename
is materially different: return `file_durability_uncertain` with
`writeApplied: true` and the observed revision when possible, instructing the
client to re-read before retrying. Do not pretend rollback occurred or send a
second implicit write. A daemon crash can leave a uniquely named temporary
file; startup must not sweep unknown files from project directories.

### Narrow Darwin ACL metadata boundary

The current safe `rustix` interface does not provide Darwin ACL inspection
through a file descriptor. The safe public exacl API takes a pathname, which
would lose the pinned-object guarantee; `/dev/fd` is not assumed equivalent.
A normal macOS text file must remain editable while a file with an extended
ACL must not lose that ACL during atomic replacement.

Add `crates/file-metadata` solely for the safe public function
`has_extended_acl(fd: impl AsFd) -> io::Result<bool>`. Its private Darwin
module contains the minimal audited bindings to `acl_get_fd_np`,
`acl_get_entry` and `acl_free`. Constants and signatures are verified against
the installed Apple SDK. The borrowed descriptor is never closed, returned
ACL storage is owned by an RAII guard, errors preserve errno, and callers
never receive raw pointers. No subprocess or path reopening is permitted in
this service. Linux continues to inspect ACL/xattrs through safe rustix APIs.

This crate explicitly denies unsafe code except within that private module,
and denies unsafe operations within unsafe functions unless individually
marked. It is the sole exception to inheriting the workspace's unsafe-code
forbid lint; workspace/core/daemon/session policy remains unchanged. All
other lints retain the workspace's strictness. The exception enables direct
inspection rather than weakening the file write policy or rejecting every
ordinary file on macOS. Test the public safe API with real regular files and
extended ACLs, and review ownership and each unsafe call before publication.

### Errors, synchronization and S3 reuse

Use existing `ApiError` envelopes. Stable service error codes are
`file_target_not_found`, `file_target_changed`, `file_not_found`,
`file_not_directory`, `file_not_regular`, `file_symlink_not_allowed`,
`file_permission_denied`, `file_too_large`, `file_not_text`,
`file_listing_too_large`, `file_conflict`, `file_metadata_unsupported`,
`file_durability_uncertain` and `file_io_error`. Invalid encoded path, text
limit or revision shape is `invalid_input` at the wire boundary. Conflict
details can carry `currentRevision`, never full old/new text or env values.
Map OS errors without logging file content, secrets or raw program commands.

This first slice uses explicit response/re-read synchronization. Do not add a
`file.changed` catalog entry while the public socket only supports terminal
subscriptions and no implemented file event feed. S1 refreshes the saved file
and affected directory after mutation, re-reads on explicit refresh/refocus,
and re-reads after daemon reconnect. A later watcher/feed must provide a real
subscription contract, bounded events, gap/reconnect semantics and the same
epoch-ms timestamps. Polling/refresh is not described as live external edits.

Expose descriptor-safe bounded read/resolution as internal service methods
for S3. The knowledge layer supplies an explicit allowlist and provenance for
rule/skill discovery; it does not gain arbitrary root access from a path string.
Global skill roots will require registered internal read-only capabilities
separate from project editor-write targets. Credential discovery/exposure is
not part of this increment. S3 must not add a second unsafe reader, database,
shell runner or Tauri client.

### Acceptance

Use real temporary directories, SQLite and the production daemon socket:

- List and edit a file under each registered target, including a project
  subdirectory and a session with `relativeDirectory`; observe disk bytes.
- Preserve a non-UTF-8 filename, spaces, leading `-` and literal backslash;
  use only the returned identifier for follow-up requests.
- Reject absolute/traversal/NUL paths, symlink components and symlink leaves;
  swapped FIFO/device leaves do not block the request or get read as text.
- Refuse a target/root replaced between requests or during observed traversal;
  file operations and worktree removal cannot race through the final save.
- Two daemon clients reading one revision cannot both commit different writes;
  changing bytes or inode externally before validation produces conflict.
- Reject binary/oversized files and unsupported metadata without changing the
  destination. Preserve line endings, mode and complete Unicode text.
- Inject failures before rename and after rename/fsync; verify original bytes
  or the explicit applied-but-uncertain outcome respectively.
- Reconnect/restart, read the saved text/revision again, and cover pagination
  and encoded frame bounds. S3's reader uses the same safe implementation.
- Execute on Linux and macOS; compile/test wire mirrors. S1 separately proves
  open → edit → save → conflict/reload on the real desktop. Types, mocked UI,
  or this ADR alone do not mark M29/M30 complete.

## Consequences

The first editor slice has a small public surface and a single access policy
for S1/S3. Exact filename identity survives Unix/JSON boundaries. Existing
documents remain ordinary files, with no second copy silently made canonical
in SQLite. Directory and file limits are explicit and can be extended with
streaming/indexing later.

Base64 identifiers and optimistic revisions add client code; symlinked and
unsupported-metadata files initially remain read-only/unavailable for save.
Atomic replacement changes inode identity. External-writer races and richer
metadata preservation remain explicit engineering limits. No file operation
spawns a process, changes session status, writes custom-agent env values to
logs, changes Git author configuration, or creates coauthor trailers.

## Alternatives considered

- Absolute string paths or generic Tauri fs access: rejected because clients
  could bypass registered target identity and introduce a second I/O owner.
- UTF-8-only paths or Git pathspec reuse: rejected because valid Unix filenames
  would become unaddressable or acquire unrelated Git argument restrictions.
- Canonicalize then normal open: rejected as the sole safety mechanism;
  descriptor-relative no-follow traversal is available on both supported OSes.
- Truncate/write in place: rejected because interruption can destroy the
  original and partial bytes are visible to agents/editors.
- Unconditional last-writer-wins: rejected because ordinary stale editors
  would overwrite external work silently. Optimistic revision detects observed
  conflicts while accurately describing the remaining portable rename race.
- Announce all future methods/events immediately: rejected because advertised
  names without usable handlers or event delivery are not functionality.
