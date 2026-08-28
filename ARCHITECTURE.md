# Architecture — Beta v0.1

## 1. Purpose and scope

This document defines the implementable architecture for a local-first desktop
workstation that orchestrates existing coding-agent CLIs on Linux and macOS.
The application is not an agent and does not proxy agent traffic through a
cloud service. It starts the user's installed CLI programs in real PTYs and
coordinates projects, sessions, Git worktrees, and terminal views.

The v0.1 domain model is:

```text
Workspace (the local application)
└── Project (a registered Git repository)
    └── Session (one running or historical agent invocation)
        ├── Worktree (optional Git isolation)
        └── Agent definition (built-in adapter or custom executable)
```

`Task` is intentionally not persisted in v0.1. A session name carries the
task-like label until multi-agent task orchestration has a concrete use case.

## 2. Architectural decisions

### 2.1 Separate daemon owns every live session

**Status:** Accepted for v0.1.

The Tauri desktop process does not own agent child processes. A separately
packaged Rust executable, `cli-masterd`, is the single per-user session daemon.
It owns SQLite access, PTY masters, child process groups, terminal buffers, and
session lifecycle state. The Tauri process is a thin, typed IPC bridge between
the webview and the daemon.

```text
React + xterm.js
       │ typed Tauri commands/events
       ▼
Tauri 2 desktop bridge
       │ versioned JSON messages over a per-user Unix socket
       ▼
cli-masterd (one instance per OS user)
       ├── SessionManager ── PTY ── agent process group
       ├── AgentRegistry
       ├── GitService ── system git
       └── Storage ── SQLite
```

Consequences:

- A webview reload, window close, or desktop bridge restart does not terminate
  active sessions. A new client obtains a snapshot and subscribes to live
  events.
- One process serializes ownership of mutable session state and database
  migrations. The UI never opens the database directly.
- Packaging and diagnostics include two Rust executables instead of one.
- If the daemon itself crashes, v0.1 cannot reattach to the old PTY master. On
  restart it marks previously live sessions `unknown` and offers a restart.
  Metadata survives. PTY reattachment across daemon restarts is deferred; the
  boundary permits a future per-session host or terminal multiplexer.

Embedding the manager in Tauri was rejected because closing the application
would necessarily tear down all PTYs. A TCP/HTTP service was rejected because
it adds port allocation and a larger local attack surface without v0.1 value.

### 2.2 Modular Rust workspace, not microservices

The backend is a modular monolith split into small crates at boundaries that
need independent testing: domain/protocol, storage, agents, Git, PTY/session,
daemon, and desktop bridge. Only `cli-masterd` is a background service. The
crates communicate through Rust APIs, not network calls.

This keeps the operational model simple while allowing a future `agentctl` to
reuse the same protocol and domain types.

### 2.3 System Git and a portable PTY implementation

Git operations invoke the installed `git` executable with a structured
executable/argument array. This preserves the user's Git behavior and avoids
reimplementing worktree semantics. Shell command strings are not accepted.

PTY support uses `portable-pty`, with a narrow internal `PtyBackend` interface
so a platform-specific replacement remains possible if testing exposes a
terminal correctness issue. The daemon starts the executable directly in the
PTY; it does not wrap agents in `sh -c` or `zsh -c`.

### 2.4 SQLite is authoritative for metadata, memory is authoritative for PTYs

SQLite stores projects, agent definitions, sessions, worktrees, and settings.
The daemon's `SessionManager` is authoritative for live PTY handles and output
buffers. Runtime changes update SQLite at meaningful transitions, not for each
output chunk. SQLite uses WAL mode, foreign keys, a busy timeout, and embedded
forward-only migrations.

Terminal output is not written to SQLite in v0.1. Each live session has a
bounded in-memory replay buffer. This prevents output-heavy agents from causing
unbounded database growth or write amplification.

## 3. Intended repository structure

```text
.
├── apps/
│   └── desktop/
│       ├── src/                    # React + TypeScript + Vite
│       │   ├── app/                # application shell and providers
│       │   ├── components/         # shared presentational components
│       │   ├── features/
│       │   │   ├── agents/
│       │   │   ├── git/
│       │   │   ├── projects/
│       │   │   ├── sessions/
│       │   │   ├── settings/
│       │   │   └── terminal/
│       │   ├── ipc/                # typed client, schemas, event router
│       │   └── stores/             # metadata/view state; no PTY ownership
│       ├── src-tauri/              # Tauri 2 bridge and bundling config
│       ├── index.html
│       └── vite.config.ts
├── crates/
│   ├── core/                       # IDs, domain types, DTOs, protocol, errors
│   ├── storage/                    # SQLite repositories and migrations
│   ├── agents/                     # registry, built-ins, custom definitions
│   ├── git/                        # repository and worktree operations
│   ├── session/                    # PTY backend and SessionManager
│   └── daemon/                     # cli-masterd binary, socket, orchestration
├── tests/
│   └── fixtures/                   # cross-crate integration fixtures
├── Cargo.toml                      # Rust workspace
├── package.json
├── pnpm-workspace.yaml
├── ARCHITECTURE.md
├── CONTRIBUTING.md
└── README.md
```

Crates may begin with more than one internal module in a single crate if that
keeps Phase 1 smaller. A crate boundary is added only when ownership, platform
isolation, or integration testing benefits from it.

## 4. Backend modules and ownership

| Module | Owns | Must not own |
|---|---|---|
| `core` | UUID-backed IDs, domain enums, serialized DTOs, protocol envelopes, stable error codes | SQLite connections, PTYs, UI types |
| `storage` | connection setup, migrations, transactions, repository queries | child processes or Git commands |
| `agents` | built-in adapters, custom definitions, PATH detection, safe `CommandSpec` creation | process lifetime or terminal rendering |
| `git` | repository validation, status/diff, branch naming, worktree create/list/remove safety | arbitrary shell execution or UI confirmation |
| `session` | `SessionManager`, PTY handles, process groups, output buffering, resize/input/stop/restart | project registration or React state |
| `daemon` | single-instance lifecycle, IPC server, authorization, service composition, event fan-out, recovery | terminal emulation or vendor-specific UI |
| `src-tauri` | daemon discovery/start, request forwarding, webview event bridge, native dialogs/clipboard | PTY handles, SQLite, business rules |

The daemon uses explicit services rather than a global mutable application
object. A practical composition root is:

```rust
struct DaemonState {
    storage: Storage,
    agents: AgentRegistry,
    git: GitService,
    sessions: SessionManager,
    events: EventBus,
}
```

`DaemonState` is an implementation detail. IPC exposes DTOs from `core`, never
database rows, `portable-pty` handles, or Rust error internals.

## 5. Agent adapter contract

The registry treats built-in and custom agents uniformly after validation.
Built-ins provide defaults in code; custom definitions are stored in SQLite.

```rust
pub trait AgentAdapter: Send + Sync {
    fn definition(&self) -> AgentDefinition;
    fn detect(&self, environment: &LaunchEnvironment) -> DetectionResult;
    fn build_command(&self, context: &LaunchContext) -> Result<CommandSpec, AgentError>;
}

pub struct AgentDefinition {
    pub id: AgentId,
    pub display_name: String,
    pub source: AgentSource, // BuiltIn | Custom
}

pub struct CommandSpec {
    pub executable: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    pub env_overrides: BTreeMap<OsString, OsString>,
}
```

Built-in adapter IDs are stable: `codex`, `claude`, `gemini`, and `opencode`.
Adapters do not guess optional vendor flags. In v0.1 they resolve the executable
and launch the CLI in its normal interactive mode. Flags are added only after
validation against the installed CLI version.

Custom agents store a display name, executable, ordered argument array, and
non-secret environment overrides. The daemon rejects NUL bytes, empty
executables, and an invalid working directory. An executable may be absolute or
resolved from the effective PATH. Arguments are never parsed as a shell string.
Secret values and agent authentication tokens are not stored; agents inherit
their already configured local authentication environment.

GUI applications often receive an incomplete PATH. `LaunchEnvironment`
combines the daemon's inherited PATH, standard Linux/macOS executable paths,
and user-configured search directories. The setup UI may explicitly import PATH
from the user's login shell by running the configured shell with a constant,
non-user-interpolated command and a short timeout. It shows that this executes
shell startup files. The resulting PATH may be saved; the full environment is
never persisted or logged.

## 6. SessionManager

`SessionManager` is a daemon-owned map from `SessionId` to `LiveSession`. Each
live session contains one PTY master, one child/process-group handle, current
dimensions, state, exit information, a bounded output buffer, and broadcast
subscribers.

```text
SessionManager
├── persisted session metadata (through Storage)
├── LiveSession A ── PTY master ── process group A
├── LiveSession B ── PTY master ── process group B
└── event fan-out ── desktop client(s)
```

### 6.1 Creation sequence

1. Validate the project, selected agent, requested directory, and dimensions.
2. If isolation is requested, reserve a worktree record as `creating`.
3. Create the branch/worktree through `GitService`; mark it `active`, or perform
   a safe compensating cleanup and record the actionable failure.
4. Insert the session with `starting` status before spawning.
5. Build a `CommandSpec`, open a PTY, set its initial size, and spawn the child
   in its own process group with the worktree/project as `cwd`.
6. Attach output and exit readers, record volatile PID/daemon instance data,
   then publish status and metadata events.

Git and SQLite cannot share a transaction. The explicit `creating` state makes
partial work visible and recoverable instead of pretending the operation is
atomic.

### 6.2 Input, output, and backpressure

- `session.write` accepts bytes, not a command string, and writes them to the
  PTY master. Ctrl+C is ordinary terminal input (`0x03`).
- Reader tasks batch output for up to 8 ms or 32 KiB, whichever happens first.
  Each chunk has a monotonically increasing per-session sequence number.
- A live session retains a configurable bounded replay buffer (default 8 MiB).
  On subscription the daemon sends a snapshot followed by chunks after the
  snapshot sequence. This reconstructs the usual xterm view after a UI reload.
- Client queues are bounded. A slow client receives `session.output_gap` and
  must request a new snapshot; it cannot block the PTY reader or other sessions.
- xterm.js is written through an imperative terminal controller. PTY bytes do
  not flow through React component state, avoiding a rerender per chunk.
- `session.resize` is debounced in the UI and the latest columns/rows are sent
  to the PTY. Grid visibility changes trigger a fresh measurement.

### 6.3 Status state machine

```text
create ──> starting ──> running <──> idle ──> exited
              │            │          │
              └────────────┴──────────┴────> failed

daemon recovery of a formerly live row ──> unknown
unknown ── restart ──> starting
```

- `starting`: metadata exists and launch is in progress.
- `running`: the process exists and recent PTY input/output indicates activity.
- `idle`: the process exists but no PTY activity occurred for the configured
  heuristic interval (10 seconds initially). This is not an LLM semantic state.
- `exited`: the process ended normally; the exit code is recorded.
- `failed`: validation, spawn, PTY, or abnormal exit failed with an actionable
  error.
- `unknown`: metadata claims a live session from another daemon instance, but
  v0.1 cannot safely prove or reattach its PTY.

Status changes are edge-triggered events. React does not poll each process.
Process liveness checks run inside the daemon at a modest interval only as a
fallback to exit notification.

### 6.4 Stop, restart, and delete

Stop and delete are separate commands. Stop sends an interrupt/termination to
the session process group, waits a short grace period, then offers a separately
confirmed force-kill. Restart reuses session metadata and the same working
directory but creates a new PTY and PID. Delete is allowed only after the
process stops; it removes metadata, never repository files. A worktree remains
a separately managed resource.

## 7. Daemon and desktop lifecycle

The desktop bridge connects to a socket in a user-only runtime directory. On
Linux this is under `$XDG_RUNTIME_DIR/cli-master` when available, with a
validated user-owned fallback below the OS temporary directory. On macOS it is
under the user's application-support/runtime area while respecting Unix socket
path-length limits. The directory is mode `0700`, the socket is `0600`, and the
daemon verifies the connecting peer UID (`SO_PEERCRED` on Linux,
`getpeereid` on macOS).

The daemon obtains a per-user lock before opening SQLite. The bridge follows
this sequence:

1. Connect and perform the protocol handshake.
2. If no daemon responds, start the bundled `cli-masterd` sidecar detached from
   the desktop process.
3. Retry with a bounded backoff; show diagnostics if startup or migration fails.
4. Request `state.snapshot`, then subscribe to session and project events.

Closing a window disconnects only that client. With active sessions, the daemon
stays alive. With no live sessions and no clients, it may exit after a five
minute idle timeout. The desktop offers distinct actions for **Close window**,
**Quit UI and keep sessions**, and **Stop all sessions and quit**; only the last
action terminates agent processes.

On a clean daemon shutdown, all child process groups are stopped before the PTY
masters close. On daemon startup, rows left in `starting`, `running`, or `idle`
by a different daemon instance become `unknown`; stale PIDs are never signaled
because PID reuse makes that unsafe. A daemon crash will normally close PTY
masters and cause children to receive hangup, but v0.1 documents that orphaned
processes may require manual inspection.

## 8. SQLite schema and migrations

All timestamps are UTC RFC 3339 strings. IDs are UUIDv7 strings. Paths are
stored as canonical absolute paths after validation; the UI may separately
abbreviate them for display. The first migration creates:

```sql
PRAGMA foreign_keys = ON;

CREATE TABLE projects (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL CHECK (length(trim(name)) > 0),
    path            TEXT NOT NULL UNIQUE,
    created_at      TEXT NOT NULL,
    last_opened_at  TEXT NOT NULL
);

CREATE TABLE agents (
    id              TEXT PRIMARY KEY,
    source          TEXT NOT NULL CHECK (source IN ('built_in', 'custom')),
    name            TEXT NOT NULL CHECK (length(trim(name)) > 0),
    executable      TEXT NOT NULL CHECK (length(trim(executable)) > 0),
    args_json       TEXT NOT NULL DEFAULT '[]',
    env_json        TEXT NOT NULL DEFAULT '{}',
    enabled         INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE TABLE sessions (
    id                  TEXT PRIMARY KEY,
    project_id          TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    agent_id            TEXT NOT NULL REFERENCES agents(id) ON DELETE RESTRICT,
    name                TEXT NOT NULL CHECK (length(trim(name)) > 0),
    cwd                 TEXT NOT NULL,
    status              TEXT NOT NULL CHECK (
                            status IN ('starting','running','idle','exited','failed','unknown')
                        ),
    runtime_pid         INTEGER,
    daemon_instance_id  TEXT,
    exit_code           INTEGER,
    error_code          TEXT,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    last_activity_at    TEXT
);

CREATE TABLE worktrees (
    id              TEXT PRIMARY KEY,
    project_id      TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    session_id      TEXT UNIQUE REFERENCES sessions(id) ON DELETE SET NULL,
    path            TEXT NOT NULL UNIQUE,
    branch          TEXT NOT NULL,
    state           TEXT NOT NULL CHECK (
                        state IN ('creating','active','remove_pending','orphaned')
                    ),
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    UNIQUE(project_id, branch)
);

CREATE TABLE settings (
    key             TEXT PRIMARY KEY,
    value_json      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE INDEX sessions_by_project_updated
    ON sessions(project_id, updated_at DESC);
CREATE INDEX sessions_by_status ON sessions(status);
CREATE INDEX worktrees_by_project ON worktrees(project_id);
```

Built-in agent rows are idempotently seeded/upserted at startup. A user can
disable but not mutate their executable/argument defaults; custom rows are
editable. Removing an agent definition referenced by a session is rejected;
disabling it preserves history.

Migrations are numbered SQL files embedded in `storage` and applied by the
daemon before accepting IPC requests. Each migration runs transactionally when
SQLite permits it. Startup makes a timestamped database backup before a
destructive migration; v0.1 migrations should remain additive whenever
possible. Migration tests create an old fixture database, upgrade it, and
exercise reads and writes through repositories rather than checking only table
existence.

The database lives in:

- Linux: `$XDG_DATA_HOME/cli-master/cli-master.db`, falling back to
  `~/.local/share/cli-master/cli-master.db`.
- macOS: `~/Library/Application Support/cli-master/cli-master.db`.

Database, logs, and runtime socket locations come from a single platform-path
module and are surfaced in diagnostics.

## 9. Stable IPC protocol

There are two IPC hops, but one domain protocol:

1. React calls typed Tauri commands and receives Tauri events.
2. The Tauri bridge forwards equivalent versioned messages to the daemon and
   relays daemon events.

The TypeScript client is generated from, or contract-tested against, Rust DTO
schemas. Runtime payloads are validated at the webview boundary. Protocol names
and DTO fields are stable within major version `1`; adding optional fields is
compatible, while renaming/removing fields requires a new major version.

Daemon transport is length-delimited JSON (one envelope per frame) over the
Unix socket. PTY byte fields use base64 in v0.1 so arbitrary terminal bytes do
not depend on JSON UTF-8 behavior. If profiling proves encoding material, a
binary frame type can be added without changing domain commands.

```json
{
  "kind": "request",
  "version": 1,
  "requestId": "019...",
  "method": "session.create",
  "payload": {}
}
```

```json
{
  "kind": "response",
  "version": 1,
  "requestId": "019...",
  "ok": false,
  "error": {
    "code": "AGENT_EXECUTABLE_NOT_FOUND",
    "message": "Could not start Codex because executable 'codex' was not found.",
    "action": "Install Codex or configure an executable search path.",
    "details": { "searchedPath": ["/usr/local/bin", "/usr/bin"] }
  }
}
```

Supported v0.1 requests:

| Method | Purpose |
|---|---|
| `system.hello` | negotiate protocol and report daemon instance/version |
| `state.snapshot` | projects, agents, sessions, worktrees, current statuses |
| `project.add`, `project.list` | validate/canonicalize and register repositories |
| `project.rename`, `project.remove` | update app metadata; never delete the directory |
| `agent.list`, `agent.detect` | definitions and installation status |
| `agent.custom.create`, `agent.custom.update`, `agent.custom.remove` | manage structured custom commands |
| `session.create`, `session.list`, `session.get` | create and inspect sessions |
| `session.subscribe`, `session.unsubscribe` | snapshot/replay and live terminal events |
| `session.write`, `session.resize` | PTY byte input and dimensions |
| `session.stop`, `session.kill`, `session.restart` | explicit lifecycle operations |
| `session.rename`, `session.delete` | metadata operations independent of worktrees |
| `git.status`, `git.diff` | branch, changed files, counts, bounded textual diff |
| `worktree.create`, `worktree.list` | isolated branch/worktree operations |
| `worktree.prepare_remove`, `worktree.remove` | two-step safe removal |
| `diagnostics.get`, `diagnostics.export` | sanitized versions, paths, and recent logs |

Supported v0.1 events:

| Event | Payload notes |
|---|---|
| `session.created`, `session.updated`, `session.deleted` | complete public session DTO |
| `session.output` | session ID, sequence, base64 bytes |
| `session.output_gap` | session ID and last available sequence |
| `session.status_changed` | previous/new state, timestamp, safe reason code |
| `session.exited` | exit code and final status |
| `project.updated` | complete public project DTO |
| `worktree.updated` | complete public worktree DTO |
| `git.status_changed` | emitted only after an explicit/low-rate refresh in v0.1 |
| `daemon.shutting_down` | reason and whether active sessions remain |

Every error has a stable machine code, a safe user-facing message, and a
suggested action when available. Internal causes appear in structured logs with
a correlation/request ID. Environment values, agent tokens, terminal contents,
and full custom argument lists are excluded from errors and logs.

## 10. Git and worktree safety

`GitService` executes an absolute resolved Git binary directly and always
supplies arguments as an array. Read operations disable pagers and interactive
prompts. Status uses porcelain v2 with NUL delimiters; code never parses
localized human output. Diff output is size-capped (2 MiB initially) and returns
a `truncated` flag.

Project registration canonicalizes the selected directory, verifies it exists,
and uses `git -C <path> rev-parse --show-toplevel` to find the repository root.
The root becomes the stored project path. Removing a project only removes app
metadata and is rejected while sessions/worktrees still reference it.

Generated names use a lowercase ASCII slug plus a short ID suffix:

```text
branch:   agent/implement-authentication-7k3m
worktree: <data-dir>/worktrees/<project-id>/implement-authentication-7k3m
```

The suffix prevents collisions without silently reusing an existing branch or
directory. The daemon validates that generated worktree paths remain below its
managed worktree root after path normalization.

Worktree removal is deliberately two-step:

1. `worktree.prepare_remove` checks process use, tracked changes, staged
   changes, untracked files, and Git's current worktree registry. It returns the
   findings and a short-lived confirmation token tied to that exact state.
2. `worktree.remove` requires the token, rechecks state, and refuses if state
   changed. Dirty removal requires an explicit `allowDirty` confirmation.

The backend never performs `git reset --hard`, discards changes, deletes the
main repository, or recursively deletes an unvalidated path. A running session
blocks worktree removal. Deleting session metadata does not remove its
worktree. Failed removal remains visible as `remove_pending` or `orphaned` with
recovery instructions.

## 11. Frontend architecture

React owns view composition, selection, dialogs, shortcuts, and cached public
metadata. It does not own processes or infer lifecycle from component mounts.

- A single IPC client completes `system.hello` and `state.snapshot`, then routes
  events into small feature stores.
- Project/session metadata may use a lightweight external store; ephemeral
  dialog and form state stays local to components.
- Each terminal controller owns its xterm.js instance and addons outside React
  render state. It subscribes on mount, requests a replay snapshot, applies
  ordered bytes, and unsubscribes on unmount without stopping the session.
- Grid layouts optimize the polished cases of one through four terminals and
  use a scrollable responsive grid above four. Resize observers send debounced
  PTY dimensions only for visible terminals.
- Keyboard actions go through one command registry, shared by menus and the
  command palette. Platform mappings use `Cmd` on macOS and `Ctrl` on Linux.
- The UI renders server-provided error codes/messages and never constructs Git
  or process commands itself.

Terminal output batching, bounded replay, imperative xterm writes, and isolated
terminal components are the main design choices supporting the v0.1 target of
10 active terminals without an unusable UI.

## 12. Logging, diagnostics, and observability

The daemon and desktop bridge write structured JSON logs with timestamp, level,
component, event name, request/correlation ID, project/session ID when relevant,
and safe error code. Events cover daemon lifecycle, migrations, project
validation, session transitions, agent launch outcome, PTY errors, Git operation
outcome, and database errors.

Logs never include the full environment, token-like values, terminal content,
or complete agent prompts/arguments. File logs rotate by size and count and use
user-only permissions. `diagnostics.export` produces a sanitized bundle only
after explicit user action and previews its contents before copying/saving.

For v0.1, local structured logs and simple counters (active sessions, failed
launches, dropped output chunks, Git operation duration) are sufficient. There
is no telemetry, remote collector, OpenTelemetry backend, or cloud account.

## 13. Platform behavior

### Linux

- XDG data, config, cache, and runtime directories are preferred with documented
  fallbacks and user-only permissions.
- AppImage is the first package target; `.deb` follows if packaging is reliable.
- CI exercises PTY input/output/resize and process-group termination on Linux.
- The architecture makes no X11 assumption; Tauri/WebKit handles Wayland/X11.
- Signals target the child process group so stopping one agent cannot affect
  another. Filesystem comparisons do not assume case-insensitivity.

### macOS

- Data follows application-support conventions; logs follow the user's Library.
- The daemon and desktop executable ship as matching universal or Apple Silicon
  artifacts. Homebrew is not an application dependency.
- `.app` is required; `.dmg`, signing, and notarization are release concerns and
  may be documented before automation is complete.
- Process-group lifecycle and PTY behavior receive the same integration tests as
  Linux. Closing the last window follows the explicit keep/stop behavior rather
  than macOS UI assumptions.

Windows, ConPTY, Windows path semantics, and Windows packaging are out of scope.

## 14. Testing boundaries

Critical backend tests use real temporary directories, real SQLite databases,
the system Git binary, and short-lived PTY child programs:

- agent detection: present executable, missing executable, structured custom
  agent, PATH resolution;
- Git: repository root detection, slug/collision behavior, worktree lifecycle,
  status parsing, dirty and in-use removal protection;
- storage: repositories, foreign-key behavior, metadata reload, migration from
  every supported schema version, disk/write failure reporting where practical;
- session: start, bytes in/out, Ctrl+C, resize, independent concurrent sessions,
  graceful stop, forced stop, exit code, output backpressure;
- protocol: Rust/TypeScript fixtures for every envelope and error shape,
  unknown optional fields, incompatible major version;
- lifecycle: UI client disconnect/reconnect while a daemon-owned process keeps
  running and receives a replay snapshot.

xterm.js itself is not unit-tested. Frontend tests verify event routing,
keyboard commands, view persistence, and that unmounting a terminal does not
issue a stop command. Linux and macOS smoke tests cover the acceptance scenario.

## 15. Explicit v0.1 limitations

- Metadata survives application and daemon restarts. Running PTYs survive UI
  reloads and desktop-client exits while the daemon stays alive, but do not
  survive daemon crash/restart, OS logout, reboot, or machine shutdown.
- Replay history is bounded and in memory. Very long sessions may lose early
  scrollback after a UI reload; no transcript is persisted by default.
- `idle` means only a lack of recent PTY activity. It does not mean an agent has
  completed reasoning or is waiting for a particular kind of response.
- Agent adapters launch installed CLIs; they do not manage vendor login, tokens,
  versions, prompts, or undocumented invocation flags.
- Git support is local status, branch, bounded text diff, and safe worktrees.
  Merge, rebase, cherry-pick, conflicts, PRs, and remote-provider integration are
  excluded.
- Worktree create/remove is recoverable but not atomic across Git and SQLite.
- Desktop-environment PATH discovery cannot reproduce every shell manager
  automatically. Users can inspect the effective PATH and add/import paths.
- A same-user process can access the user's files and processes already; socket
  permissions and peer UID checks prevent cross-user access but are not a
  sandbox for untrusted local programs.
- No cloud sync, authentication, telemetry service, remote execution, SSH,
  containers, team collaboration, agent-to-agent messaging, scheduler, or
  Windows support is included.

## 16. Incremental implementation order

1. Build the Tauri/Vite/React shell, Rust workspace, daemon handshake, SQLite
   migration, and typed `system.hello`/`state.snapshot` path.
2. Add projects and real Git repository detection with persistence.
3. Prove shell PTY correctness end to end before starting an agent adapter.
4. Add built-in and custom agents, then multiple independent sessions.
5. Add single/grid terminal views, reconnection/replay, and status events.
6. Add safe worktree creation/removal and launch sessions in worktrees.
7. Add bounded Git status/diff, diagnostics, errors, and platform packaging.

Every phase must leave the repository buildable on Linux and macOS. Terminal
correctness, session robustness, and Git safety take precedence over UI polish.
