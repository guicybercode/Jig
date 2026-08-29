# Changelog

## 0.1.0-beta.1 - 2026-08-29

First Beta v0.1 cut for Linux and macOS. Windows is out of scope.

### Added

- `cli-masterd` Unix-socket daemon owning SQLite, Git worktrees, agent
  adapters, and PTY sessions.
- Built-in adapters for Codex, Claude Code, Gemini CLI, and OpenCode, plus
  custom executables with structured argument lists.
- Dirty worktree protection: removal requires an explicit confirmation token
  and `allowDirty` when Git reports changes.
- Desktop workspace for projects, sessions, Git status/diff, custom agents,
  and a 1-4 terminal grid.
- Linux packages embed `cli-masterd` next to the desktop binary so an
  installed AppImage or `.deb` can start the daemon without a PATH install.
- Linux and macOS CI quality jobs for format, clippy, tests, and docs.

### Changed

- `AgentId` is a stable registry key (`codex`, custom keys) rather than a UUID
  newtype, matching storage and adapters.

### Fixed

- Session manager kills live process groups on drop and waits for stop so Git
  can remove a worktree afterwards.
- Nested SQLite mutex use in session deletion no longer deadlocks.
- Timed-out Git commands no longer hang forever while joining the worker
  thread.
- PTY output is published on each read so interactive input is not stuck
  waiting for a later chunk.
