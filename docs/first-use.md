# First use

This page is the shortest path from a verified install to a working local
daemon. It does not walk vendor CLI onboarding. Those tools have their own
login flows.

## 1. Confirm Git

```bash
git --version
```

GUI apps on macOS often see a shorter `PATH` than your shell. If Git lives
only in Homebrew (`/opt/homebrew/bin`), the daemon may miss it until that
directory is on the desktop process `PATH`. `--preflight` reports the Git
it actually found.

## 2. Run preflight

```bash
cli-masterd --preflight
```

You want `"ok": true` and `"git": { "available": true }`. Codex, Claude,
Gemini, and OpenCode are listed as optional. Missing ones do not fail
preflight.

On Linux this also creates user-only directories under XDG. On macOS it
creates Application Support, Caches, and Logs folders. Modes are `0700`.

## 3. Open the desktop app

Launch the AppImage or `CLI Master.app`. The window is the control center.
The session daemon is a separate executable packaged beside it.

Closing the window is not the same as stopping agents. That is the product
rule. In this Beta the Tauri process can locate `cli-masterd`; the typed
IPC handshake (`system.hello`) is owned by the daemon and is not a Tauri
command named `protocol_info`. If a build still shows only the shell UI,
the sidecar is present for the next wiring step. Do not treat
`protocol_info` as proof that sessions are live.

## 4. Add a Git project

Pick a repository you already have. CLI Master records metadata. It does
not copy the repo and it does not delete it when you remove the project
from the app.

## 5. Start an agent only when the CLI is installed

Built-in adapters resolve `codex`, `claude`, `gemini`, and `opencode` from
`PATH`. They do not invent vendor flags. If detection fails, install that
CLI or register a custom executable as an argument array, never a shell
string.

## Data the first launch creates

Linux:

| Kind | Default location |
|---|---|
| Database | `~/.local/share/cli-master/cli-master.db` |
| Config | `~/.config/cli-master` |
| Cache | `~/.cache/cli-master` |
| Logs | `~/.local/state/cli-master/logs` |
| Socket | `$XDG_RUNTIME_DIR/cli-master/daemon.sock` |

macOS:

| Kind | Location |
|---|---|
| Database | `~/Library/Application Support/CLI Master/cli-master.db` |
| Logs | `~/Library/Logs/CLI Master` |
| Cache | `~/Library/Caches/CLI Master` |
| Socket | `/tmp/cli-master-<hash>/daemon.sock` |

None of these paths are your Git repositories. Backup is described in
[backup-and-recovery.md](backup-and-recovery.md).
