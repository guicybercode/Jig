# XIRP local workflows report

Branch: `feat/xirp-local-workflows`.
Worktree: `/Users/eguimacs/cli-master-xirp`.
Base: `0ac8dd7`, fetched from `origin/refactor/canvas-only-shell` on 2026-09-05.

The full user request remains open. Source/acceptance inventory:
[xirp-local-parity.md](../xirp-local-parity.md). Architecture:
[ADR 0005](../adr/0005-local-knowledge.md).

## Coordination and ownership

S2 approved the knowledge, discovery and organization contracts and additive
shared registrations in [xirp-coordination-reply.md](xirp-coordination-reply.md),
reserving migrations `0004_knowledge_documents.sql` and `0005_organization.sql`
for S3. The later organization acknowledgment delegates those modules to S3
and supersedes S2's initial retention of pin/archive/workflow implementation.
S2 retains shared-contract review, runtime, worktrees, file service and
conversation capabilities. S1 retains canvas and composer integration; the
request is in [xirp-integration-request.md](xirp-integration-request.md).

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

## First-increment verification executed on macOS

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

## First-increment commits and integration checkpoint

- `569449f`: source inventory and integration proposal; pushed.
- `2f3b7c8`: persisted knowledge contract/SQLite/daemon/typed IPC; pushed.
- `a757f05`: isolated `KnowledgePanel`, 12 behavior tests and visual evidence; pushed. Props:
  `{client,currentProject,onInsert,insertDisabledReason}`; insertion receives
  `{sourceId,kind,title,body}` and must append to an editable composer draft.
  Keep panel mounted if unsaved editor drafts must survive closing its surface.
- Discovery and organization subsequently advanced as recorded below.
  Initial-objective delivery remains an S2 runtime dependency. Canvas acceptance
  and additional agent features require their own integration evidence.
  Portal/MCP remain separate dependencies; no connection is simulated.


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


## Organization increment

Backend+typed IPC published as `fce3982`. Migration 0005 adds project/session
organization tables with cascading metadata FKs. organization.get reads up to
100 distinct registered targets in request order, without persisting defaults.
organization.save compares the whole record's revision and atomically saves
pin/archive flags plus session workflow. Revision 0 means unwritten defaults;
returning to default flags after a save retains the row and positive revision.

OrganizationPanel receives client(getOrganization/saveOrganization), optional
currentProject{id,name}, currentSession{id,name,status}, connectionKey and
onChanged(entry). It preserves per-target drafts and requires explicit Save.
Conflicts preserve choices; Refresh revision displays the latest saved values
before another save. Current daemon status remains visible even when workflow
is Done or archive is selected. S1 should keep the panel mounted while hidden
and refresh canvas sorting/filtering through onChanged.

Verification: all 199 core/storage/daemon tests passed, including 10 real SQLite
organization tests and 2 socket tests. The socket acceptance starts /bin/cat
through SessionManager, changes archive/pin and every workflow value, and
checks the live session DTO stays unchanged. Restart preserves organization;
explicit stop/delete remain separate actions and repository files survive.
Core/storage/daemon all-target clippy, formatting and whitespace passed.
Independent QA found no concrete backend or UI correctness defect; the batch
snapshot stress test remains probabilistic, while production reads explicitly
share one transaction.

Frontend focused 26 tests (panel 9, decoder 6, client 11), typecheck and targeted
ESLint passed. Isolated Chromium fixtures at 1100px/375px showed visible keyboard
focus and no horizontal overflow: [desktop](artifacts/xirp/organization-desktop.png),
[narrow](artifacts/xirp/organization-narrow.png). Temporary preview files/server
removed. Real canvas mounting/filtering and Linux/macOS CI remain integration
work; these screenshots are not native desktop acceptance.

Native initial-objective/options/continuity evidence is recorded in
[xirp-native-capability-evidence.md](xirp-native-capability-evidence.md) and sent
to S2. Installed flags do not prove successful resume/fork; actual adapter/start
wiring and trustworthy conversation IDs remain required.

## Canvas reconciliation checkpoint — 2026-09-05

Reconciled S1 through `fd7f8d5` (including real library/composer mounting in
`1150de6`), retaining source revisions, discovery, organization migration0005,
worktree.list and native browser/runtime changes. The three catalogs match
36 method names. No unresolved merge markers remain.

Combined macOS validation: frontend293 tests, TypeScript/build and12 Chromium
E2E pass. Rust core/agents/daemon tests passed through their complete binaries;
storage45 tests passed separately, and all-target Clippy for core/storage/
agents/session/daemon plus formatting passed. The expanded session PTY suite
had one failure: `dropping_the_manager_cleans_a_live_process_group`; its exact
rerun passed, but the failed run left a live test shell. Runtime/process-tree
code was unchanged by this merge. A cache-publication race is under read-only
review and was reported to S2; this checkpoint does not claim a clean full
backend suite or native/Linux acceptance. No test assertion was weakened.

Canvas library insertion now has real host consumption as an explicit editable
text snapshot; it does not implicitly send/start or retain a live source link.
Discovery inspector and organization mounting/filtering remain S1 follow-ups.

## Cross-platform reconciliation evidence

At published `41f679c`, [CI Linux/macOS](https://github.com/guicybercode/Jig/actions/runs/34005300527)
and [Packaging Linux/macOS](https://github.com/guicybercode/Jig/actions/runs/34005300530)
passed. Both CI jobs ran frontend checks, Chromium E2E, formatting, Clippy,
workspace Rust tests and documentation. This covers the already published
knowledge/discovery/organization implementation and the reconciled composer.
It does not cover subsequent uncommitted nested discovery, native interactive
acceptance, or prove the independently reproduced process-cache race absent.
S1's inspector mounting is separately published in `9329d01`; its merge into
this branch follows the isolated nested scanner increment.

## Nested project source discovery

The existing scanner now inventories supported rule/skill locations in the
registered project's descendants. Root sources precede globals/admin sources,
which precede nested traversal. Relative scope identifies each configuration
owner, including names containing spaces. Ordinary directory links are skipped;
known skill links retain the existing captured-target semantics. Exact provider,
VCS and dependency pruning is visible, and all phases share response, entry,
node and cumulative directory-depth limits. No IPC fields, methods, migration,
Git operation or file-service adapter changed.

Validation on macOS: all **31 knowledge daemon tests** pass, including **9 new
scanner regressions and 1 new real-socket regression**. Tests cover nested
formats/provider identity, exact pruning, root/global priority, cumulative
depth, shared enumeration limits, pinned-directory replacement races and old
capability rejection after replacing a nested parent. Existing safe-reader and
scan-expiry tests remain green. All-target daemon Clippy, formatting and diff
checks pass. Independent review found no remaining concrete defect.

Sources above the registered project and effective session-cwd ancestry remain
unevaluated. Native activation, editing and bundled/plugin sources remain open
as described in the discovery contract. Earlier cross-platform results at
`41f679c` are not evidence for this new scanner increment; its CI follows push.
