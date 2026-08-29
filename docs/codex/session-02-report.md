# Session 02: daemon PTY lifecycle

This session made `SessionManager` the runtime owner of child processes and
PTY masters. The desktop process still does not hold file descriptors or
`Child` handles. Ten concurrent PTYs, reconnect replay, and process-group
stop were the acceptance bar.

## Final architecture

```text
SessionManager
├── map<SessionId, LiveSession>
├── event broadcast (bounded)
└── idle scanner (Tokio task)
        │
        ▼
LiveSession
├── PTY master (portable-pty)
├── child / clone_killer
├── process group id
├── writer OS thread  ← bounded sync_channel
├── reader OS thread  → Tokio mpsc → 8ms / 32KiB batcher
├── replay ring buffer (default 8 MiB)
└── output broadcast (bounded, lag = resubscribe)
```

`cli-master-session` is the daemon process layer. It does not open SQLite or
speak the Unix socket protocol. The future `cli-masterd` binary should hold
one `SessionManager`, persist status on `StatusChanged` / `Exited`, and
encode `SessionEvent::Output` with `SessionOutputPayload`.

Spawn uses `portable-pty` and a structured `CommandSpec`. There is no `sh -c`
wrapper around agent commands. Tests may pass a constant `sh -c` script as an
argv array.

Stop sequence, Linux and macOS:

1. Status becomes `stopping`.
2. `SIGINT` to the session process group, never the daemon's group, never pgid `<= 1`.
3. Wait `interrupt_timeout`.
4. `SIGTERM` if still live.
5. Wait `terminate_timeout`.
6. `SIGKILL` plus `ChildKiller::kill` if still live.

`portable-pty` calls `setsid` in the child, so the child pid is the session
and process-group leader. `killpg` then hits the foreground job and obvious
grandchildren. Force-kill is `session.kill`. Drop of the last `SessionManager`
clone SIGKILLs leftovers so tests do not leak.

Frontend disconnect is `subscribe` drop. The process keeps running. A later
`subscribe` returns a snapshot plus live chunks after `snapshot.next_sequence`.
A lagged live subscriber gets `SubscribeError::Lagged` and should request a
new snapshot (`session.output_gap` on the wire).

Status uses only process signals:

| Status | Meaning |
|---|---|
| `starting` | metadata inserted, spawn in progress |
| `running` | process exists, recent PTY I/O |
| `idle` | process exists, no I/O for `idle_after` (10s default) |
| `stopping` | stop/kill requested, signals in flight |
| `exited` | `wait` completed, exit code stored when the OS reports one |
| `failed` | validation or spawn failed |
| `unknown` | still a protocol value; this crate does not invent daemon-crash recovery |

`idle` is PTY silence, not "agent thinking". `stopping` is new and additive.

## Files changed

- `ARCHITECTURE.md` — `stopping` in the state machine and schema CHECK
- `Cargo.toml` — workspace member `crates/session`
- `Cargo.lock` — `portable-pty`, `tokio`, `nix`, `tracing`, `base64`
- `crates/core/Cargo.toml`, `crates/core/src/lib.rs`, `crates/core/src/model.rs`
- `crates/core/src/session_ipc.rs` — method/event names and wire payloads
- `crates/storage/migrations/0002_session_stopping.sql`
- `crates/storage/src/lib.rs`
- `crates/session/` — manager, PTY backend, replay buffer, Unix signals
- `crates/session/tests/pty_lifecycle.rs`
- `docs/codex/session-02-report.md`

No React, Tauri, or worktree changes. No Codex/Claude/Gemini detection.

## Tests

`crates/session/tests/pty_lifecycle.rs` uses `/bin/sh`, `cat`, `true`, `false`,
`sleep`, `dd`, and `stty`. Nothing requires a vendor CLI.

Covered:

- start a shell, UTF-8 output, typed input
- interactive `cat`, then Ctrl+D
- exit code 0 and 1
- Ctrl+C on `sleep`
- resize, then `stty size`
- duplicate `start` rejected
- 10 concurrent `sleep` sessions; stopping one leaves the others
- unsubscribe does not kill the process; reconnect snapshot still has output
- replay buffer truncates large `dd` output
- missing executable → `failed`
- delete after exit removes the record

`cargo fmt`, `cargo clippy -D warnings`, and `cargo test` pass on
`cli-master-core`, `cli-master-storage`, and `cli-master-session`.
`cargo doc -D warnings` passes on those crates.

This environment does not have WebKitGTK, so the Tauri crate was not built
here. Linux/macOS CI should still compile the desktop member.

## Limitations

- PTY reattach after daemon crash is still out of scope. A crash closes the
  master; children typically get hangup. Leftover processes may need a manual
  check.
- Replay is in-memory and bounded. Long sessions lose early scrollback on UI
  reload. Output is not written to SQLite.
- `portable-pty` reports signaled exits as `exit_code = 1` plus a signal name,
  not always `128 + n`. Callers should treat any recorded code as the OS
  result, not a vendor-specific agent status.
- Full-screen programs get `TERM=xterm-256color` and a real TTY. There is no
  curses CI job (no vim/less fixture).
- `setsid` plus `killpg` reaps obvious children. A child that daemonizes into
  a new session is not tracked.
- Writer backpressure is `try_send` on a 32-deep queue. A stuck PTY returns
  `session_write_timeout` rather than blocking the caller.
- `SessionManager::new` must run inside a Tokio runtime (idle scanner and
  wait tasks).

## Integration risks

- The daemon crate does not exist yet. Someone still has to map IPC methods
  (`session.create`, `session.write`, `session.resize`, `session.stop`,
  `session.kill`, `session.restart`, `session.subscribe`) onto this API and
  fan events out over the Unix socket with base64 payloads.
- Persist `stopping` only after migration 0002. Older databases reject that
  CHECK until migrated.
- xterm.js must consume `session.output` imperatively. Putting PTY bytes in
  React state will miss the 10-session target even if the daemon is fine.
- `session.unsubscribe` must not call `stop`. Unmount is reconnect, not
  teardown.
- Resize should be debounced in the UI; the manager applies the last size it
  is given.
- Do not log `CommandSpec` arguments, env values, or chunk bytes. The manager
  logs session id, pid, executable name, dimensions, and lifecycle events
  only.
