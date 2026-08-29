# Session 03 report — durable metadata and startup recovery

This session implemented the SQLite storage layer for Beta v0.1: connection
setup, numbered migrations, repositories, transactions, serialization, and
startup reconciliation. The daemon still does not exist; recovery is exposed as
a typed API that a future `SessionManager` implements through
`LiveSessionIndex`.

## Schema

Authoritative metadata lives in `cli-master-storage`. Tables match
`ARCHITECTURE.md`, with additive recovery columns from migration `0002`.

Custom agents are **not** a second table. They are rows in `agents` with
`source = 'custom'`. Built-ins use `source = 'built_in'`. That matches the
official schema and avoids duplicating identity, foreign keys, and history.

| Table | Role |
|---|---|
| `schema_migrations` | Applied version, name, UTC timestamp |
| `projects` | Registered repositories. Path is unique. Directory is never deleted. |
| `agents` | Built-in and custom definitions. Args are a JSON array. Env is explicit overrides only. |
| `sessions` | Session metadata and persisted status. `runtime_pid` is historical. |
| `worktrees` | Git isolation records. `ON DELETE SET NULL` for session_id. |
| `settings` | Small JSON settings. Token-like keys are rejected. |

### Projects

- `id`, `name`, `path` (unique, absolute), `repository_root`
- `created_at`, `updated_at`, `last_opened_at` (UTC RFC 3339)

Missing or moved directories are reported as `PathStatus::Missing`. Rows are
not deleted.

### Sessions

- `id`, `project_id` (RESTRICT), `agent_id` (RESTRICT), `name`, `cwd`
- `status` in `starting | running | idle | exited | failed | unknown`
- `runtime_pid` (last known PID, never a liveness proof)
- `daemon_instance_id`
- `branch`, `worktree_path`
- `exit_code`, `error_code`
- `created_at`, `updated_at`, `last_activity_at`, `started_at`, `exited_at`

### Custom agents (`agents` where `source = 'custom'`)

- `id`, `name`, `executable`, `args_json`, `env_json`
- `enabled`, `created_at`, `updated_at`
- Built-in executable/args may be upserted; they cannot be mutated through the
  custom-agent API. Disable is allowed. Delete is blocked while sessions exist.

IDs on the wire remain UUIDv7 strings from `cli-master-core`.

## Migrations

Numbered SQL files are embedded in the crate:

1. `0001_initial.sql` — original architecture tables and indexes.
2. `0002_recovery_metadata.sql` — additive columns and indexes for recovery.

Rules:

- Applied in ascending version order.
- Each migration plus its `schema_migrations` row commits in one `IMMEDIATE`
  transaction.
- Re-applying is a no-op.
- A recorded version newer than this binary is `STORAGE_SCHEMA_UNSUPPORTED`.
- After migrate, required tables/columns are verified. Mismatches are
  `STORAGE_SCHEMA_INCOMPATIBLE` and do not delete data.
- A timestamped SQLite backup runs only before a migration marked
  `destructive`. v0.1 migrations are additive, so no backup is taken today.

Empty-database and v1→v2 upgrade tests exercise repositories after upgrade, not
only table existence.

## SQLite configuration

Evaluated against a single-writer daemon on Linux and macOS.

| Setting | Value | Why |
|---|---|---|
| Foreign keys | `ON` every connection | Enforcement is per-connection, not stored. |
| Busy timeout | 5 seconds | Lets a rare lock wait instead of failing immediately. |
| Journal mode | `WAL` on files | Crash recovery, non-blocking readers later, supported on both target OSes. |
| Journal mode | `memory` in-memory DBs | SQLite cannot WAL an in-memory database. Tests that assert WAL use files. |
| Synchronous | `FULL` | Metadata writes are infrequent (session/project edges, not PTY bytes). FULL survives process crash and is stronger on OS crash than NORMAL. |
| Concurrency | one `Mutex<Connection>` | Architecture: one process owns SQLite. WAL is ready if a future reader is added. |
| Close | `PRAGMA wal_checkpoint(TRUNCATE)` via `Storage::close`; `PASSIVE` on `Drop` | Leaves a consistent `-wal` merge without deleting the database. |

WAL is appropriate for the current model: one writer, file-backed user data
dir, Linux and macOS. It is not used for ephemeral in-memory tests.

## Persistence behavior

Supported through repositories (no SQL in IPC or domain commands):

- add / rename / remove project (metadata only)
- create / update / disable / remove custom agent
- create session (`starting`), update status, record branch/worktree
- record start (`running`, historical PID, daemon instance id)
- record exit and exit code
- list after reopen of the same database file

Secrets:

- Environment keys that look like tokens are rejected.
- A map that looks like a full process environment (`HOME`+`USER`+`SHELL`, or
  more than 32 keys) is rejected.
- Settings keys matching the same token markers are rejected.
- Args are stored as a JSON string array, never a shell string.

## Recovery and reconciliation

`Storage::reconcile_sessions` loads persisted rows and consults
`LiveSessionIndex` (implemented by slices today, by `SessionManager` later).

| Situation | Result |
|---|---|
| Known metadata, already `unknown` | `Known`, unchanged |
| Session id is live in this process | `Running`, status kept |
| Live status, same `daemon_instance_id`, not in the index | `ProcessGone` → `unknown` |
| Live status, different or missing daemon instance | `DaemonRestarted` → `unknown` |
| Already `exited` or `failed` | `ExitedNormally`, history kept |

PID is never signaled and never used as proof of ownership. Processes are
never recreated. Exit codes, names, and last PID remain.

## Integrity

- Foreign keys with `ON DELETE RESTRICT` for project/agent references.
- Unique project path, unique worktree path, unique `(project_id, branch)`.
- Check constraints on names, statuses, worktree states, agent source.
- Multi-step session+worktree insert uses an immediate transaction and rolls
  back on error.
- Removing a project is refused while sessions or worktrees exist.
- Missing paths error on insert and are flagged on read; rows stay.

## Tests

`cargo test -p cli-master-storage` covers:

- empty-database migrate, idempotent reopen, v1→v2 upgrade
- WAL / foreign keys / busy timeout / FULL synchronous
- project CRUD and reopen
- custom agent CRUD, secret rejection, full-env rejection
- session create, status, branch/worktree, exit, reload
- reconciliation after simulated daemon restart
- foreign keys, constraint checks, unique path/branch
- transactional rollback
- corrupted file and future schema version
- concurrent inserts on one `Storage`
- user-facing errors omit SQL and secrets

`cargo test -p cli-master-core` still passes after additive DTO fields
(`updated_at_ms`, `started_at_ms`, `exited_at_ms`) and the historical-PID
documentation on `Session.pid`.

Workspace `cargo clippy` for the Tauri crate needs GTK/WebKit on the host.
Core and storage clippy with `-D warnings` pass.

## Risks

- There is still no daemon crate. UI close/reopen preservation depends on the
  daemon calling these repositories and `state.snapshot`.
- `idle`/`running` transitions are written only when the daemon asks. This
  crate does not watch PTYs.
- Canonicalize-on-insert follows symlinks. Two registrations of the same
  physical directory collide, which is intended.
- `FULL` + WAL is slightly slower than `NORMAL`. Acceptable for metadata.
- Token filtering is heuristic. A determined user can still store a secret
  under a benign key. Do not add a dump-the-environment command.
- SQLite cannot reattach PTYs. After daemon crash, `unknown` is the honest
  state even if an orphan child still exists.

## Integration steps

1. Add `cli-master-storage` to the daemon crate.
2. On start: resolve `default_database_path()`, `Storage::open_migrated`, seed
   built-in agents with `upsert_builtin_agent`.
3. Generate a daemon instance id. Build `RecoveryContext` from that id and
   `SessionManager` live ids (empty after a process restart).
4. Call `reconcile_sessions` **before** accepting IPC. Do not spawn agents.
5. Implement project/session/agent IPC by calling repositories only.
6. On session spawn: `create_session` then `record_session_started`. On exit:
   `record_session_exit`. Never treat `session.pid` as live after restart.
7. Surface `StorageError::to_api_error()` on the IPC boundary.
8. Keep the UI reading snapshots/events. No SQLite in the webview.

Platform paths:

- Linux: `$XDG_DATA_HOME/cli-master/cli-master.db` or
  `~/.local/share/cli-master/cli-master.db`
- macOS: `~/Library/Application Support/cli-master/cli-master.db`
