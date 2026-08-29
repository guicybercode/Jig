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
├── reader OS thread  → Tokio mpsc → 8ms / 8KiB batcher
├── replay ring buffer (default 8 MiB)
└── output broadcast (bounded, lag = resubscribe)
```

`cli-master-session` is the daemon process layer. It does not open SQLite or
speak the Unix socket protocol. `cli-masterd` should hold one `SessionManager`,
persist status on `StatusChanged` / `Exited`, and adapt raw `SessionEvent`
values to the authoritative types in `cli_master_core::wire`.

Spawn uses `portable-pty` and a structured `CommandSpec`. There is no `sh -c`
wrapper around agent commands; test shell commands are written through the PTY
after a shell has been spawned directly.

Stop sequence, Linux and macOS:

1. Record daemon-private stop intent; the public status remains process-bearing.
2. `SIGINT` to the session process group, never the daemon's group, never pgid `<= 1`.
3. Wait `interrupt_timeout`.
4. `SIGTERM` if still live.
5. Wait `terminate_timeout`.
6. `SIGKILL` plus `ChildKiller::kill` if still live, then fail noisily on timeout.

`portable-pty` calls `setsid` in the child, so the child pid is the session
and process-group leader. `killpg` then hits the foreground job and obvious
grandchildren. Immediate kill remains a daemon-internal shutdown primitive;
there is no `session.kill` Beta method. Drop of the last `SessionManager` clone
SIGKILLs leftovers so tests do not leak.

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
| `exited` | `wait` completed, exit code stored when the OS reports one |
| `failed` | validation, spawn, wait, or an unrequested non-zero exit failed |
| `unknown` | still a protocol value; this crate does not invent daemon-crash recovery |

`idle` is PTY silence, not "agent thinking". Stop-in-progress is daemon-owned
runtime policy and does not add a public or persisted status.

## Files changed

- `ARCHITECTURE.md` — PTY batching aligned with the bounded wire payload
- `Cargo.toml` — workspace member `crates/session`
- `Cargo.lock` — `portable-pty`, `tokio`, `nix`, and `tracing`
- `crates/core/src/model.rs` — process-bearing status helper without changing wire values
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

- The daemon socket foundation exists, but it still has to compose this manager
  with storage and map the existing `core::wire` session methods and events.
- Output chunks are capped at the wire limit of 8 KiB. The daemon adapter must
  encode them as `PtyOutputBase64`, apply subscription cursors, and emit explicit
  replay-complete or output-gap events.
- Stop intent is private runtime state. It must never be persisted or mirrored
  as an extra wire status.
- xterm.js must consume `session.output` imperatively. Putting PTY bytes in
  React state will miss the 10-session target even if the daemon is fine.
- `session.unsubscribe` must not call `stop`. Unmount is reconnect, not
  teardown.
- Resize should be debounced in the UI; the manager applies the last size it
  is given.
- Do not log `CommandSpec` arguments, env values, or chunk bytes. The manager
  logs session id, pid, executable name, dimensions, and lifecycle events
  only.
