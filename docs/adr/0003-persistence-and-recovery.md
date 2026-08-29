# ADR 0003: Persistence and recovery

## Status

Accepted for Beta v0.1.

## Context

Session metadata must survive desktop and daemon restarts, while PTY masters
and child-process ownership are tied to one daemon lifetime. Persisted runtime
metadata cannot prove that a reused PID still belongs to CLI Master.

## Decision

SQLite is authoritative for durable metadata. `SessionManager` memory is
authoritative for PTY handles, process groups, subscribers, and bounded replay.
Status is persisted on meaningful lifecycle changes, not for each output chunk.

The public and persisted timestamp representation is Unix epoch milliseconds.
Repository adapters also accept compatible legacy SQLite values during schema
upgrade, but emit the current representation.

The frozen public session statuses are `starting`, `running`, `idle`, `exited`,
`failed`, and `unknown`. On daemon start, rows that claim a process-bearing
state from another daemon lifetime are reconciled to `unknown`; the new daemon
never signals a PID it did not spawn. Stop-in-progress is daemon-owned runtime
state and does not add an unversioned wire status.

Closing the UI does not change session status. Client reconnect uses
`state.snapshot`, then `session.subscribe` with an output cursor. Replay
completion and retention gaps are explicit events.

Schema changes after the initial migration are additive numbered files. The
worktree dirty-state addition is migration `0002`, rather than a silent rewrite
of `0001`. Migration `0003` adds optional recovery history and lookup indexes;
it does not broaden the public status or wire contracts.

## Consequences

Restart from `exited`, `failed`, or `unknown` goes through `starting`; it never
jumps directly to `running`. A daemon crash cannot preserve the PTY, and the UI
offers recovery instead of claiming stale ownership.

Terminal scrollback remains bounded and in memory. A desktop reload while the
daemon remains alive can replay retained output; a daemon restart cannot.

## Alternatives considered

- Persist PTY bytes or handles: rejected because handles are process-local and
  unbounded output would turn SQLite into a terminal log.
- Trust stored PIDs after restart: rejected because PID reuse can target an
  unrelated process.
- Add `created` and `stopping` wire states: rejected by the finalized Beta
  contract in favor of daemon-owned transition state and six stable statuses.
