# Known issues for Beta v0.1

This page lists current product and distribution limits. It does not claim
coverage from tests that do not exercise the packaged desktop application.

## Project and session flows are not wired through daemon IPC

The React UI includes **Repository path**, project workspace, session controls,
and `TerminalGrid`, but the packaged daemon currently returns an empty state
snapshot and `method_not_implemented` for the project, agent, session, and
worktree domain mutations. The packaged desktop therefore cannot yet add or
list a project, create a live session, or populate the terminal grid through
the production IPC path. There is also no native folder picker.

The corresponding manual release-checklist items must remain unchecked until
that domain bridge is implemented. Rust acceptance tests cover the underlying
services; they are not evidence that these window-level product flows work.

## Daemon restart cannot reattach terminals

At the runtime layer, dropping and recreating a subscriber can preserve a
session while the daemon remains alive. If the daemon itself exits, its PTY
handles are lost. Startup recovery marks affected session metadata as
`unknown` and does not reattach to, or signal, a stored PID. The packaged
desktop cannot exercise this lifecycle until the domain IPC gap above is
closed.

## Signing and notarization are not configured

Release packaging was introduced by
[PR #16](https://github.com/guicybercode/cli-master/pull/16), but the repository
does not contain a signing identity, certificate, or macOS notarization
credentials. Treat generated macOS bundles as unsigned until a separate
human-controlled signing process is established.

## Native browser permission defense requires packaged testing

The canvas browser uses the operating system WebKit engine and gives remote
pages no Tauri capability. Camera, microphone, screen capture, geolocation,
and notifications are denied by an all-frame document-start guard because the
current Tauri/Wry version does not expose a portable native permission-deny
handler. Release candidates must exercise a hostile page in the packaged app
on macOS and Linux. Agent-driven browsing is not part of this increment;
connections only support explicit, reviewable URL handoff to notes and live
terminals.

## Package support is limited to Beta platforms

AppImage is the supported Linux package format for Beta v0.1. Windows is
outside the Beta scope; no `.exe`, MSI, or NSIS package is produced or tested.
