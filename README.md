# Jig

Terminals on a canvas. A local-first desktop workspace for coding-agent CLIs.

[Releases](https://github.com/guicybercode/Jig/releases) ·
[Installation guide](docs/install.md) · [Contributing](CONTRIBUTING.md) ·
[Security](SECURITY.md) · [MIT license](LICENSE)

![Jig — terminals on a canvas](docs/brand/social-preview.png)

## What is Jig?

Jig brings real terminals, project folders, Git worktrees, notes, and browser
cards into one canvas. Run Codex, Claude Code, Gemini CLI, OpenCode, a shell,
or your own executable, and keep related work together.

Jig is not a coding agent or a vendor proxy. No Jig account is required. Agent
CLIs are installed separately and keep their own authentication and network
connections. Git worktrees separate working copies; they are not a security
sandbox for the programs you launch.

The source tree targets **Beta v0.2.0** for Linux and macOS. Read the
[known limitations](docs/KNOWN_ISSUES.md) before using it for important work.

## Download and install

Use [GitHub Releases](https://github.com/guicybercode/Jig/releases) for published
builds. The [Packaging workflow](https://github.com/guicybercode/Jig/actions/workflows/packaging.yml)
provides temporary candidate artifacts; an Actions build is not a published
release. The version in this source tree may be newer than the latest release.

| Platform | Architecture | Package format |
| --- | --- | --- |
| Linux | x86_64 | AppImage |
| macOS 12+ | Apple Silicon (aarch64) | DMG or zipped `.app` |
| Windows | — | Not supported in this Beta |

See [package availability](docs/distribution.md) for distribution channels and
Linux repository status. Do not assume a similarly named package is this Jig.

Download the artifact for your platform and its checksum file, verify it,
then follow the [installation guide](docs/install.md). Builds are **unsigned
and not notarized**; macOS may block the first launch. Git must be installed
and available on `PATH`. Node.js and Rust are needed only for source builds.

## Run from source

### Prerequisites

- **Node.js 24**, matching CI, and **pnpm 11.9.0**, pinned in `package.json`.
- **Rust stable** and Cargo. CI uses current stable Rust, not a separate
  minimum-version job for the workspace's declared Rust 1.85 baseline.
- **Git** on `PATH`.
- **macOS:** Xcode Command Line Tools (`xcode-select --install`).
- **Linux:** Tauri native dependencies, including WebKitGTK 4.1. See the
  [Ubuntu setup commands](docs/install.md#linux-build-dependencies) or the
  [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your distribution.

If pnpm is not installed, install the pinned version through your Node.js
installation:

```bash
npm install --global pnpm@11.9.0
```

If you already manage pnpm with Corepack, `corepack enable` lets it use the
repository's `packageManager` pin. See the
[pnpm installation guide](https://pnpm.io/installation) for alternatives.

### Start the desktop application

```bash
git clone https://github.com/guicybercode/Jig.git
cd Jig
pnpm install --frozen-lockfile
pnpm tauri dev
```

The first run compiles the Rust application and daemon, which can take several
minutes. A native **Jig** window should open and connect to the local daemon.
You do not need to start the daemon separately.

`pnpm dev` starts only the Vite frontend at `http://localhost:1420`. A normal
browser does not provide the native desktop bridge or live daemon sessions.
Use `pnpm tauri dev` to run the actual app.

### Start your first terminal

1. Click the sidebar **+** (**Add workspace project**), choose a local Git
   repository with **Choose a project folder**, then click **Add Project**.
2. Select that project and click **Add terminal card** in the canvas toolbar.
3. Choose **Shell** to try Jig without an agent account, or choose an agent CLI
   you have already installed and authenticated. Check the working directory.
4. Choose **Use project working copy**, or **Create an isolated Git worktree**
   for a separate branch and checkout. The latter requires a repository with
   at least one commit.
5. Click **Create terminal**. With the daemon connected, the terminal starts
   on the canvas. If the card is still a draft, click **Start terminal**.

Closing the window or removing a canvas card does **not** stop its session.
Use the terminal's session actions and **Stop process** to stop the running
program. If startup fails, check **Diagnostics** and the
[troubleshooting guide](docs/install.md#troubleshooting).

### Build installable packages

From the repository root, on the platform you want to package:

```bash
pnpm package
```

This builds and stages `cli-masterd`, builds the desktop bundle, smoke-tests
the bundled daemon, and writes packages plus `SHA256SUMS` to `dist/artifacts/`.
It does not sign, notarize, or publish a release. `pnpm build` builds only the
frontend; it does not produce an installable desktop app. Python 3 is required
by the packaging scripts. See [packaging](docs/PACKAGING.md) for details.

## Development and architecture

The React + xterm.js interface talks through a Tauri 2 bridge to a per-user
Unix-socket daemon. The daemon owns PTYs, process lifecycle, Git operations,
and SQLite metadata. Closing the UI leaves the daemon running; restarting
the daemon cannot restore lost PTY handles.

| Location | Responsibility |
| --- | --- |
| `apps/desktop/` | React interface, typed IPC client, Tauri bridge, frontend tests |
| `crates/` | Rust domain, storage, Git, PTY, daemon, and runtime acceptance tests |
| `crates/core/src/wire/` | Authoritative versioned IPC contract |
| `protocol/catalog.json` | IPC catalog mirror |
| `docs/` | Architecture decisions, installation, packaging, and recovery |

Start with [CONTRIBUTING.md](CONTRIBUTING.md) for the complete development gate,
[ARCHITECTURE.md](ARCHITECTURE.md) for system boundaries, and
[AGENTS.md](AGENTS.md) for repository rules.

## Community, security, and license

Bug reports, documentation fixes, and focused pull requests are welcome.
Use [GitHub Issues](https://github.com/guicybercode/Jig/issues) for ordinary
bugs and feature proposals. Follow [SECURITY.md](SECURITY.md) to report a
vulnerability privately; do not include tokens or private terminal output
in public reports.

Jig is open source under the [MIT License](LICENSE). Dependencies and agent
CLIs retain their own licenses.
