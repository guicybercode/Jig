# Daemon sidecar and socket discovery

The desktop process never owns sessions, SQLite, or PTYs. It locates the
`cli-masterd` executable, connects to the per-user Unix socket, and forwards
wire envelopes. Closing the window disconnects that client. It does not send
`SIGTERM` to the daemon, so live sessions survive the UI.

## Socket path

The bridge uses `DaemonConfig::discover()` from `cli-master-daemon`, the same
resolver the daemon binary uses:

- Linux: `$XDG_RUNTIME_DIR/cli-master/daemon.sock` when `XDG_RUNTIME_DIR` is an
  absolute path, otherwise `<data>/runtime/daemon.sock`. Data lives in
  `$XDG_DATA_HOME/cli-master` or `~/.local/share/cli-master`.
- macOS: data in `~/Library/Application Support/CLI Master`. The socket is
  `/tmp/cli-master-<16 hex digits>/daemon.sock`, where the suffix is a
  non-cryptographic hash of `$HOME`. The short path stays within `sockaddr_un`
  limits.

The socket directory is mode `0700` and the socket is `0600`. Linux also checks
`SO_PEERCRED`. Do not invent a second path scheme in Tauri.

## Binary search order

`cli-masterd` is resolved without a shell. The first executable regular file
wins:

1. `CLI_MASTERD`, if set, must be an absolute path to an executable file. A
   relative value is rejected and is not a fallback to `PATH`.
2. A file named `cli-masterd` in the same directory as the running desktop
   executable (`std::env::current_exe()`).
3. `cli-masterd` on `PATH`.
4. Development fallbacks: `target/debug/cli-masterd` then
   `target/release/cli-masterd`, relative to the workspace root inferred from
   `apps/desktop/src-tauri`.

### Linux

Packaged AppImage layouts place `cli-masterd` next to the Jig desktop
executable. The same-directory rule covers that layout. `bundle.externalBin`
stages the daemon for `tauri build`.

### macOS

The application bundle should contain:

```text
Jig.app/Contents/MacOS/cli-master-desktop
Jig.app/Contents/MacOS/cli-masterd
```

`current_exe()` is the `MacOS` directory, so the sibling lookup finds the
sidecar. Do not look inside `Resources` for the daemon binary.

## Development

`cargo build -p cli-master-daemon --bin cli-masterd` writes
`target/debug/cli-masterd`. `cargo tauri dev` then finds that file as a sibling
of `cli-master-desktop` when both land in the workspace `target/debug`
directory. If the desktop crate is built alone, set `CLI_MASTERD` or start
`cli-masterd` yourself before opening the window.

The sidecar is spawned only when the socket is not already listening. The child
is in its own process group, has `kill_on_drop` disabled, and is reaped in the
background. `app_quit` and window close call `DaemonBridge::shutdown()`, which
drops the Unix client and leaves the daemon running.
