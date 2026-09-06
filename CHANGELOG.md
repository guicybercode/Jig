# Changelog

## 0.2.0 Beta

Jig now delivers the full local project-to-terminal workflow through the
packaged desktop and daemon.

### Highlights

- Added the spatial canvas with persistent, movable, resizable terminal cards,
  connections, map controls, and terminal presets.
- Connected project, agent, Git, worktree, and live terminal operations through
  the versioned Tauri and daemon IPC bridge.
- Added native repository selection, agent settings, diagnostics, session
  recovery, bounded terminal replay, and safe worktree removal.
- Rebranded the desktop experience as Jig and refreshed its app identity.
- Added a local prompts/context library, explicit rule and skill discovery,
  and a composer that sends only to the selected live terminal.
- Added revision-checked local file IPC and organization metadata contracts;
  file editor and organization canvas integration remain separate work.
- Made process snapshots monotonic to prevent older concurrent scans from
  replacing newer session process observations.
- Expanded Linux and macOS acceptance coverage and hardened process, path,
  confirmation, and error handling.

### Open-source project

- Added the MIT license to the repository, package metadata, and application
  bundle resources.
- Added a security policy with private vulnerability reporting, updated the
  contribution guide, and clarified installation and development commands.
- Added the missing `pnpm package` command and documented distribution status
  without implying unverified package-manager availability.

### Distribution

- Linux: unsigned AppImage.
- macOS: unsigned Apple Silicon `.app` and `.dmg`.
- Protocol version remains `1`; application and daemon version is `0.2.0`.
- Windows, code signing, and macOS notarization remain out of scope.

See [docs/install.md](docs/install.md),
[docs/KNOWN_ISSUES.md](docs/KNOWN_ISSUES.md), and
[docs/RELEASE_CHECKLIST.md](docs/RELEASE_CHECKLIST.md).

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
