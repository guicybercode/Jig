# ADR 0001: Session ownership

## Status

Accepted for Beta v0.1.

## Context

The product has to keep coding-agent CLIs alive while the user reloads the
window, closes the window, or crashes the webview. It also has to run more
than one agent at a time without one session's I/O blocking another.

Two designs were on the table: own PTYs inside the Tauri process, or own
them in a per-user daemon (`cli-masterd`) and treat Tauri as a bridge.

## Decision

`cli-masterd` owns every live session. That means the PTY master, the child
process group, the output buffer, and the only legal writes to
`SessionStatus`.

The Tauri process may start the daemon, forward typed requests, and relay
events into the webview. It must not hold PTY handles or SQLite.

React owns view state. Unmounting a terminal does not stop a session.
`session.write` and `session.resize` are commands, not component lifecycle
hooks.

Public session DTOs omit `pid` and any PTY handle. Those values tempt a UI
to kill the wrong process after PID reuse. Diagnostics can expose a
sanitized PID later if we have a real need.

## Consequences

Closing the UI is not the same as stopping agents. That is the feature.
It also means packaging two binaries, a per-user socket, and a recovery
story when the daemon itself dies.

Daemon crash still loses PTYs in v0.1. Metadata survives. Formerly live
rows become `unknown`. We do not signal leftover PIDs.

Multiple concurrent agents are then a map in `SessionManager`, not a pile
of React effects. Each session has its own process group so a stop of
session A cannot SIGINT session B.
