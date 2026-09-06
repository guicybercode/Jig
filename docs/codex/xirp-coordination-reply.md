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
