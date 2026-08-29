# Agents

This file is the working agreement for Codex sessions and any other agent
that edits this repository. Read it before writing code. If a change would
violate a rule here, stop and update an ADR instead of improvising.

## Product

CLI Master is a local-first desktop app for Linux and macOS. It coordinates
projects, Git worktrees, PTYs, and coding-agent CLIs. It is not an agent.
It does not proxy vendor traffic. Windows is out of scope for Beta v0.1.

## Source of truth

| Concern | Owner |
|---|---|
| Domain types, errors, IPC catalog | `crates/core` |
| SQLite connection, migrations, SQL | `crates/storage` |
| Adapter detection and `CommandSpec` | `crates/agents` |
| Git argv and worktree safety | `crates/git` |
| PTY, process groups, `SessionManager` | `crates/session` |
| Unix socket, composition, recovery | `crates/daemon` |
| Tauri window, dialogs, event relay | `apps/desktop/src-tauri` |
| React views and typed IPC client | `apps/desktop/src` |
| Authoritative Beta wire contract | `crates/core/src/wire` |
| TypeScript/catalog mirrors | `apps/desktop/src/ipc`, `protocol/catalog.json` |

`crates/core` has no I/O. If you need a filesystem, you are in the wrong crate.

## IPC

The Beta v1 catalog is authoritative in Rust and mirrored in two artifacts:

1. `cli_master_core::wire::method` and `wire::event_name`
2. `protocol/catalog.json`
3. `apps/desktop/src/ipc/methods.ts`

Add a method in `crates/core/src/wire` first, then update both mirrors and
`apps/desktop/src/ipc/domain.ts`. Do not
add a raw Tauri command for a domain operation. The desktop process forwards
daemon requests. It does not own sessions.

Envelope method/event names remain strings for forward compatibility. The
daemon checks `wire::method::is_supported` and rejects unknown methods; clients
ignore unknown events. Do not invent behavior for a name you do not know.

## Sessions, processes, and the UI

`SessionManager` in the daemon is the only code allowed to:

- open a PTY
- spawn an agent process
- write status transitions
- signal a process group

`SessionWorktreeSaga` coordinates Git and SQLite effects for isolated session
creation, recovery, and two-step worktree removal. It delegates every process
spawn and rollback to `SessionManager`; it must not open PTYs or signal PIDs.

React may call `session.start` / `session.stop` / `session.write`. Unmounting a
terminal view must not stop the session. Do not store PTY bytes in React
state. Do not kill a PID from the UI. Although the local public session DTO
reports daemon-observed `pid` and `ptyId`, only the daemon may act on them.

## Agents and commands

Launch with `CommandSpec`: executable plus an argument array plus cwd plus
env overrides. Never interpolate a shell string. Never wrap the child in
`sh -c`. Custom agent env values may cross IPC to the local owner. They must
not appear in logs, `Debug`, or error details.

Agent IDs, including built-in definitions persisted locally, are UUIDv7 values.
Adapter keys such as `codex` are discovery keys, not wire or database IDs.

## Git

`crates/git` will invoke the system `git` binary with an argv array. Status
parsing uses porcelain v2 with NUL delimiters. Worktree removal is two-step
through `worktree.prepare_remove` and a state-bound confirmation token; there
is no dirty-delete bypass. Deleting session metadata
must not delete a worktree. Removing a project must not delete the repo
directory.

## Persistence

SQLite stores metadata. Memory stores PTY handles and the replay buffer.
After a daemon crash, live rows become `unknown`. Never signal a stale PID.

Timestamps on the wire and in SQLite are Unix epoch milliseconds.

## Platforms

Linux and macOS are first-class. Do not add Win32, ConPTY, or `\` path
logic. Prefer XDG directories on Linux. Compare paths without assuming a
case-insensitive filesystem. Stop a session by signaling its process group,
not the daemon, and not sibling sessions.

## Tests

Backend tests use real temp dirs, real SQLite, the real `git` binary, and
short-lived child programs. Frontend tests mock the project-owned IPC client,
not scattered Tauri APIs. Do not unit-test xterm.js.

## Desktop bridge

The Tauri process forwards versioned wire envelopes through generic commands
(`daemon_invoke`, `daemon_status`, `daemon_reconnect`, `app_quit`). Do not add
domain Tauri commands for `project.*`, `agent.*`, `session.*`, or `git.*`.
`protocol_info` remains a local catalog snapshot and is not `system.hello`.
Closing the window disconnects the socket client and does not stop daemon-owned
sessions. See `docs/desktop/daemon-sidecar.md` for binary discovery.
