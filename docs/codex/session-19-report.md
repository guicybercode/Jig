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

During reconciliation, `main` already owned daemon startup recovery through
`Storage::reconcile_sessions` and its current `EventBus` composition. The old
parallel recovery helper from this PR was discarded.

## Decisions

- Put runtime acceptance in `crates/e2e` and create both isolated sessions
  through the production `SessionWorktreeSaga<SessionManager>`. Two
  subscriptions model independent consumers; dropping and recreating them
  validates replay, not a literal window lifecycle.
- Ship `cli-master-fake-agent` as an interactive stand-in. Fragmented
  writes, Ctrl+C, exit codes, and `--hold` are protocol features, not
  production knobs.
- Run Playwright against the disconnected shell users actually see. Do not
  count empty or unconditionally skipped Tauri cases as acceptance coverage.
- Keep daemon recovery, event fanout, and Git inspection exactly as composed
  on `main`; test restart through `Daemon::bind` without adding a second path.
- Resolve the fake-agent binary from explicit Cargo/target locations without
  `PATH` or build polling. Bound protocol reads and reap every child with RAII.
- Linux and macOS share the same tests. CI already has both runners.

## Integration provenance

The dedicated worktree already contained two uncommitted frontend config
edits before reconciliation: Vitest exclusion for `e2e/**` and an import-only
reorder in `vite.config.ts`. Both were preserved. The Vitest exclusion is a
required fix and is validated by `pnpm check`; the import reorder is behavior
neutral and is reported separately rather than attributed to the E2E work.

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
