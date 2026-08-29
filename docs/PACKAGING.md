# Packaging for Beta v0.1

CLI Master ships two Rust executables: the Tauri desktop process and
`cli-masterd`. The daemon is a Tauri `externalBin` sidecar named
`cli-masterd-<target-triple>` under `apps/desktop/src-tauri/binaries/`.
Those staged files are gitignored.

## Commands

```bash
bash scripts/check-versions.sh
bash scripts/stage-sidecar.sh          # release cli-masterd
bash scripts/stage-sidecar.sh --debug  # debug cli-masterd
pnpm tauri:build                       # stage + tauri build
pnpm package                           # build, smoke the bundle, checksums
```

`pnpm package` writes `dist/artifacts/` plus `SHA256SUMS`. It does not
create a GitHub Release.

`cargo clippy --workspace` and `cargo test --workspace` need
`apps/desktop/src-tauri/binaries/cli-masterd-<triple>` to exist. Quality CI
stages a real debug sidecar. A debug compile can also write a stub if the
file is missing. Release `tauri build` still requires `scripts/stage-sidecar.sh`.

## Bundle targets

Configured in `apps/desktop/src-tauri/tauri.conf.json`:

| Host | Artifact |
|---|---|
| Linux | AppImage |
| macOS | `.app` and `.dmg` |
| Windows | Not built |

Identifier: `com.guicybercode.climaster`. Category: DeveloperTool.

`cli-masterd` is copied next to the desktop binary inside the AppImage and
inside `Contents/MacOS` on macOS. `scripts/smoke-bundle.sh` fails if that
file is missing, then runs `cli-masterd --version` and `--preflight`.

## Linux notes

- AppImage is the supported Beta format. `.deb` is not produced.
- `patchelf` must be installed on the build machine.
- The bundle does not vendor WebKitGTK. Testers still need WebKitGTK 4.1.
- XDG directories are created at `0700`. See [install.md](install.md).

## macOS notes

- CI builds on `macos-15` (Apple Silicon).
- `bundle.macOS.signingIdentity` is `null`.
- Hardened Runtime is off until a human sets a real Developer ID.
- Gatekeeper warnings on first launch are expected for unsigned builds.

## CI

`.github/workflows/packaging.yml` runs on pull requests, `main`, and
`workflow_dispatch`. Each OS job uploads
`cli-master-v0.1.0-<platform>` with the installable files and checksums.
Retention is 14 days.

The workflow has `contents: read` only. It cannot publish a Release and it
does not receive signing secrets.

## Signing and notarization (not in this repository)

Stop before any step that needs a secret or an Apple account:

1. Set `bundle.macOS.signingIdentity` locally to a Developer ID Application
   identity you already have.
2. Notarize through Apple's official notary flow.
3. Staple the ticket to the `.app` or `.dmg`.
4. Record the identity name in the release notes. Never commit the key.

Linux AppImage signing is a separate, optional follow-up. Unsigned
AppImages are valid for this Beta as long as checksums are published.

## Version lock

`scripts/check-versions.sh` and the desktop/core tests require:

- `Cargo.toml` `[workspace.package].version`
- root `package.json`
- `apps/desktop/package.json`
- `apps/desktop/src-tauri/tauri.conf.json`
- `protocol/catalog.json` `applicationVersion`

to equal `0.1.0`, with `protocolVersion` equal to `1`.
