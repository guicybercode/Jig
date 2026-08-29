# Session 03 report — durable metadata and startup recovery

This change hardens the existing SQLite storage implementation without
creating a second domain or IPC contract. `crates/core/src/wire` remains the
authoritative Beta protocol, and the six public session statuses remain
`starting`, `running`, `idle`, `exited`, `failed`, and `unknown`.

## Schema history

The merged migration sequence is immutable:

1. `0001_initial.sql`
2. `0002_worktree_dirty_state.sql`
3. `0003_recovery_metadata.sql`

Migration 3 adds optional session association/history columns and recovery
indexes. It does not rewrite the first two migrations, remove the worktree
dirty flag, or replace the cross-project worktree triggers. New lifecycle
timestamp columns use SQLite `INTEGER` affinity and carry Unix epoch
milliseconds. Existing timestamp columns continue accepting legacy RFC 3339
rows while current writes use epoch-millisecond values.

Migration startup validates both version and immutable migration name. This
prevents an older development database that used version 2 for a different
migration from being silently treated as compatible. Required tables, columns,
indexes, and triggers are verified after migration. The storage layer has a
SQLite backup primitive and invokes it before any future migration marked as
destructive; all three current migrations are additive.

## Durability and concurrency

One daemon owns one `rusqlite::Connection` behind a mutex. This makes shared
storage access explicit and serializes writes without introducing a second
connection pool. File-backed databases use:

- foreign keys enabled on every connection;
- a five-second busy timeout;
- WAL journal mode;
- `synchronous=FULL` because writes are metadata transitions, not PTY output;
- an explicit truncating checkpoint through `Storage::close` and a passive
  best-effort checkpoint on drop.

`Storage::transaction` uses an immediate transaction and commits only after the
whole closure succeeds. `Storage::open_migrated` performs compatibility checks,
optional backup, migration, and schema verification before returning.

Project/worktree paths retain native bytes on Unix and are not canonicalized by
storage. Repository path policy remains owned by the Git/application service
layer. Settings are bounded JSON values with caller-supplied epoch-millisecond
timestamps; secret-like keys are rejected. Agent metadata also rejects likely
full process-environment dumps in addition to token-like keys.

## Recovery contract

`Storage::reconcile_sessions` receives the fresh daemon instance identifier,
the session IDs for which the in-memory manager owns live PTYs, and an explicit
epoch-millisecond timestamp. It runs in one immediate transaction before the
daemon binds its socket.

- A session in the live in-memory index keeps its runtime state.
- `exited`, `failed`, and already `unknown` rows remain unchanged.
- A process-bearing row absent from the live index becomes `unknown`.
- Recovered rows clear `runtime_pid` and `daemon_instance_id`.
- No process is spawned, reattached, or signaled during reconciliation.

Persisted PIDs are therefore diagnostic history only until reconciliation and
are never converted into a signal target. Process-group signaling remains
exclusive to the in-memory session manager, which only acts on handles it
created during the current daemon lifetime.

## Coverage

Tests cover migration 1→2→3, migration-name collisions, preservation of dirty
worktree state and triggers, FULL/WAL configuration, concurrent writes,
transaction rollback, settings validation, readable backups, sanitized IPC
errors, stale-PID clearing, same-daemon missing-process recovery, and daemon
startup reconciliation before accepting clients.
