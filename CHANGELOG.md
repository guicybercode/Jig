# Changelog

## 0.1.0 Beta packaging

Unsigned Linux AppImage and macOS `.app` / `.dmg` builds. The session
daemon `cli-masterd` is included next to the desktop binary.

### Install

- Linux: AppImage. Verify `SHA256SUMS` before `chmod +x`.
- macOS: DMG or `.app` zip. Gatekeeper will warn; the build is not
  notarized.
- `cli-masterd --version` prints `0.1.0` and protocol `1`.
- `cli-masterd --preflight` creates XDG or Application Support/Logs
  directories at mode `0700`, requires Git, and lists optional agent CLIs.

### Not in this Beta drop

- Code signing, Hardened Runtime with a Developer ID, or notarization
- Automated GitHub Releases
- Windows
- Debian packages as a supported format
- Reattaching PTYs after a daemon crash

See [docs/install.md](docs/install.md) and
[docs/RELEASE_CHECKLIST.md](docs/RELEASE_CHECKLIST.md).
