# Known issues for Beta v0.1

This page lists current product and distribution limits. It does not claim
coverage from tests that do not exercise the packaged desktop application.

## Add Project requires a path

Add Project uses the **Repository path** text field. Enter an existing absolute
path; there is no native folder picker yet.

## Daemon restart cannot reattach terminals

Closing and reconnecting the desktop window can preserve a session while the
daemon remains alive. If the daemon itself exits, its PTY handles are lost.
Startup recovery marks affected session metadata as `unknown` and does not
reattach to, or signal, a stored PID.

## Signing and notarization are not configured

Release packaging is tracked by [PR #16](https://github.com/guicybercode/cli-master/pull/16),
but the repository does not contain a signing identity, certificate, or macOS
notarization credentials. Treat generated macOS bundles as unsigned until a
separate human-controlled signing process is established.

## Package support is limited to Beta platforms

AppImage is the supported Linux package format for Beta v0.1. Windows is
outside the Beta scope; no `.exe`, MSI, or NSIS package is produced or tested.
