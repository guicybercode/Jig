# S3 rule/skill discovery contract

Approved by S2 in `xirp-coordination-reply.md` after first knowledge commit
`2f3b7c8` and UI commit `a757f05`.
Approved narrow filesystem reader remains inside `daemon::knowledge`; no
migration or generic editor is introduced. Source inventory is
[xirp-rule-sources.md](xirp-rule-sources.md).

Methods:

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

Inventory limits: four scans cached for five minutes, 512 candidates, 10,000
visited filesystem entries, depth 16. The selected project is inventoried before
globals. Entry JSON is capped at 448 KiB; at most 32 issues use up to 32 KiB. Read limit: 64 KiB; JSON envelope remains
under 1 MiB even for worst-case escaping. Truncation/issues must remain visible.
No content enters logs, Debug, error details, SQLite or telemetry.

Symlink support is intentional at known skill-directory boundaries. Capture
the target and open it component by component using directory descriptors;
subsequent SKILL.md reads are pinned to that directory, using NOFOLLOW and
NONBLOCK before verifying regular-file identity. Merely canonicalizing and
then reopening a string path is insufficient. Rule/SKILL leaf symlinks are
reported as unavailable rather than followed into arbitrary credentials.

Root locations come from daemon-owned HOME/CODEX_HOME and registered projects.
Their directory aliases are resolved using descriptors during capture, then
reads reopen the captured resolved path with NOFOLLOW and compare directory
and file identities. The viaSymlink flag identifies skill-directory links,
not OS root aliases such as macOS /var.

A skill-directory link retargeted after discovery does not redirect an existing
capability: that scan still reads its captured original target if unchanged.
A fresh scan follows the new target. Source replacement/content edits conflict;
daemon restart, cache eviction and five-minute expiry invalidate scans.
Removing the registered project invalidates reads from its scan.

This increment inventories root configuration locations and recursively grouped
rules/skills within those locations. It does not walk every project subfolder,
evaluate session-cwd ancestors, import rules, parse activation conditions, read
native overrides, or enumerate bundled/plugin sources. These limits are visible
as inventory issues and remain open parity work, not claims of native loading.

No native credential/config files, support scripts, imports, or transcript
files are read. Editing waits for S2's common file contract. Root will publish
wire/catalog/mirror registration together with functioning handlers/tests.
All five knowledge operations run through spawn_blocking, preserving the S2
composition change and existing generic transport.

S1: isolated `KnowledgeSourceInspector` receives client, optional currentProject
and optional connectionKey (daemon instance or reconnect generation), with
refresh/list/preview. Pass the connection key to invalidate scans on reconnect. Keep it inside the canvas surface.
The current library also needs a host-controlled editable composer draft;
S1's existing terminal-card draft is configuration, not prompt text.
