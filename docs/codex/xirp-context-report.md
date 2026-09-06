# XIRP local workflows report

Branch: `feat/xirp-local-workflows`.
Worktree: `/Users/eguimacs/cli-master-xirp`.
Base: `0ac8dd7`, fetched from `origin/refactor/canvas-only-shell` on 2026-09-05.

## Scope and evidence

The source/acceptance inventory is [xirp-local-parity.md](../xirp-local-parity.md).
The architecture proposal is [ADR 0005](../adr/0005-local-knowledge.md).
The full user request remains open; documentation and isolated components are
intermediate deliverables.

## Coordination

A concrete proposal was placed in `docs/codex/xirp-integration-request.md` in
S1, S2 and S3 worktrees. S2 acknowledgment is pending for shared contracts,
module registrations, migration allocation and dispatch. S1 acknowledgment is
pending for canvas mounting and composer insertion. No shared code has been
changed by S3 at this point.

Baseline drift reported to owners: active daemon dispatch is `server.rs`;
`client.rs` is uncompiled. Frontend currently has `types.ts`/`schema.ts` rather
than the mirrors named in AGENTS. Runtime registry/adapters need consolidation
by S2; S3 will not introduce another registry or Tauri client.

## Work in progress

- Pure validated knowledge types and contract tests (new module only).
- Isolated saved prompt/context editor/picker with typed callbacks and tests.
- Persisted handlers and integration follow the agreed shared boundaries.

## Commits and checks

Initial documentation commit: source review completed; `git diff --check`.
Implementation tests and runtime/platform evidence will be recorded here as
executed. No Linux/macOS runtime parity is claimed yet.
