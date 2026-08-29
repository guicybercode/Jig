# Packaging — Beta v0.1

CLI Master ships two Rust executables: the Tauri desktop process and, later,
`cli-masterd`. Beta v0.1 packaging currently covers the desktop binary. The
daemon sidecar is not bundled yet.

Configured bundle targets live in
[`apps/desktop/src-tauri/tauri.conf.json`](../apps/desktop/src-tauri/tauri.conf.json).
The identifier is `com.guicybercode.climaster`. Icons are under
`apps/desktop/src-tauri/icons/`.

Build artifacts on the current OS:

```bash
pnpm tauri:build
```

CI smoke-builds with `--debug --no-bundle`. That compiles the frontend and the
Tauri crate without producing AppImage, `.deb`, `.app`, or `.dmg` files.

## Linux

First-class formats:

- **AppImage** — initial distribution format. Built when `bundle.targets`
  includes `appimage`. Runtime still needs a working WebKitGTK 4.1 stack on the
  host; the bundle does not vendor the entire desktop environment.
- **Debian package** — configured with section `devel` and runtime depends
  `libwebkit2gtk-4.1-0` and `libgtk-3-0`. Produce it only when `dpkg` tooling
  is available.

Install the [Tauri 2 Linux prerequisites](https://v2.tauri.app/start/prerequisites/)
before a package build. CLI Master CI additionally installs `libgtk-3-dev`,
`patchelf`, and `pkg-config`. AppImage linking uses `patchelf`; do not skip it
on the build machine.

Desktop integration:

- Tauri writes a `.desktop` entry from the product name and identifier.
- Icons are the PNG/ICNS set already in `src-tauri/icons`.
- The application is a local-first desktop tool; it does not install a
  system daemon unit in v0.1.

XDG paths the daemon will use once packaged (see ARCHITECTURE.md):

| Kind | Location |
|---|---|
| Database | `$XDG_DATA_HOME/cli-master/cli-master.db`, else `~/.local/share/cli-master/cli-master.db` |
| Runtime socket | `$XDG_RUNTIME_DIR/cli-master` when available |
| Config | `$XDG_CONFIG_HOME/cli-master` |

Directories must be user-owned with mode `0700`. Do not place the Unix socket
on a shared tmpfs without that check.

WebKit notes:

- Build against WebKitGTK 4.1 (`libwebkit2gtk-4.1-dev`).
- Wayland and X11 are both acceptable; the architecture does not assume X11.
- Missing WebKit at runtime is a host package problem, not an application
  configuration problem.

## macOS

First-class formats:

- **`.app`** — required.
- **`.dmg`** — configured (window layout in `tauri.conf.json`). Built when the
  host can run the Tauri bundler.

Apple Silicon is the CI target (`macos-15`). Universal binaries are not
produced in v0.1.

Bundle metadata:

- Identifier: `com.guicybercode.climaster`
- Minimum system version: `12.0`
- Hardened Runtime: enabled in config so a future signed build can notarize
- Icons: `icon.icns`

GUI apps receive a reduced `PATH`. Agent detection uses
`LaunchEnvironment` (daemon PATH + standard Unix locations + user search
directories). Users can register a custom adapter with an absolute executable
or import PATH from a login shell later; Homebrew is not required.

### Signing and notarization (pending)

These steps are **not** automated and must not run without real credentials:

1. Set `bundle.macOS.signingIdentity` to a Developer ID Application identity.
2. Provide notarization credentials through the official Apple notary flow
   (App Store Connect API key or equivalent). Do not commit secrets.
3. Staple the ticket to the `.app` / `.dmg`.
4. Record the signing identity name in the release notes, never the key.

Unsigned local builds are valid for development. CI does not sign.

## Checksums

After a release build, publish SHA-256 sums next to the artifacts:

```bash
shasum -a 256 CLI\ Master.appimage cli-master_0.1.0_amd64.deb CLI\ Master.dmg
```

Do not treat a missing checksum file as optional for a tagged Beta.

## What is not packaged yet

- `cli-masterd` sidecar alongside the `.app` / AppImage
- Signed and notarized macOS artifacts
- RPM
- Windows
