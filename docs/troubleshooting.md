# Troubleshooting

## `cli-masterd --preflight` says Git is missing

Install Git and launch Jig from a context that inherits the same
`PATH`. macOS `.app` bundles do not automatically load your zshrc.

`--preflight` prints the resolved Git path when detection succeeds. If you
use Homebrew, confirm `/opt/homebrew/bin/git` exists and is executable.

## Optional CLIs are listed as unavailable

Codex, Claude Code, Gemini CLI, and OpenCode are optional. Preflight stays
green without them. A session that needs a missing binary fails at start
with `executable_not_found`, not at packaging time.

## Linux window does not open

The AppImage still needs WebKitGTK 4.1 on the host. Install the packages
your distribution documents for Tauri 2. Wayland and X11 are both
acceptable. Missing WebKit is a host problem, not an AppImage config
switch.

## macOS says the app is damaged or cannot be opened

The Beta is unsigned and not notarized. Verify `SHA256SUMS` first. Then
allow the app in Privacy & Security. If you did not download the artifact
yourself, do not override Gatekeeper.

`signingIdentity` in `tauri.conf.json` is `null` on purpose. There is no
certificate in CI.

## Socket or directory permission errors

Private directories must be mode `0700` and owned by you. If
`$XDG_RUNTIME_DIR/cli-master` or `/tmp/cli-master-*` is a symlink or owned
by another user, the daemon refuses it. Remove the untrusted path and
retry. Do not chmod a shared `/tmp` directory to make this "work".

## `another CLI Master daemon already owns ...`

A previous `cli-masterd` is still running. Find it with `ps` and stop that
process, or log out. Do not delete `daemon.lock` while the process is
alive.

## Logs

Linux: `~/.local/state/cli-master/logs/cli-masterd.json.log`
macOS: `~/Library/Logs/CLI Master/cli-masterd.json.log`

Logs are JSON. They rotate after 10 MiB. They must not contain tokens,
full environments, or terminal contents. If you see those, that is a bug.

## Desktop cannot connect to the daemon

Open Diagnostics and confirm the `system.hello` handshake. `protocol_info`
only mirrors the frozen method list; it is not proof of a live connection.
Run `cli-masterd --preflight`, inspect the JSON log, and use the reconnect
action after correcting any path or permission error.

## Checksums fail

Re-download the artifact and `SHA256SUMS` from the same workflow run. Do
not mix files from two builds. Regenerated binaries need new sums.
