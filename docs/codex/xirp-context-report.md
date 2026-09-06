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
Draft [PR 45](https://github.com/guicybercode/Jig/pull/45) targets the canvas
branch to run Linux/macOS CI. Actual Tauri/canvas consumption and full parity
remain unverified; CI state must be checked rather than inferred from push.

## Commits / next integration

- `569449f`: source inventory and integration proposal; pushed.
- `2f3b7c8`: persisted knowledge contract/SQLite/daemon/typed IPC; pushed.
- `a757f05`: isolated `KnowledgePanel`, 12 behavior tests and visual evidence; pushed. Props:
  `{client,currentProject,onInsert,insertDisabledReason}`; insertion receives
  `{sourceId,kind,title,body}` and must append to an editable composer draft.
  Keep panel mounted if unsaved editor drafts must survive closing its surface.
- Next: bounded rules/skills discovery; consume S2 pin/archive/workflow and
  initial-objective contracts; mount with S1; verify additional agent features
  only against trustworthy native sources. Portal/MCP remain explicit separate
  dependencies; no connection is simulated.


## Canvas integration and revision provenance checkpoint

Published `cfc79ba` merges S1 canvas through `6f6af26`, preserving both owners'
coordination notes. The merged frontend check passed TypeScript, all 137 tests
and production build without the earlier NODE_OPTIONS workaround; all 39 daemon
tests passed. S1 subsequently began the per-terminal prompt composer and will
mount the library while retaining hidden editor drafts.

Published `b7b0b4e` adds sourceRevision to KnowledgeInsertion. SourceId/revision
identify the editor's base snapshot; title/body are the actual draft and can
include unsaved edits. Both provenance fields are null for a never-saved draft.
Fourteen library tests, typecheck and targeted ESLint passed. Insertion remains
explicit draft composition; S1 owns delivery through its terminal input queue.

For b7b0b4e, macOS Quality and Packaging passed. Linux Quality failed in the
existing adapter catalog test `detect_and_set_enabled_do_not_require_real_clis`
with a version-probe spawn failure; frontend, Playwright and clippy passed first.
The runtime owner received the exact log and is investigating. Linux acceptance
is not complete: https://github.com/guicybercode/Jig/actions/runs/34002926329

The concrete next organization contract is
[xirp-organization-contract.md](xirp-organization-contract.md). S1 supports S3's
bounded modules; S2 has now delegated their implementation and allocated
0005_organization.sql. S3 owns those modules, with S2 retaining runtime and
initial-objective execution. Implementation continues toward the original
objective.


## Rule/skill discovery increment

Backend and typed IPC published as `4a367d0`. Approved knowledge.discover/read
inventory known root/global/admin rule and skill locations. They accept registered
project IDs and opaque expiring selectors, never caller paths. Reads revalidate
file/directory identity and nanosecond mtime/ctime, stay <=64 KiB and never execute
content. Skill-directory links bind the captured target. See the precise bounds,
source policy and remaining nested-scope gaps in
[xirp-discovery-contract.md](xirp-discovery-contract.md).

The isolated KnowledgeSourceInspector consumes discoverKnowledge/readKnowledge
from the existing IpcClient. Props are client, optional currentProject and
connectionKey. S1 should pass daemon instance/reconnect generation. Lists show
path/provider/scope/precedence, linked origin, availability and incomplete-scan
issues. Source previews are plain text. Correlation and full source-descriptor
checks prevent displaying another source's content; request epochs discard stale
responses. It neither edits sources nor sends terminal input.

Verification on macOS: full core/daemon 146 tests passed; a subsequent independent
QA review found an enumeration-budget edge case that discarded already collected
candidates. It was fixed with a real-directory regression; the final 21 knowledge
daemon tests passed, including both socket tests. Final core/daemon all-target
clippy and formatting passed. Full frontend check passed TypeScript, 162 tests
and Vite build. Targeted inspector/IPC/library 43 tests and ESLint passed.

Chromium screenshots use an isolated mock fixture at 1100px and 375px, not a claim
of canvas integration: [desktop](artifacts/xirp/knowledge-inspector-desktop.png),
[narrow preview with keyboard focus](artifacts/xirp/knowledge-inspector-narrow.png).
Both had no horizontal viewport overflow. Temporary preview files/server removed.
S1 mounting, real desktop smoke and this new increment's Linux/macOS CI remain
separate integration evidence.
