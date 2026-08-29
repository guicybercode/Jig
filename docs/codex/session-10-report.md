# Session 10 report: Beta v0.1 integrator

Role: release captain on `codex/10-beta-release`. Goal: integrate, test, and
document a coherent Beta v0.1 rather than invent a new architecture.

## What was already on main

`main` had architecture, design tokens, a Tauri/React empty shell, `crates/core`
DTOs, SQLite migrations, agent adapters, and Linux/macOS CI. There were no
`docs/codex/session-*-report.md` files and no separate ADR files. Accepted
decisions are in `ARCHITECTURE.md`.

Parallel cloud sessions for PTY, storage, Git, UI, and CI had been planned but
were not landed as those reports. This branch implemented the missing runtime
on top of the contracts that were already in tree.

## Integration work

1. Included `crates/agents` in the Cargo workspace and made `AgentId` a
   validated registry key.
2. Added storage repositories, builtin seed rows, and unknown-session
   reconciliation.
3. Added `GitService` with dirty worktree protection and output caps.
4. Added `SessionManager` (portable-pty) independent of React.
5. Added `cli-masterd` with length-prefixed JSON and the v0.1 method set.
6. Bridged Tauri `daemon_request` to the Unix socket; spawn `cli-masterd` when
   the socket is missing.
7. Wired the desktop workspace: projects, custom agents, sessions, Git,
   terminal grid.
8. Bundled `cli-masterd` as a Tauri `externalBin` sidecar so Linux packages
   can start the daemon without a PATH install.

## Incompatibilities found and fixed

- UUID `AgentId` vs adapter keys `codex` / `claude` / `gemini` / `opencode`.
- Worktree `session_id` FK: the session row must exist before the worktree
  points at it.
- `std::sync::Mutex` is not reentrant. `session.delete` held the storage guard
  across a second `storage()` call and deadlocked.
- `session.subscribe` over request/response was adding a dropped live
  subscriber on every poll. Replay is now a snapshot copy.
- Git timeout path joined `wait_with_output` after `kill -9` without a bound.
- PTY read loop batched output across a blocking `read()`, so interactive
  echo stayed in `pending` until a later chunk. Output is published per read.
- Linux AppImage and `.deb` shipped only `cli-master-desktop`. The bridge
  looks for a sibling `cli-masterd`. Sidecar staging fixes that.

## Tests run on this machine (Linux)

Exact commands and statuses are in
[docs/BETA_V0.1_ACCEPTANCE.md](../BETA_V0.1_ACCEPTANCE.md). Summary:

| Command | Result |
| --- | --- |
| `pnpm install --frozen-lockfile` | passed |
| `pnpm --filter @cli-master/desktop check` | passed (4 vitest, Vite build) |
| `cargo fmt --all -- --check` | passed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | passed |
| `cargo test --workspace --locked --offline` | passed, 0 failed |
| `cargo test -p cli-master-daemon --test acceptance` | 12 passed |
| `RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked` | passed |
| `pnpm tauri build` | AppImage, `.deb`, `.rpm` written; sidecar present |
| macOS local GUI / packages | not run (Linux host) |

## What was not done

- xterm.js (textarea host instead).
- Native folder dialog.
- Push PTY events over the Unix socket.
- Driving the 28-step scenario in a real window.
- Windows.
- Canvas, browser automation, agent-to-agent, cloud, mobile.

## Suggested next three priorities (not implemented here)

1. Replace the textarea host with xterm.js outside React state, including
   resize observers and paste.
2. Stream `session.output` events from the daemon so the UI does not poll.
3. Produce signed Linux AppImage/deb and macOS dmg from CI with a sidecar
   `cli-masterd` next to the app binary.
