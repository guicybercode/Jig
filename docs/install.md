# Install and run Jig

Jig runs locally on Linux and macOS. Choose a downloadable bundle to use the
app, or a source build to develop it. Windows is outside this Beta's scope.

## Choose a download

Use [GitHub Releases](https://github.com/guicybercode/Jig/releases) for published
versions. Download the asset for your operating system and its checksum file.
The source tree currently targets **0.2.0 Beta**, which may not yet be published.

| System | Package |
| --- | --- |
| Linux x86_64 | `Jig_<version>_amd64.AppImage` |
| macOS 12+ on Apple Silicon | `Jig_<version>_aarch64.dmg` or `Jig.app.zip` |

The [Packaging workflow](https://github.com/guicybercode/Jig/actions/workflows/packaging.yml)
also produces temporary candidate artifacts for tested commits. Download and
extract the artifact for your platform to find its packages and `SHA256SUMS`.
These are development candidates, not automatically published releases.
See [distribution status](distribution.md) for available channels.

Bundles are **unsigned and not notarized**. Checksums help detect a corrupt
or changed download; they do not replace a publisher's code signature.
Obtain both the package and checksum file from the official release or the
same official workflow run.

## Runtime requirements

- Git installed and available on `PATH`.
- Linux with the native libraries needed by the AppImage, including
  WebKitGTK 4.1, or macOS 12 or newer on Apple Silicon.
- Optional agent CLIs installed and authenticated separately. You can use
  Jig's **Shell** terminal without an agent account.

Node.js, pnpm, Rust, and a Jig account are not required to run a bundle.
Jig does not install agent CLIs or proxy their authentication. Only run agents
and custom executables you trust: worktree isolation is not an OS sandbox.

## Linux: install the AppImage

These examples use a 0.2.0 candidate. Substitute your downloaded filename
when installing another version.

1. In the download directory, calculate the checksum:

   ```bash
   sha256sum Jig_0.2.0_amd64.AppImage
   ```

   Compare the complete hash with the line for that filename in the downloaded
   checksum file. If it differs, do not run the file; download it again from
   the official source.

2. Make the file executable and launch it:

   ```bash
   chmod +x Jig_0.2.0_amd64.AppImage
   ./Jig_0.2.0_amd64.AppImage
   ```

3. The Jig window should open and connect to its bundled local daemon.

The AppImage includes `cli-masterd`. It does not bundle every Linux desktop
library. CI builds on Ubuntu 24.04; other distributions may need native
runtime packages or a source build. No `.deb` or `.rpm` is produced.

## macOS: install the app

1. Calculate the checksum of the file you downloaded, substituting its actual
   filename if it is not a 0.2.0 candidate:

   ```bash
   shasum -a 256 Jig_0.2.0_aarch64.dmg
   # If you downloaded the application ZIP instead:
   shasum -a 256 Jig.app.zip
   ```

   Compare the complete hash with the matching filename in the checksum file.
   If it differs, do not open the file.

2. Open the DMG and drag **Jig** to **Applications**, or extract `Jig.app.zip`
   and move `Jig.app` to Applications.
3. Launch Jig. If macOS blocks the unsigned application, review the warning
   in **System Settings → Privacy & Security**. Only use **Open Anyway** if
   you trust the official download and have verified its checksum. Do not
   disable Gatekeeper globally. See Apple's guide to
   [safely opening apps](https://support.apple.com/en-us/102445).
4. The Jig window should open and connect to its bundled daemon.

The distributed macOS build targets Apple Silicon, not Intel or Universal.
The daemon lives inside `Jig.app/Contents/MacOS/cli-masterd`; it does not
need a separate installation.

## Run from source

Use **Node.js 24**, **pnpm 11.9.0**, **stable Rust with Cargo**, and **Git**,
matching CI. The workspace declares a Rust 1.85 baseline, but the current
dependency lockfile is validated with stable Rust, not a minimum-version job.

Install Node.js and Rust using their official
[Node.js](https://nodejs.org/en/download) and
[Rust](https://www.rust-lang.org/tools/install) instructions. With Node.js
available, install the pinned package manager if needed:

```bash
npm install --global pnpm@11.9.0
```

An existing Corepack installation can use `corepack enable` and the repository's
`packageManager` pin instead. Avoid changing the lockfile to work around an
incompatible package-manager version.

### macOS build dependencies

Install the Xcode Command Line Tools and complete any installation prompt:

```bash
xcode-select --install
```

### Linux build dependencies

On Ubuntu 24.04, install the native packages used by packaging CI, plus Git
and Python 3 for the repository's scripts:

```bash
sudo apt-get update
sudo apt-get install --no-install-recommends \
  build-essential curl file git libayatana-appindicator3-dev \
  libgtk-3-dev librsvg2-dev libssl-dev libwebkit2gtk-4.1-dev \
  libxdo-dev patchelf pkg-config python3 wget
```

For other distributions, use the package names in the
[official Tauri prerequisites](https://v2.tauri.app/start/prerequisites/#linux).
The macOS tools requirement is also documented in
[Tauri's macOS prerequisites](https://v2.tauri.app/start/prerequisites/#macos).

### Clone and start Jig

```bash
git clone https://github.com/guicybercode/Jig.git
cd Jig
pnpm install --frozen-lockfile
pnpm tauri dev
```

Keep that terminal open while developing. The first compilation takes longer
than subsequent starts. A native Jig window should open; the development
command builds the daemon automatically before starting the frontend.

`pnpm dev` serves only the frontend at `http://localhost:1420`; live terminals
and native dialogs require Tauri. See
[your first terminal](../README.md#start-your-first-terminal) for adding a
project and starting a shell.

### Build a distributable bundle

From a prepared source checkout on Linux or macOS:

```bash
pnpm package
```

The command stages `cli-masterd`, runs the Tauri build, smoke-tests the bundled
daemon, and writes packages and `SHA256SUMS` to `dist/artifacts/`. It packages
for the host platform and does not sign, notarize, or publish a release.
Python 3 is required by the packaging scripts. `pnpm build` alone only builds
the frontend.

## Troubleshooting

### pnpm is missing or reports an incompatible Node version

Check `node --version` and `pnpm --version`. Use Node.js 24 and pnpm 11.9.0,
then rerun `pnpm install --frozen-lockfile` from the repository root. Do not
delete or regenerate `pnpm-lock.yaml` to bypass a version mismatch.

### The browser opens, but native operations do not work

You probably started `pnpm dev`. Run `pnpm tauri dev` and use its native window
for sessions, folder dialogs, and daemon operations.

### Linux cannot find WebKitGTK or another shared library

Install your distribution's Tauri native dependencies. Source builds need the
development packages listed above. A package built on a newer distribution
may not run on older system libraries; build from source on the target host
when necessary.

### An agent is unavailable, or the daemon cannot find Git

Check that Git and the agent executable run in your normal terminal and are
on the `PATH` available to Jig. Desktop-launcher environments can differ from
your interactive shell. For a custom executable, use its absolute path in the
terminal configuration. Open **Diagnostics** to inspect detection failures.
Never paste API keys or complete environment dumps into public issues.

### The daemon does not connect

Open **Diagnostics** first. To check a source-built daemon from the repository
root without starting a session:

```bash
cargo run -p cli-master-daemon --locked -- --version
cargo run -p cli-master-daemon --locked -- --preflight
```

For a standard macOS installation:

```bash
/Applications/Jig.app/Contents/MacOS/cli-masterd --version
/Applications/Jig.app/Contents/MacOS/cli-masterd --preflight
```

A 0.2.0 build prints `cli-masterd 0.2.0 (protocol 1)`. Preflight checks local
directories, Git, and optional agent CLIs. Missing optional agents do not mean
Jig itself failed to install. The sidecar is not automatically added to `PATH`.
See [daemon discovery](desktop/daemon-sidecar.md) for binary locations and the
explicit `CLI_MASTERD` override.

### A terminal remains running after closing the window

This is intentional. Reopen Jig and use the session's **Stop process** action
to stop it. Removing a canvas card also leaves the session running. If the
daemon crashes, lost terminal handles cannot be reattached and affected rows
become `unknown`. Do not try to recover by killing stored PIDs. See the
[known issues](KNOWN_ISSUES.md).

## Signing and support

The configuration enables Hardened Runtime, but no Developer ID signing
identity, notarization, or stapling is configured. Treat distributed macOS
bundles as unsigned. No Apple credentials are stored in the repository.

Report ordinary installation bugs through
[GitHub Issues](https://github.com/guicybercode/Jig/issues), including your
version, operating system, architecture, and sanitized error. Follow
[SECURITY.md](../SECURITY.md) for security reports.
