# Playwright testing

Playwright covers the desktop UI that actually exists. It does not fake a
session grid, and it does not add production hooks so a browser test can pretend
the daemon is connected.

## What runs in CI

`apps/desktop/e2e/empty-shell.spec.ts` starts the Vite production preview and
drives the current `AppShell` with role locators:

- New Session stays disabled until a project exists
- Add Project explains that the local daemon is required
- the empty workspace copy is honest
- the skip link is first in tab order

Those tests wait on Playwright's auto-waiting assertions. They do not use
`page.waitForTimeout`.

Chromium is the CI browser on both Ubuntu and macOS. The same spec file runs on
both operating systems.

## What does not run against this UI yet

The Beta acceptance flow needs two live terminal tiles, worktree isolation,
resize, stop, window close/reopen, dirty worktree protection, and daemon
restart. React still renders `WorkspaceEmptyState`. Wiring a dummy grid only
for Playwright would test a page that users never see.

Those criteria are implemented in `crates/e2e` against `SessionManager`,
`cli-master-git`, `cli-master-storage`, and `Daemon::bind`. The skipped spec
`apps/desktop/e2e/beta-acceptance.spec.ts` points at those tests. Set
`CLI_MASTER_TAURI_E2E=1` when a real Tauri window can host two live tiles, then
fill that file in with window-level actions instead of skipping it.

## Commands

From `apps/desktop`:

```bash
pnpm test:e2e
```

That script builds the frontend, starts `vite preview` on port 4173, and runs
Playwright. Install the browser once with:

```bash
pnpm exec playwright install --with-deps chromium
```

Do not point these tests at `tauri dev` until the grid exists. Browser-only
Vite cannot spawn PTYs, and the current shell does not claim that it can.
