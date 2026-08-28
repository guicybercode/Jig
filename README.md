# CLI Master

CLI Master is a local-first desktop control center for running multiple coding
agent CLIs. It coordinates projects, interactive terminal sessions, Git
worktrees, and agent processes without replacing the agents themselves.

The Beta v0.1 target supports these installed CLIs through adapters:

- OpenAI Codex
- Claude Code
- Gemini CLI
- OpenCode
- custom executables with structured argument lists

CLI Master does not require an account, cloud backend, telemetry service, or
API server. Authentication remains with each CLI installed on the machine.

> [!IMPORTANT]
> Beta v0.1 is under active development. The repository currently contains the
> architecture and the first buildable Tauri/React foundation. Do not treat the
> current `main` branch as a finished release.

## Supported platforms

- Linux is a first-class target. AppImage is the initial package format; a
  Debian package follows when the packaging path is reliable.
- macOS is a first-class target on Apple Silicon and supported modern releases.
  The initial artifacts are an application bundle and DMG.
- Windows is explicitly outside the Beta v0.1 scope.

## How it works

```text
React + xterm.js
       │ typed Tauri commands and events
       ▼
Tauri 2 desktop bridge
       │ versioned local IPC
       ▼
CLI Master daemon
       ├── PTY session manager ── Codex / Claude / Gemini / OpenCode
       ├── Git and worktree service
       └── SQLite metadata storage
```

The separate daemon owns live PTYs and SQLite. Closing or reloading the desktop
window therefore does not inherently stop active sessions. Read
[ARCHITECTURE.md](ARCHITECTURE.md) for the protocol, schema, lifecycle, and
safety decisions.

## Prerequisites

Install the following development tools:

- Node.js 22 or newer
- Corepack and pnpm 11
- Rust 1.85 or newer, including `cargo`, `rustfmt`, and `clippy`
- Git
- Tauri 2 operating-system dependencies

Linux needs WebKitGTK and the distribution-specific packages documented in the
[official Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).
macOS needs Xcode Command Line Tools. CLI Master itself does not require
Homebrew.

At least one supported coding-agent CLI is required only when exercising real
agent sessions. The application foundation builds without one installed.

## Set up development

1. Clone the repository and enter it.
2. Enable the package manager declared by the repository.
3. Install JavaScript dependencies.

```bash
corepack enable
pnpm install --frozen-lockfile
```

The dependency policy permits install scripts only for explicitly reviewed
packages. pnpm will reject an unexpected dependency build script.

## Run the desktop app

```bash
pnpm tauri dev
```

This command starts Vite and the Tauri desktop process. It does not start an
agent until the user creates a session.

For browser-only UI development:

```bash
pnpm dev
```

Browser mode cannot exercise native dialogs, PTYs, Git, or daemon IPC.

## Validate changes

Run the repository gate before committing:

```bash
pnpm check
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

`pnpm check` type-checks and builds the frontend, then checks every Rust crate.
Platform-specific PTY and packaging tests run in their Linux and macOS CI jobs
as those subsystems land.

## Build packages

```bash
pnpm tauri build
```

Tauri creates the formats supported by the current operating system. Code
signing and macOS notarization are release concerns and are not required for a
local development build.

## Repository layout

```text
apps/desktop/                  React, TypeScript, Vite, and Tauri bridge
crates/                        Rust domain, storage, Git, PTY, and daemon crates
design-system/cli-master/      UI tokens and interaction rules
ARCHITECTURE.md                accepted architecture and protocol design
```

## Safety principles

- Commands use an executable plus an argument array, not interpolated shell
  strings.
- Removing a project never removes its repository directory.
- Worktrees with uncommitted changes are never silently deleted.
- Stopping a process and deleting session metadata are separate actions.
- Full environments, tokens, and terminal contents are excluded from logs.

See [CONTRIBUTING.md](CONTRIBUTING.md) before opening a change.
