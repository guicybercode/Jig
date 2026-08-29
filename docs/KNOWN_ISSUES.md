# Known issues (Beta v0.1.0-beta.1)

These are observed limitations, not a backlog of new product ideas.

## Terminal host is a textarea

The desktop grid writes PTY output into a focused textarea and forwards typed
keys plus Enter as raw bytes. It is interactive and independent of React
session metadata, but it is not xterm.js. Resize is sent only when the daemon
creates the session (80x24) unless a later caller invokes `session.resize`.
Copy/paste, CSI mouse, and accurate column wrapping are incomplete.

## Live output is polled

`session.subscribe` returns a replay snapshot over request/response. The UI
polls about every 80ms. High-rate output can feel lagged. There is no
push event stream from `cli-masterd` to the webview yet.

## Folder picker is a path field

Add Project asks for an absolute path. There is no native directory dialog in
this beta, because the extra GTK dialog plugin was not required for the
daemon contract.

## macOS was not exercised on this integrator machine

This release branch was built and tested on Linux. macOS coverage is the CI
`Quality (macos-15)` job plus whatever a macOS developer runs locally. App
bundle and DMG generation were not produced here.

## Linux packages were built, not launched

`pnpm tauri build` on Linux produced an AppImage and a `.deb` that include
`cli-masterd` next to `cli-master-desktop`. Those artifacts were inspected
(`dpkg-deb -c`, AppDir `usr/bin`) and were not installed or opened as a GUI.

## Daemon crash cannot reattach PTYs

If `cli-masterd` exits, live PTY masters are gone. Restart marks previously
running rows `unknown` and keeps metadata. Restart the session to get a new
process.

## Built-in agents need to be installed

Codex, Claude Code, Gemini CLI, and OpenCode are detected from PATH. The app
starts without them. Session create fails with `AGENT_EXECUTABLE_NOT_FOUND`
until the CLI exists or a custom executable is registered.

## Git must be on PATH

`GitService` looks at `CLI_MASTER_GIT`, then PATH, then common Unix locations.
If Git is missing, daemon open fails with `GIT_NOT_FOUND`.

## Parallel session reports

Earlier planning sessions described architecture, PTY, storage, worktrees,
adapters, UI, grid, security, and CI as separate reports under
`docs/codex/session-*-report.md`. Those files were not present on `main` when
this integrator started. Decisions live in `ARCHITECTURE.md`. This cut is
`docs/codex/session-10-report.md`.
