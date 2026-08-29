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

## What is intentionally not represented as a browser test

The full Beta flow needs a real Tauri window connected to the private daemon:
two live terminal tiles, worktree isolation, resize, stop, window close/reopen,
dirty worktree protection, and daemon restart. A Vite browser preview cannot
spawn PTYs or exercise the Tauri sidecar, so wiring a browser-only fake would
test a path users never run.

The runtime portions are implemented in `crates/e2e` against the production
`SessionWorktreeSaga<SessionManager>`, Git, SQLite, and `Daemon::bind`. Dropping
and recreating PTY subscriptions verifies replay semantics, but it is not
described as a real window test. There are no placeholder or unconditionally
skipped Playwright cases: every test listed by Playwright executes.

Add a separate Tauri-driver suite when a real window can host the live grid.
That future suite must perform window-level actions and must fail when its
harness is unavailable instead of silently skipping the acceptance criteria.

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
