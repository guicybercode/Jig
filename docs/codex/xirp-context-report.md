# XIRP local workflows report

Branch: `feat/xirp-local-workflows`.
Worktree: `/Users/eguimacs/cli-master-xirp`.
Base: `0ac8dd7`, fetched from `origin/refactor/canvas-only-shell` on 2026-09-05.

The full user request remains open. Source/acceptance inventory:
[xirp-local-parity.md](../xirp-local-parity.md). Architecture:
[ADR 0005](../adr/0005-local-knowledge.md).

## Coordination and ownership

S2 approved the first knowledge contract and additive shared registrations in
[xirp-coordination-reply.md](xirp-coordination-reply.md), reserving migration
`0004_knowledge_documents.sql`. S2 retains organization/workflow, runtime,
worktrees, file service and conversation capabilities. S1 retains canvas and
composer integration; the request is in [xirp-integration-request.md](xirp-integration-request.md).

The active daemon dispatch is `server.rs`. Knowledge handlers reuse its
existing storage connection and generic request path, with no session/process
operation. The additive `methods.ts`/`domain.ts` mirrors preserve existing
`client.ts`/`types.ts`/`schema.ts`. Merge S2's `worktree.list` mirror entries by
name, rather than replacing whole files.

## Implemented first vertical increment

- `knowledge.list`: nullable project scope, optional kind, literal title/body
  query, exclusive UUIDv7 cursor; `{entries,nextCursor}`. Global query returns
  globals; project query returns its entries plus globals. Unknown projects
  fail. Pages cap at 50 rows and 512 KiB of serialized JSON.
- `knowledge.save`: UUIDv7, prompt/context kind, title (256 UTF-8 bytes), body
  (64 KiB), project scope, revision and epoch-ms timestamps. Create omits
  id/expectedRevision; update requires both. Revision conflicts are atomic.
- `knowledge.delete`: id plus expectedRevision. Stale requests preserve data.
- `knowledge_documents` in existing SQLite, additive v3→v4 migration, schema
  verification and scoped indexes. Project metadata removal cascades its
  knowledge rows; it never deletes project files. Archive remains separate.
- `knowledge_conflict`, `knowledge_not_found`, `project_not_found`,
  `knowledge_revision_exhausted`, `knowledge_corrupt_data` are stable errors.
  Text/queries are excluded from Debug and request error details.
- IpcClient methods `listKnowledge`, `saveKnowledge`, `deleteKnowledge` use
  validated TypeScript mirrors and a shared Rust/TypeScript JSON fixture.

`knowledge.updated` is deliberately not advertised: compiled baseline supports
session-specific streams, and general metadata event transport remains an S2
integration dependency. Clients refresh after mutations and can explicitly
refresh; revision checks protect concurrent edits. Search uses SQLite ASCII
case folding; Unicode text is preserved but non-ASCII case folding is not
claimed. Pagination is per-request consistent, not a multi-request snapshot;
refresh observes inserts/edits made during browsing.

## Verification executed on macOS

Use `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`
for the Rust commands below; debug information was reduced because the host
had limited free disk space.

- `cargo test -p cli-master-core -p cli-master-storage --locked`: existing
  suites and 9 new core contract tests passed.
- `cargo test -p cli-master-storage --test knowledge --locked`: 9 tests passed;
  actual file-backed databases, competing connections, CAS races, reopen,
  v3 migration, FK cleanup, scope moves, literal search, revision exhaustion,
  corrupt content, 57-row pagination and worst-case JSON escaping.
- `cargo test -p cli-master-core --test knowledge_catalog --locked`: passed;
  JSON catalog matches Rust methods/events exactly.
- `cargo test -p cli-master-daemon --locked`: 37 tests passed, including 2 new
  real Unix-socket acceptance tests. Proves scoped CRUD, cross-client conflicts,
  restart persistence, frame limits, safe errors and unchanged session state.
- `cargo clippy -p cli-master-core -p cli-master-storage --all-targets --locked
  -- -D warnings`, plus the equivalent daemon command: passed.
- `cargo fmt --all -- --check`: passed.
- IPC TypeScript tests and typecheck passed. Full frontend run with Node 25
  needs `NODE_OPTIONS=--no-experimental-webstorage` to use jsdom storage;
  CI uses Node 24. Full frontend check passed: 121 tests, TypeScript and Vite build.
- KnowledgePanel: 12 behavior tests, targeted ESLint and Chromium inspection at
  1100px/375px passed. Native labels/focus and no horizontal overflow checked.
  [Desktop capture](artifacts/xirp/knowledge-desktop.png) and
  [narrow editor](artifacts/xirp/knowledge-narrow-editor.png) use an isolated
  mock-client preview, not a claim of integrated Tauri behavior.

Direct typed serde duplicate-field validation is not a promise that duplicate
keys are rejected after the daemon's intermediate JSON Value decode.
Linux CI, actual Tauri/canvas consumption and full parity remain unverified.

## Commits / next integration

- `569449f`: source inventory and integration proposal; pushed.
- `2f3b7c8`: persisted knowledge contract/SQLite/daemon/typed IPC; pushed.
- Isolated `KnowledgePanel` is implemented as the next UI commit. Props:
  `{client,currentProject,onInsert,insertDisabledReason}`; insertion receives
  `{sourceId,kind,title,body}` and must append to an editable composer draft.
  Keep panel mounted if unsaved editor drafts must survive closing its surface.
- Next: bounded rules/skills discovery; consume S2 pin/archive/workflow and
  initial-objective contracts; mount with S1; verify additional agent features
  only against trustworthy native sources. Portal/MCP remain explicit separate
  dependencies; no connection is simulated.
