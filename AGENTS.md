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
| Git argv and worktree safety | `crates/git` (not created yet) |
| PTY, process groups, `SessionManager` | `crates/session` (not created yet) |
| Unix socket, composition, recovery | `crates/daemon` (not created yet) |
| Tauri window, dialogs, event relay | `apps/desktop/src-tauri` |
| React views and typed IPC client | `apps/desktop/src` |
| Shared method/event names | `protocol/catalog.json` |

`crates/core` has no I/O. If you need a filesystem, you are in the wrong crate.

## IPC

The v1 catalog lives in three places that must stay equal:

1. `cli_master_core::IpcMethod` and `IpcEvent`
2. `protocol/catalog.json`
3. `apps/desktop/src/ipc/methods.ts`

Add a method by changing all three, plus request/response types in
`crates/core/src/payloads.rs` and `apps/desktop/src/ipc/domain.ts`. Do not
add a raw Tauri command for a domain operation. The desktop process forwards
daemon requests. It does not own sessions.

Unknown methods deserialize as `IpcMethod::Unknown` and must be rejected with
`PROTOCOL_UNKNOWN_METHOD`. Do not invent behavior for a name you do not know.

## Sessions, processes, and the UI

`SessionManager` in the daemon is the only code allowed to:

- open a PTY
- spawn an agent process
- write status transitions
- signal a process group

React may call `session.start` / `session.stop` / `session.write`. Unmounting a
terminal view must not stop the session. Do not store PTY bytes in React
state. Do not kill a PID from the UI. Public session DTOs have no `ptyId`
and no `pid` for that reason.

Legal status edges are encoded in `SessionStatus::can_transition_to`. Copy
that table; do not keep a second one in the daemon.

## Agents and commands

Launch with `CommandSpec`: executable plus an argument array plus cwd plus
env overrides. Never interpolate a shell string. Never wrap the child in
`sh -c`. Custom agent env values may cross IPC to the local owner. They must
not appear in logs, `Debug`, or error details.

Built-in IDs are `codex`, `claude`, `gemini`, and `opencode`. Those strings
are the primary keys. Do not mint UUIDs for built-ins.

## Git

`crates/git` will invoke the system `git` binary with an argv array. Status
parsing uses porcelain v2 with NUL delimiters. Worktree removal is two-step
and refuses dirty trees unless the user confirms. Deleting session metadata
must not delete a worktree. Removing a project must not delete the repo
directory.

## Persistence

SQLite stores metadata. Memory stores PTY handles and the replay buffer.
After a daemon crash, live rows become `unknown`. Never signal a stale PID.

Timestamps on the wire and in SQLite are RFC 3339 UTC strings.

## Platforms

Linux and macOS are first-class. Do not add Win32, ConPTY, or `\` path
logic. Prefer XDG directories on Linux. Compare paths without assuming a
case-insensitive filesystem. Stop a session by signaling its process group,
not the daemon, and not sibling sessions.

## Tests

Backend tests use real temp dirs, real SQLite, the real `git` binary, and
short-lived child programs. Frontend tests mock the project-owned IPC client,
not scattered Tauri APIs. Do not unit-test xterm.js.

## What this session did not build

There is still no `cli-masterd`, no PTY crate, and no Git crate. Do not
pretend the desktop `protocol_info` command is a daemon handshake. The next
sessions should create those crates against the types in `crates/core`.
