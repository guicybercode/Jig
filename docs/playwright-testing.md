# Playwright testing

Playwright covers the disconnected desktop and local canvas that the browser
preview can exercise. Notes, drafts and layout remain available without the
daemon. It does not inject fake project/session state or add production hooks
to pretend the daemon domain API is implemented.

## What runs in CI

The specs start the Vite production preview and drive `AppShell` with role
locators:

- `empty-shell.spec.ts` checks disabled project management, offline notice,
  sidebar navigation and the first keyboard skip link.
- `canvas-interactions.spec.ts` checks group selection/movement/duplication/
  removal, note persistence after reload, metadata search preserving draft
  card DOM identity, shortcuts inside note editors, Gemini draft creation
  and compact canvas geometry at 360 and 640 px.

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
described as a real window test. Browser tests use drafts and do not prove
PTY continuity, Gemini authentication or native packaging. There are no placeholder or unconditionally
skipped Playwright cases: every test listed by Playwright executes.

Add a separate Tauri-driver suite after the daemon domain API can populate the
existing grid in a real window. That future suite must perform window-level
actions and must fail when its harness is unavailable instead of silently
skipping the acceptance criteria.

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

Do not point these tests at `tauri dev` until the domain IPC and a Tauri-driver
harness can populate the existing grid. Browser-only Vite cannot spawn PTYs,
and the disconnected shell does not claim that it can.
