# Release checklist

Use this list for a Beta v0.1 cut. Do not tick an item that was not run.

## Identity

- [x] Workspace version, npm versions, and `tauri.conf.json` match
      (`0.1.0-beta.1` or the next pre-release).
- [x] `cli-master-daemon::APP_VERSION` matches that string.
- [x] `CHANGELOG.md` has an entry for this version.
- [x] `docs/KNOWN_ISSUES.md` is current.

## Linux local gate

Run from the repository root:

```bash
pnpm install --frozen-lockfile
pnpm --filter @cli-master/desktop check
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

- [x] Frontend typecheck, vitest, and Vite build passed.
- [x] Rustfmt, clippy `-D warnings`, and workspace tests passed.
- [x] `cargo test -p cli-master-daemon --test acceptance` passed.

Recorded outcomes live in [BETA_V0.1_ACCEPTANCE.md](BETA_V0.1_ACCEPTANCE.md).

## macOS

- [ ] CI `Quality (macos-15)` is green, or the same commands were run on a Mac.
- [x] Do not claim a local macOS GUI run unless one happened.

## Daemon acceptance (no GUI)

Covered by `crates/daemon/tests/acceptance.rs`:

- [x] Add Git repo, custom agent, PTY write/read, resize, two sessions, stop
      isolation, dirty worktree block, persist + reconcile, project remove
      keeps the directory.

## Desktop GUI

- [ ] `pnpm tauri dev` opens the window.
- [ ] Add Project with a real path shows name, path, and branch.
- [ ] Custom agent registers and can start a session.
- [ ] Two terminals can be shown; stopping one leaves the other.
- [ ] Dirty worktree removal is blocked in the Git panel.

## Packages

Linux (from a Linux builder):

```bash
bash apps/desktop/src-tauri/scripts/stage-daemon.sh release
pnpm tauri build
```

- [x] AppImage exists under `target/release/bundle/appimage/` or the Tauri
      output directory.
- [x] `.deb` exists under `target/release/bundle/deb/` when that target is
      enabled.

macOS (from a macOS builder):

- [ ] `.app` and `.dmg` exist under the Tauri bundle output.

If a platform is missing, record "not produced" in the acceptance report. Do
not copy artifacts from another OS.

## Ship

- [ ] Tag `v0.1.0-beta.1` only after the checklist above is honest.
- [ ] Attach packages that were actually built.
