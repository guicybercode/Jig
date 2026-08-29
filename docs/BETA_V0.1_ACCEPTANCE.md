# Beta v0.1 acceptance

Version under test: `0.1.0-beta.1`. Integrator host: Linux `x86_64`
(kernel 6.12.94+, rustc 1.98.0, Node 22.14.0, pnpm 11.9.0, git 2.43.0).
macOS was not available on this machine.

Status values are `passed`, `failed`, `partial`, or `not tested`. Partial is
not treated as passed.

## Primary scenario (28 steps)

| # | Requirement | Status | Platform | Evidence | Notes |
| --- | --- | --- | --- | --- | --- |
| 1 | Open the app | not tested | linux GUI | none | `pnpm tauri dev` was not driven as a user session. Window code exists. |
| 2 | Add a Git repository | passed | linux daemon | `project_add_exposes_name_path_and_branch` | GUI uses a path field, not a folder dialog. |
| 3 | See name, path, and branch | passed | linux daemon + UI unit | same test; `AppShell` connected snapshot test | |
| 4 | Detect installed agents | passed | linux daemon | `hello_and_snapshot_report_beta_identity` lists builtins; detect flag from PATH | Real Codex/Claude/Gemini/OpenCode were not required. |
| 5 | Register fake or custom agent | passed | linux daemon | `custom_agent_and_working_tree_session_round_trip` | |
| 6 | Create a session in the current working tree | passed | linux daemon | same test, `createWorktree: false` | |
| 7 | See an interactive terminal | partial | linux daemon + UI | PTY write/read passed; UI is a textarea host | Not xterm.js. GUI not exercised. |
| 8 | Send input | passed | linux daemon | writes `alpha` to the PTY | |
| 9 | Receive output | passed | linux daemon | replay contained `alpha` | |
| 10 | Resize the terminal | passed | linux daemon | `session.resize` to 40x12 after start | UI does not send live resize yet. |
| 11 | Create a second session | passed | linux daemon | `stopping_one_session_does_not_stop_the_other` | |
| 12 | Show both in a grid | partial | linux UI code | CSS `terminal-grid--2` plus `visibleSessionIds` cap of 4 | No GUI run. |
| 13 | Focus each terminal in turn | partial | linux UI code | click/focus handlers on the textarea | No GUI run. |
| 14 | Stop one without affecting the other | passed | linux daemon | stop first; second still `running` | |
| 15 | Create a session with a new worktree | passed | linux daemon | `dirty_worktree_removal_is_blocked_until_confirmed` | |
| 16 | Confirm branch and path | passed | linux daemon | branch starts with `agent/`; path under managed worktrees | |
| 17 | Change a file in the worktree | passed | linux daemon | wrote `README.md` | |
| 18 | See Git status | passed | linux daemon | `isDirty` true | |
| 19 | See diff | passed | linux daemon | diff text contained `dirty change` | |
| 20 | Try to remove a dirty worktree | passed | linux daemon | `worktree.remove` with `allowDirty: false` | While the session was live this returned `WORKTREE_IN_USE`; after stop, `WORKTREE_DIRTY`. Both block removal. |
| 21 | Confirm removal was blocked | passed | linux daemon | codes `WORKTREE_IN_USE` then `WORKTREE_DIRTY` | |
| 22 | End the session | passed | linux daemon | `session.stop` then `session.delete` | |
| 23 | Close and reopen the interface | passed | linux daemon | drop `Daemon`, `Daemon::open` on the same paths | GUI reload was not run. Process lifetime is the daemon, not React. |
| 24 | Confirm metadata persistence | passed | linux daemon | snapshot still had project name Demo | |
| 25 | Confirm missing processes are reconciled | passed | linux daemon | restored session status `unknown`; `live_count == 0` | |
| 26 | Remove a clean worktree | passed | linux daemon | `project_remove_does_not_delete_the_repository` | |
| 27 | Remove the project from the app only | passed | linux daemon | `project.remove` after sessions/worktrees gone | |
| 28 | Confirm the repository still exists on disk | passed | linux daemon | `README.md` still present | |

## Failure paths

| Requirement | Status | Platform | Evidence | Notes |
| --- | --- | --- | --- | --- |
| Git not installed | partial | linux | `GitError::NotFound` mapping in daemon; `GitService::from_environment` | Not forced on this host because `/usr/bin/git` exists. |
| Agent not found | passed | linux daemon | `failure_paths_return_actionable_codes` / `AGENT_INVALID` | |
| Invalid custom executable | passed | linux daemon | relative `../bin/sh` -> `AGENT_INVALID`; missing absolute path -> `AGENT_EXECUTABLE_NOT_FOUND` | |
| Project moved | passed | linux daemon | `moved_project_is_reported_when_starting_a_session` | |
| Directory without permission | not tested | linux | none | Would need a non-root chmod fixture. |
| Existing branch | passed | linux git crate | `existing_branch_is_rejected` | Daemon suffixes branches so collisions are rare. |
| Existing worktree directory | passed | linux git crate | `existing_worktree_directory_is_rejected` | |
| Process exited unexpectedly | passed | linux daemon | `unexpected_process_exit_is_observed` with `/bin/true` | |
| Frontend reloaded | partial | architecture + daemon persist | metadata survives daemon reopen | GUI reload not driven. |
| Daemon disconnected | passed | linux UI unit | empty shell shows Daemon unavailable | |
| Database unavailable | partial | storage crate | open/migrate errors map to `DATABASE_UNAVAILABLE` | Not injected in daemon acceptance. |
| Very large output | passed | session crate | replay capped at 8 MiB | Not a GUI memory profile. |
| Git command over limit | passed | git crate | 2 MiB diff cap, 4 MiB output cap, 30s timeout | |
| Duplicate stop | passed | linux daemon | `SESSION_NOT_RUNNING` | |
| Duplicate start | passed | linux daemon | `SESSION_ALREADY_RUNNING` on restart while live | |

## Performance (qualitative)

| Requirement | Status | Platform | Evidence | Notes |
| --- | --- | --- | --- | --- |
| 10 registered sessions | passed | linux daemon | `ten_registered_sessions_can_be_listed_and_stopped` | Created 10 `/bin/sleep` sessions, listed 10, `live_count >= 4`. No fabricated timings. |
| At least 4 visible terminals | partial | linux UI | `visibleSessionIds` sliced to 4 | Not measured in a window. |
| Multiple outputs | passed | linux daemon | echoer PTY plus independent sessions | |
| Single/grid toggle | partial | linux UI | visibility toggle per session | No GUI run. |
| Switch project | partial | linux UI | `selectProject` | No GUI run. |
| End sessions | passed | linux daemon | stop/kill/drop | |
| Memory not obviously unbounded | not tested | linux | none | No profiler run. Replay is capped. |
| No duplicate listeners | passed | linux daemon | subscribe polling uses replay snapshot, not leaked subscribers | UI poll recreates an interval when workspace identity changes. |

## Builds

| Artifact | Status | Platform | Evidence | Notes |
| --- | --- | --- | --- | --- |
| Linux AppImage | passed | linux | `pnpm tauri build` | File: `target/release/bundle/appimage/CLI Master_0.1.0-beta.1_amd64.AppImage` (76M). Contains `usr/bin/cli-masterd` next to the desktop binary. GUI of the AppImage was not launched. |
| Linux `.deb` | passed | linux | `pnpm tauri build` | File: `target/release/bundle/deb/CLI Master_0.1.0-beta.1_amd64.deb` (5.0M). `dpkg-deb -c` lists `/usr/bin/cli-masterd` and `/usr/bin/cli-master-desktop`. Package was not installed. |
| Linux `.rpm` | passed | linux | `pnpm tauri build` | Extra byproduct of `bundle.targets = "all"`. Not a stated v0.1 requirement. |
| macOS `.app` | not tested | macos | none | No macOS host. Use CI or a Mac: `pnpm tauri build`. |
| macOS `.dmg` | not tested | macos | none | No macOS host. |

Commands if a builder is available:

```bash
bash apps/desktop/src-tauri/scripts/stage-daemon.sh release
pnpm tauri build
```

macOS:

```bash
pnpm tauri build
```

## Gate commands recorded on Linux

Run on 2026-08-29 on this integrator host. Later failures must not be edited
to passed.

| Command | Result |
| --- | --- |
| `pnpm install --frozen-lockfile` | passed. Lockfile up to date. |
| `pnpm --filter @cli-master/desktop check` | passed. `tsc --noEmit`; vitest 4 passed; Vite production build. |
| `cargo fmt --all -- --check` | passed. Exit 0. |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | passed. Finished with no clippy errors. |
| `cargo test --workspace --locked --offline` | passed. 0 failed. Daemon acceptance: 12 passed. Session: 5 passed. Git: 5 passed including preexisting directory rejection. |
| `RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked` | passed. |
| `pnpm tauri build` | passed. AppImage, `.deb`, and `.rpm` written under `target/release/bundle/`. Sidecar `cli-masterd` present in AppImage and deb. |
| Local macOS gate | not tested. No macOS host. |
| CI `Quality (macos-15)` | not tested on this machine. Recorded after the pull request runs. |
| `pnpm tauri dev` GUI 28-step walkthrough | not tested. |
