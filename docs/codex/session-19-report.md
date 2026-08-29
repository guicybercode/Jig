# Session 19 report: Beta E2E acceptance

## Diagnosis

Beta acceptance was specified as a user-visible flow: add a repo, run two
isolated agent sessions, treat the window as disposable, refuse dirty
worktree deletes, and survive daemon restart without murdering leftover
PIDs.

The PTY manager, Git worktree safety, SQLite, and daemon bind were already
on `main`. The React shell was not. It still renders an empty workspace.
A Playwright suite that clicked a grid would have been testing fiction, or
it would have needed a test-only UI.

`Daemon::bind` also skipped the recovery step ADR 0003 already described.
Live rows from a previous instance would have stayed `running` with a stale
PID.

## Decisions

- Put the real acceptance tests in `crates/e2e` and drive production crates.
  Two subscriptions are the grid. Dropping them is closing the window.
  Resubscribe plus snapshot is reopen.
- Ship `cli-master-fake-agent` as an interactive stand-in. Fragmented
  writes, Ctrl+C, exit codes, and `--hold` are protocol features, not
  production knobs.
- Run Playwright against the empty shell that users actually see. Skip the
  Tauri grid spec until `CLI_MASTER_TAURI_E2E=1` can point at a real window.
- Call `recover_stale_sessions_for_daemon` during `Daemon::bind`. That is
  product behavior, not a test hook.
- Linux and macOS share the same tests. CI already has both runners.

## Files

- `crates/fake-agent/**`
- `crates/e2e/**`
- `crates/daemon/src/server.rs`
- `apps/desktop/e2e/**`, `apps/desktop/playwright.config.ts`
- `docs/playwright-testing.md`, `docs/test-coverage-review.md`
- CI, `AGENTS.md`, `README.md`

## Commands

```bash
cargo fmt --all
cargo test -p cli-master-fake-agent -p cli-master-e2e -p cli-master-daemon --locked
pnpm --filter @cli-master/desktop test:e2e
```
