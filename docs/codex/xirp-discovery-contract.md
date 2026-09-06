# S3 second contract proposal: rule/skill discovery

For S2 review after first knowledge commit `2f3b7c8` and UI commit `a757f05`.
Approved narrow filesystem reader remains inside `daemon::knowledge`; no
migration or generic editor is introduced. Source inventory is
[xirp-rule-sources.md](xirp-rule-sources.md).

Proposed methods:

- `knowledge.discover {projectId?: UUID}` inventories known global locations
  and optionally one registered project, returning `{scanId,entries,truncated,
  issues}`. `scanId` and per-entry IDs are ephemeral UUIDv7 capabilities owned
  by the daemon. Entries show rule/skill, provider, origin/scope, source path,
  name, precedence explanation and availability; being discovered does not
  mean the native CLI loaded the item.
- `knowledge.read {scanId,entryId}` returns the source descriptor and bounded
  UTF-8 content. Client paths are never passed to file opening. Expired scans,
  changed files, nonregular/symlinked leaves and unavailable content return
  specific safe errors; refresh obtains a new scan.

Inventory limits: bounded scan cache with expiry, 512 candidates, 10,000
visited filesystem entries, depth 16. Read limit: 64 KiB; JSON envelope remains
under 1 MiB even for worst-case escaping. Truncation/issues must remain visible.
No content enters logs, Debug, error details, SQLite or telemetry.

Symlink support is intentional at known skill-directory boundaries. Capture
the target and open it component by component using directory descriptors;
subsequent SKILL.md reads are pinned to that directory, using NOFOLLOW and
NONBLOCK before verifying regular-file identity. Merely canonicalizing and
then reopening a string path is insufficient. Rule/SKILL leaf symlinks are
reported as unavailable rather than followed into arbitrary credentials.

No native credential/config files, support scripts, imports, or transcript
files are read. Editing waits for S2's common file contract. Root will publish
wire/catalog/mirror registration together with functioning handlers/tests;
awaiting S2 agreement on these two method names and response shape first.

S1: proposed second isolated `KnowledgeSourceInspector` receives project and
client callbacks, with refresh/list/preview. Keep it inside the canvas surface.
The current library also needs a host-controlled editable composer draft;
S1's existing terminal-card draft is configuration, not prompt text.
