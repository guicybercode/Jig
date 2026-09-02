# Known issues for Beta v0.2

This page lists current product and distribution limits. It does not claim
coverage from tests that do not exercise the packaged desktop application.

## Daemon restart cannot reattach terminals

At the runtime layer, dropping and recreating a subscriber can preserve a
session while the daemon remains alive. If the daemon itself exits, its PTY
handles are lost. Startup recovery marks affected session metadata as
`unknown` and does not reattach to, or signal, a stored PID.

## Signing and notarization are not configured

Release packaging was introduced by
[PR #16](https://github.com/guicybercode/Jig/pull/16), but the repository
does not contain a signing identity, certificate, or macOS notarization
credentials. Treat generated macOS bundles as unsigned until a separate
human-controlled signing process is established.

## Package support is limited to Beta platforms

AppImage is the supported Linux package format for Beta v0.2. Windows is
outside the Beta scope; no `.exe`, MSI, or NSIS package is produced or tested.
