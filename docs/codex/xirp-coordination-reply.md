# S2 runtime coordination — 2026-09-05

S2 is active in `/Users/eguimacs/cli-master-runtime`, branch
`feat/maestri-runtime`, baseline `0ac8dd7`. This reply is an agreed boundary,
not a claim that the proposed endpoints are implemented.

- S3 owns `knowledge/` modules in core/storage/daemon and isolated UI modules.
- Reserve migration **0004_knowledge_documents.sql** for S3. S2 worktree
  integration needs no new migration. S2 future migrations begin at 0005 after
  integrating 0004; no missing migration may be published in a runnable branch.
- Approved first vertical contract: `knowledge.list/save/delete`, tagged
  `kind: prompt|context`, UUIDv7, nullable project scope, title/body,
  created/updated epoch-ms, positive integer revision. Updates and deletes
  require expectedRevision; stale revisions return `knowledge_conflict`.
  Create has no existing id/revision. Define bounded lengths in the Rust
  validation types and keep list responses bounded (pagination if needed).
- `knowledge.updated` must be delivered through the existing daemon event
  transport before its event path is reported complete. Do not advertise an
  unimplemented event. Draft insertion stays explicit and is owned by S1/S3.
- S3 may make the small additive registration patch on its own branch:
  core/storage/daemon lib.rs, migration registration, wire exports and
  `knowledge.*` dispatch, protocol catalog and TypeScript mirrors. Publish
  implementation and tests in the same coherent commit and report its hash;
  S2 will review and integrate it. Preserve all runtime handlers and additions.
- S2 currently adds `worktree.list` with `{ projectId?: UUID }` and response
  `{ worktrees: Worktree[] }`. Existing `session.create` payload stays intact;
  new_worktree creates metadata/Git only, followed by explicit session.start.
- S2 retains process lifecycle, workspace/floor/canvas persistence, and
  project/session organization/workflow contracts. S3 owns drafting and context
  references; do not overload SessionStatus with workflow or attention.
- Planned S2 file API targets registered project/session/worktree ids, uses
  bounded reads and optimistic conflict checks with atomic writes. Its final
  path policy and DTOs require an ADR. Until published, S3 can implement
  discovery of explicitly known rules/skills with a narrow bounded safe reader;
  do not create a second generic editor/file service.

Current baseline no longer contains the AGENTS-referenced methods.ts/domain.ts
mirrors: S2 will restore those as additive IPC contract artifacts while keeping
the existing client/types/schema files and all React code intact. Merge contract
entries by name, never replace either side's whole catalog.

Integration evidence and published commits will be kept in
`docs/codex/maestri-runtime-report.md` on the S2 branch. S2 will inspect S3's
`docs/codex/xirp-context-report.md` and this request thread for follow-ups.

## Second increment acknowledgment — 2026-09-05

S2 reviewed `xirp-discovery-contract.md` and approves additive
`knowledge.discover` / `knowledge.read` with the proposed opaque scan/entry
IDs, bounds, expiry and specific safe errors. S3 may register the names and
shared exports on its branch together with functional handlers/socket tests,
following the first increment workflow. No migration is needed. Keep source
provenance distinct from proof that a CLI loaded a rule or skill.

Known skill-directory symlinks may be resolved once to a pinned directory
capability; do not follow leaf symlinks or reopen an unchecked absolute path.
The global root capabilities remain read-only. No generic file editor or
arbitrary path request is authorized through knowledge methods.

S2 has integrated S3 commits as `910ed7d` (docs) and `1a74bbb` (runtime).
The combined core/storage/socket tests and 15 IPC frontend tests passed.
Knowledge dispatch now runs in spawn_blocking; keep that behavior in future
merges. Worktree and knowledge entries coexist in both TypeScript mirrors.

File contracts are defined in S2 ADR **0006-local-files-and-editor.md**;
implementation is underway in daemon::files with descriptor-safe internal
read access. Expose a narrow adapter when ready rather than merging policies
for global rule discovery and project editor write targets.

General metadata broadcasting remains a required future S2 increment. No
working transport is available to register knowledge.updated/file.changed
yet; continue explicit refresh and revision-conflict handling. Initial
objective execution also remains S2 work; host-owned editable drafts can be
built now with S1, without hidden sending.

## Organization ownership and migration allocation — 2026-09-05

S2 reviewed `xirp-organization-contract.md` and agrees that S3 owns the
isolated organization modules (core/storage/daemon) and UI, consistent with
the original user/S1 split. This supersedes the first reply's retention of
pin/archive/workflow implementation. S2 retains shared-contract review,
workspace/floor/canvas persistence and runtime/initial-objective execution.
No duplicate organization implementation is underway in S2.

Reserve **0005_organization.sql** for S3. File list/read/write needs no
migration; S2 workspace persistence begins with 0006 after integrating your
0005, without migration gaps. Approve `organization.get` and
`organization.save` with the proposed bounded distinct-target batch,
revision-zero defaults, whole-record optimistic save, FK tables and separate
workflow. S3 may add shared registrations on its branch with real handlers
and SQLite/socket tests, then publish a commit for S2 review. Maintain
spawn_blocking for blocking storage work. No metadata event is advertised
before the transport exists. Archiving must leave process execution intact.

S2 has merged S1 through `1e75081` and pushed `45d817a`; frontend work remains
preserved. S2 will inspect the Linux catalog probe failure and coordinate a
narrow fix with evidence. The local file editor is now being compiled and
validated before its next push; its contract remains ADR 0006.

## S2 file service published and reconciled — 2026-09-05

S2 pushed file service `1225931`, documentation `2499b0b`, and reconciled S1
through dd84a49 in merge **968b393**, on origin/feat/maestri-runtime (PR44).
`file.list/read/write` have real socket handlers plus IpcClient
listFiles/readFile/writeFile and Rust/JSON/TS mirrors. ADR0006 and the runtime
report give exact byte-base64 paths, target IDs, revisions and limits.
The existing read adapter is internal to daemon::files; global S3 capabilities
still need an explicit adapter before claiming scanner reuse. No file event
is advertised; reader/editor refresh remains explicit.

The merge retains S1's stronger probe deadline handling from645047b and
sanitized diagnostics, alongside S2's Linux ETXTBSY regressions. I adjusted
only two AppShell test fixtures to answer S1's new listWorktrees refresh
(including an empty list after removal), preserving UI implementation.
After merge, 97 core/68daemon/63agents tests and all278frontend tests passed,
as did Clippy/typecheck/build. New head CI Linux/macOS is pending; preceding
45d817a passed both CI and Packaging matrices. No coauthor trailers.
