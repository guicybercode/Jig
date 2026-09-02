# Release checklist for Beta v0.2

Do this before anyone treats a build as the Beta drop. Do not skip a step by
marking CI `continue-on-error`. Do not publish a GitHub Release from automation
in this repository.

Record the candidate commit, package checksum, operating system, and tester
with the results.

## Prerequisite

The E2E acceptance flow for sessions and Git worktrees should already be green
on the branch being packaged. Packaging CI smoke-tests the **bundle**
(`cli-masterd` inside AppImage / `.app`); it does not replace the real packaged
GUI acceptance below.

## Versioning

- [ ] `scripts/check-versions.sh` passes.
- [ ] `package.json`, `apps/desktop/package.json`, `tauri.conf.json`,
      `Cargo.toml`, and `protocol/catalog.json` `applicationVersion` are
      `0.2.0`.
- [ ] `protocol/catalog.json` `protocolVersion` is `1`.
- [ ] `cli-masterd --version` prints `cli-masterd 0.2.0 (protocol 1)`.
- [ ] `Cargo.lock` and `pnpm-lock.yaml` are committed.

## Quality gate

- [ ] `pnpm check:versions`
- [ ] `pnpm --filter @cli-master/desktop check`
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings`
- [ ] `cargo test --workspace --locked` on Linux
- [ ] `cargo test --workspace --locked` on macOS (CI `macos-15`)

## Linux package

- [ ] WebKitGTK 4.1, GTK 3, `patchelf`, and `pkg-config` are installed on
      the build host.
- [ ] `pnpm package` produced an AppImage.
- [ ] `scripts/smoke-bundle.sh` found `cli-masterd` inside the AppImage.
- [ ] Bundled `cli-masterd --preflight` reports Git available.
- [ ] No `.exe`, NSIS, or MSI files were attached.

## macOS package

- [ ] Built on Apple Silicon.
- [ ] `.app` contains `Contents/MacOS/cli-masterd`.
- [ ] `.dmg` exists, or the `.app` zip is attached and the missing DMG is
      recorded in the notes.
- [ ] `signingIdentity` is still `null` unless a human signed locally.
- [ ] Gatekeeper unsigned-app behavior is mentioned in the notes.

## Artifacts

- [ ] AppImage attached for Linux.
- [ ] `.dmg` and/or `.app.zip` attached for macOS.
- [ ] `SHA256SUMS` sits next to those files.
- [ ] Checksums were regenerated after the last rebuild.

## Manual package acceptance

Every item below starts unchecked. A Vite/Playwright smoke run or a Rust
acceptance test is useful evidence, but neither substitutes for exercising the
real packaged GUI and bundled daemon.

- [ ] Install or mount the actual release-candidate package, launch Jig,
      and confirm the packaged desktop connects to its bundled daemon.
- [ ] Add an existing Git repository through **Repository path** and confirm
      that its path and current branch are displayed correctly.
- [ ] Create two sessions for that repository and confirm that each uses its
      own managed worktree and appears as a separate live terminal.
- [ ] Type in both terminals, resize a terminal, then stop one session and
      confirm that the other terminal remains interactive.
- [ ] Disconnect and reconnect the desktop while the daemon remains running;
      confirm that both sessions remain listed and retained output is replayed.
- [ ] Make a worktree dirty, attempt to remove it, and confirm removal is
      blocked without deleting or modifying the user's file.
- [ ] Record all failures and unresolved platform limitations in the release
      notes before attaching or publishing the package.

See [KNOWN_ISSUES.md](KNOWN_ISSUES.md) for limitations that are expected in
the Beta and must not be reported as successful manual acceptance.

## Documentation

- [ ] [CHANGELOG.md](../CHANGELOG.md) matches what testers will see.
- [ ] [install.md](install.md), [first-use.md](first-use.md),
      [backup-and-recovery.md](backup-and-recovery.md),
      [troubleshooting.md](troubleshooting.md), and
      [uninstall.md](uninstall.md) are current.
- [ ] Signing and notarization are described as **not configured**.

## Stop here

- [ ] No certificate, notary API key, or `TAURI_` secret was added.
- [ ] No `gh release create` / marketplace publish was run from CI.
- [ ] A person downloads the workflow artifacts, verifies checksums, and
      only then decides whether to attach them to a GitHub Release by hand.
