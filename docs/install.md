# Install CLI Master Beta v0.1

CLI Master is a local-first desktop app for Linux and macOS. Windows is out
of scope for this Beta.

Installable artifacts come from GitHub Actions on this repository, not from
an app store. Builds are **unsigned**. macOS notarization is not configured.

## What you need

- Git on `PATH`. The daemon refuses to start a useful preflight without it.
- A Linux distribution with WebKitGTK 4.1, or macOS 12 or newer.
- Optional: Codex, Claude Code, Gemini CLI, or OpenCode. The app starts
  without them. Sessions that need a missing CLI fail later, with an
  actionable error.

Do not put vendor API tokens into CLI Master. Each agent CLI keeps its own
login.

## Linux (AppImage)

1. Download the `*.AppImage` and `SHA256SUMS` files from the packaging
   workflow artifacts.
2. Verify the checksum:

   ```bash
   sha256sum -c SHA256SUMS
   ```

3. Mark the AppImage executable and run it:

   ```bash
   chmod +x "CLI Master_0.1.0_amd64.AppImage"
   ./CLI\ Master_0.1.0_amd64.AppImage
   ```

The AppImage includes `cli-masterd` next to the desktop binary. It does not
vendor your whole desktop stack. If the window fails to open, install the
WebKitGTK 4.1 packages for your distribution. See
[the Tauri Linux prerequisites](https://v2.tauri.app/start/prerequisites/).

Debian packages are not a Beta v0.1 distribution format. AppImage is.

## macOS (application bundle and DMG)

Apple Silicon is the CI target. Universal binaries are not produced.

1. Download the `.dmg` or `CLI Master.app.zip` plus `SHA256SUMS`.
2. Verify the checksum:

   ```bash
   shasum -a 256 -c SHA256SUMS
   ```

3. Open the DMG and drag CLI Master to Applications, or unzip the `.app`.
4. Launch it from Applications.

The first launch of an unsigned app is blocked by Gatekeeper. That is
expected. Open System Settings, Privacy & Security, and allow the app after
you have verified the checksum yourself. Notarization would remove this
step. It is not wired up, and this repository does not contain certificates.

`cli-masterd` lives in `CLI Master.app/Contents/MacOS/cli-masterd`.

## Confirm the daemon binary

From a terminal, after install:

```bash
# Linux AppImage, after --appimage-extract, or from a source build:
cli-masterd --version
cli-masterd --preflight
```

`--version` must print `cli-masterd 0.1.0 (protocol 1)`. `--preflight` must
report Git as available. Missing Codex or Claude is fine.

## Build from source

Development setup is in the [README](../README.md). To produce the same
unsigned artifacts CI uploads:

```bash
pnpm install --frozen-lockfile
pnpm package
```

`pnpm package` stages `cli-masterd`, runs `tauri build`, smoke-tests the
bundle, and writes `dist/artifacts/SHA256SUMS`. It never signs, notarizes,
or creates a GitHub Release.

## Signing status

| Step | Status |
|---|---|
| Application version `0.1.0` | Locked across Cargo, npm, Tauri, and the protocol catalog |
| Linux AppImage | Built unsigned |
| macOS `.app` / `.dmg` | Built unsigned, `signingIdentity` is `null` |
| Hardened Runtime | Off until a Developer ID exists |
| Notarization / stapling | Not configured. No Apple API key is stored in CI |
| GitHub Release publish | Not automated. Upload artifacts by hand after review |

Do not add certificates, API keys, or `TAURI_*` signing secrets to this
repository to "finish" the Beta.
