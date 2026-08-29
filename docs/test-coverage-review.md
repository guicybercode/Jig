# Test coverage review

This is a map of Beta v0.1 acceptance criteria to the tests that actually
exercise them. Unit tests in individual crates still matter. They are not
repeated here unless they are the only coverage for a criterion.

## Runtime acceptance (`crates/e2e`)

| Criterion | Test | How it waits |
|---|---|---|
| Add a local Git repository | `adds_a_local_repository_and_runs_two_grid_sessions` | `Git::inspect_repository` plus SQLite `list_projects` |
| Two sessions on different worktrees and branches | same | `Git::create_worktree` twice, then two `SessionManager` cwds |
| Open both in a grid and type into both | same | two subscriptions; distinct `ack:` lines |
| Resize a tile | same | `session.resize` then the fake agent's `size` command |
| Stop one without affecting the other | same | `wait_status(Exited)` plus a later `ack:` on the survivor |
| Close and reopen the UI | same | drop both subscriptions, resubscribe, snapshot contains prior bytes |
| Recover metadata | same | SQLite still has the project, sessions, and worktree rows |
| Dirty worktree protection | `dirty_worktree_cannot_be_removed` | `prepare_remove` blockers and `remove_worktree` error |
| Daemon restart to `unknown` | `daemon_restart_converts_stale_live_sessions_to_unknown_without_killing_the_pid` | bind a new daemon, reload the row, `kill -0` on the leftover PID |
| Exit codes and failures | `fake_agent_exit_codes_are_captured_by_the_session_manager` | `wait_status` on `exited` / `failed` |
| Linux and macOS | CI `ubuntu-24.04` and `macos-15` | real OS jobs, not a stub |

The fake agent is `crates/fake-agent`. It is interactive, echoes commands as
`ack:` lines, flushes output one byte at a time, prints
`FAKE_AGENT_INTERRUPT` on Ctrl+C without exiting, supports `exit N` and
`fail`, and `--hold` parks after stdin EOF so a UI disconnect can be simulated
without a live PTY master. It never prints environment values.

Readiness is always an observable: `FAKE_AGENT_READY`, `ack:…`, session status,
SQLite rows, Git errors, `FAKE_AGENT_HOLDING`, or `kill -0`. Tests poll those
conditions every 5ms up to a deadline. They do not `sleep(3)` and hope.

## Desktop UI

| Criterion | Coverage |
|---|---|
| Empty shell, disabled actions, skip link | Vitest `AppShell.test.tsx` and Playwright `empty-shell.spec.ts` |
| Two-tile grid, window close, tile resize handles | Not present in the UI on this branch. Tracked by skipped Playwright tests that name the Rust e2e owners |

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
methods. Closing a real Tauri window therefore cannot be Playwright-tested
end to end until that grid and those methods land. `Daemon::bind` does now
reconcile leftover live rows to `unknown`, which is the recovery half of that
story.
