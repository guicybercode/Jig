# Test coverage review

This is a map of Beta v0.1 acceptance criteria to the tests that actually
exercise them. Unit tests in individual crates still matter. They are not
repeated here unless they are the only coverage for a criterion.

## Runtime acceptance (`crates/e2e`)

| Criterion | Test | How it waits |
|---|---|---|
| Add a local Git repository | `adds_a_local_repository_and_runs_two_grid_sessions` | `Git::inspect_repository` plus SQLite `list_projects` |
| Two sessions on different worktrees and branches | same | `SessionWorktreeSaga<SessionManager>` creates and persists both isolated sessions |
| Open both in a grid and type into both | same | two subscriptions; distinct `ack:` lines |
| Resize a tile | same | `session.resize` then the fake agent's `size` command |
| Stop one without affecting the other | same | `wait_status(Exited)` plus a later `ack:` on the survivor |
| Close and reopen the UI | same | drop both subscriptions, resubscribe, snapshot contains prior bytes |
| Recover metadata | same | SQLite rows are created by the production saga and remain queryable after subscription reconnect |
| Dirty worktree protection | `dirty_worktree_never_receives_a_removal_token` | the saga returns `Blocked` with `UntrackedFiles`; no token exists |
| Daemon restart to `unknown` | `daemon_restart_marks_stale_sessions_unknown_without_signaling_the_pid` | bind a new daemon, reload the row, probe the guarded PID through `rustix` |
| Exit codes and failures | `fake_agent_exit_codes_are_captured_by_the_session_manager` | `wait_status` on `exited` / `failed` |
| Linux and macOS | CI `ubuntu-24.04` and `macos-15` | real OS jobs, not a stub |

The fake agent is `crates/fake-agent`. It is interactive, echoes commands as
`ack:` lines, flushes output one byte at a time, prints
`FAKE_AGENT_INTERRUPT` on Ctrl+C without exiting, supports `exit N` and
`fail`, and `--hold` parks after stdin EOF so a UI disconnect can be simulated
without a live PTY master. It never prints environment values.

Readiness is always an observable: `FAKE_AGENT_READY`, `ack:…`, session status,
SQLite rows, Git blockers, `FAKE_AGENT_HOLDING`, or a process probe. PTY waits
use Tokio deadlines; binary protocol tests read through a bounded channel so a
blocking `read_line` cannot defeat the timeout. Child guards terminate and reap
processes during panic unwinding.

## Desktop UI

| Criterion | Coverage |
|---|---|
| Empty shell, disabled actions, skip link | Vitest `AppShell.test.tsx` and Playwright `empty-shell.spec.ts` |
| Two-tile grid, real Tauri window close, tile resize handles | Not covered at the window layer. No skipped placeholder is counted as coverage; runtime behavior is covered in Rust |

See [playwright-testing.md](playwright-testing.md).

## Intentionally not weakened

- No `#[cfg(test)]` backdoors on `SessionManager`, Git removal, or daemon lock.
- Production `SessionManagerConfig::default` grace periods are unchanged.
- E2E uses `SessionManagerConfig::for_tests`, the same helper as
  `crates/session/tests/pty_lifecycle.rs`.
- Worktree removal still refuses dirty trees. There is no `--force` path.
- Daemon restart clears metadata PIDs and does not signal them.

## Gaps that remain product work

The daemon still does not compose `SessionManager` into its Unix-socket
methods. Closing a real Tauri window therefore cannot be exercised end to end
until that grid and those methods land. `Daemon::bind` does reconcile leftover
live rows to `unknown`, which is the recovery half of that story.
