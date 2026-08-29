# Session 08 — safety, redaction, and diagnostics integration

Original branch: `cursor/08-safety-diagnostics-2d64`.

The original change predated the daemon, session, Git, storage, and consolidated
desktop architecture. This integration kept its unique defense-in-depth work
and replaced parallel implementations with the current owners.

## Integrated behavior

| Concern | Integrated owner and behavior |
|---|---|
| Command launch | Existing `CommandSpec`, agent adapters, Git runner, and `SessionManager`; executable and argument arrays remain structured |
| Git safety | Existing modular `crates/git`; removal is clean-only, re-inspected, exact-state checked, and never passes `--force` |
| Confirmation | Existing Beta v1 `worktree.prepare_remove` / `worktree.remove` contract; blocked responses cannot carry a token and bypass fields are rejected |
| Redaction | Pure `crates/core::redact` helpers from the hardened PR, applied at every `ApiError` constructor/detail boundary |
| Diagnostic issues | Existing bounded daemon ring, now defensively redacting code, message, and action before logging or retention |
| Diagnostic export | Existing `diagnostics.get` daemon method now returns backend-generated recursively redacted JSON with home-relative paths |
| Desktop UI | Diagnostics dialog uses the consolidated project-owned IPC client and shared accessible dialog; it copies only backend `exportText` |
| Tauri | Existing generic daemon bridge only; no diagnostics or worktree domain command was added |
| Documentation | Threat model rewritten against the integrated architecture and current accepted risks |

## Superseded code removed

- the parallel `crates/safety` destructive-operation and IPC framework
- the monolithic PR-era Git service
- `allowDirty`, dirty-force confirmation, and `git worktree remove --force`
- direct `diagnostics_get` / `diagnostics_export` Tauri commands
- the PR-era dirty-removal dialog and checkbox
- duplicate diagnostics DTOs that read `HOME` from `crates/core`
- unused global timeout/limit constants disconnected from current component owners

These removals are intentional. Keeping them would create two authorities for
Git, process lifecycle, IPC validation, and confirmation state.

## Hardening retained from commit `2c78ee4`

- secret-name recognition avoids the `AUTHOR`/`AUTH` false positive
- labelled values, spaced assignments, authorization and cookie headers,
  private-key blocks, common provider token prefixes, AWS keys, and JWT-like
  values are redacted
- JSON details are recursively sanitized by key and content
- loader failures do not render external error text
- clipboard export never falls back to raw on-screen data
- home prefixes are replaced by the daemon, keeping core free of environment
  and filesystem I/O

## Verification scope

Rust coverage includes redaction patterns, recursive API-error sanitization,
diagnostic issue sanitization, export sanitization, command debug projections,
Git clean-only removal, state reinspection, event replay, and recovery.

Frontend coverage includes typed `diagnostics.get` routing through the mock IPC
client, accessible dialog behavior, backend-export-only clipboard use, no raw
fallback, and secret-free loader errors.

## Residual risks

- redaction is pattern-based and cannot identify arbitrary unlabelled secrets
- exact local paths remain visible to the local user in the on-screen snapshot
- the daemon worktree handler must preserve exact-state token binding when its
  currently declared wire methods are fully composed
- PTY reattachment after daemon death remains outside the Beta scope

CLI Master remains an orchestrator, not a sandbox. A user-selected executable
can intentionally perform any action available to that OS account.
