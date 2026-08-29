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
> architecture, a buildable Tauri/React shell, backend crates, and a
> cross-platform quality gate. Do not treat `main` as a finished release.

## Supported platforms

- Linux is a first-class target. AppImage is the initial package format; a
  Debian package is configured and follows when the packaging path is reliable.
- macOS is a first-class target on Apple Silicon. The artifacts are an
  application bundle and DMG.
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
       ├── PTY session manager ── Codex / Claude / Gemini / OpenCode / fake agent
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

Linux needs GTK 3, WebKitGTK 4.1, `patchelf`, and `pkg-config`. Follow the
[official Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) and
the extra packages listed in [docs/PACKAGING.md](docs/PACKAGING.md).

macOS needs Xcode Command Line Tools. Apple Silicon is the supported
architecture. CLI Master itself does not require Homebrew.

Coding-agent CLIs (Codex, Claude, Gemini, OpenCode) are **not** required to
build, lint, or test. Use the in-repo fake agent for interactive flows.

## Set up on Linux

```bash
corepack enable
pnpm install --frozen-lockfile
sudo apt-get install --no-install-recommends \
  build-essential curl file libayatana-appindicator3-dev libgtk-3-dev \
  librsvg2-dev libssl-dev libwebkit2gtk-4.1-dev libxdo-dev patchelf \
  pkg-config wget
```

Package names vary by distribution. If `pnpm check` fails on `gdk-3.0` or
`webkit2gtk-4.1`, the Tauri system dependencies are incomplete. Cache of Cargo
artifacts will not supply those libraries.

## Set up on macOS

```bash
corepack enable
pnpm install --frozen-lockfile
xcode-select --install
```

GUI apps see a reduced `PATH`. Adapters resolve executables through
`LaunchEnvironment`, not through an interactive shell. Use an absolute
executable or the fake agent when developing without vendor CLIs.

## Commands

| Task | Command |
|---|---|
| Frontend lint | `pnpm lint:frontend` |
| Frontend typecheck | `pnpm typecheck:frontend` |
| Frontend test | `pnpm test:frontend` |
| Frontend build | `pnpm build:frontend` |
| Rust format check | `pnpm fmt:rust` |
| Rust clippy | `pnpm clippy` |
| Rust tests | `pnpm test:rust` |
| Integration / acceptance | `pnpm test:integration` |
| Tauri package build | `pnpm tauri:build` |
| Full local gate | `pnpm check` |

`pnpm check` runs `scripts/check.sh`: frontend lint, typecheck, tests, and
production build, then `cargo fmt --check`, clippy with warnings denied, a
locked fake-agent build, and `cargo test --workspace --locked`.

## Tests

Tests are headless and do not call vendor CLIs.

- Frontend: Vitest + Testing Library. Mock the project-owned IPC client.
- Backend: `cargo test --workspace --locked`.
- Git/worktree tests use temporary repositories and the system `git`.
- PTY tests spawn `cli-master-fake-agent`.
- `crates/e2e` covers the main flow: register a repo, create a worktree, start
  the fake agent, write input, read output, inspect Git status, stop, reload
  metadata, refuse dirty worktree removal, then remove a clean worktree.

Build the fixture before a focused PTY test:

```bash
cargo build -p cli-master-fake-agent --locked
cargo test -p cli-master-session --locked
```

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

## Build and package

```bash
pnpm tauri:build
```

Tauri produces the formats configured for the current OS: AppImage and `.deb`
on Linux; `.app` and `.dmg` on macOS. Signing and notarization are not part of
the development build. See [docs/PACKAGING.md](docs/PACKAGING.md) and
[docs/RELEASE_CHECKLIST.md](docs/RELEASE_CHECKLIST.md).

## Fake agent

`crates/fake-agent` is a deterministic interactive CLI used by tests. It is
not a product agent.

```bash
cargo run -p cli-master-fake-agent -- --banner hello --prompt
```

Useful flags:

- `--banner TEXT` / `--no-banner`
- `--prompt` / `--no-input`
- `--reply ack:{input}`
- `--sleep-ms N`
- `--stream-lines N` / `--stream-interval-ms N`
- `--unicode`
- `--bytes N`
- `--exit-code N`
- `--ignore-sigterm`
- `--hold`
- `--report-cwd` / `--report-args` (default on; environment is never printed)

A custom adapter can point at the compiled binary:

```text
target/debug/cli-master-fake-agent
```

## Create an adapter

Built-in adapters live in `crates/agents` and implement `AgentAdapter`:

1. Stable key (`codex`, `claude`, `gemini`, `opencode`, or a custom key).
2. `detect` against an explicit `LaunchEnvironment` (never the full process
   environment).
3. `build_command` returning a `CommandSpec` (executable + argument array +
   cwd + optional env overrides). No shell strings.

Custom agents use `CustomAgentDefinition`: an absolute path or a bare
executable name, an ordered argument array, and non-secret overrides. Relative
paths with a directory separator are rejected. Register the adapter with
`AgentRegistry` and persist the definition through storage — never by
interpolating a command line.

For tests, register the fake agent as a custom adapter instead of installing a
vendor CLI.

## Troubleshooting

- **`gdk-3.0` / WebKit not found** — install Tauri Linux packages. Cargo cache
  does not include system libraries.
- **`cli-master-fake-agent was not built`** — run
  `cargo build -p cli-master-fake-agent --locked` or `pnpm check`.
- **PTY tests hang** — they wait up to 10 seconds for output. If they fail,
  inspect the snapshot in the timeout error; do not shorten the wait.
- **Dirty worktree cannot be removed** — that is required behavior. Clean the
  tree or pass an explicit `allow_dirty` confirmation after
  `prepare_remove`.
- **Vendor CLI not detected in the desktop app** — GUI `PATH` is incomplete.
  Use an absolute executable or a custom search directory.
- **Unsigned macOS app** — expected for local and CI unsigned builds.

## Repository layout

```text
apps/desktop/                  React, TypeScript, Vite, and Tauri bridge
crates/                        domain, storage, agents, git, session, fake-agent
scripts/check.sh               full local quality gate
docs/PACKAGING.md              Linux and macOS artifacts
docs/RELEASE_CHECKLIST.md      Beta v0.1 ship list
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
