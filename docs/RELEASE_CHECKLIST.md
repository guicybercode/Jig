# Release checklist — Beta v0.1

Use this list before tagging a Beta v0.1 build. Do not skip a step by marking
CI `continue-on-error`.

## Versioning

- [ ] `package.json`, `apps/desktop/package.json`, `apps/desktop/src-tauri/tauri.conf.json`, and `[workspace.package].version` in `Cargo.toml` share the same `0.1.x` version.
- [ ] `Cargo.lock` is committed after `cargo test --workspace --locked`.
- [ ] `pnpm-lock.yaml` is committed after `pnpm install --frozen-lockfile` succeeds.
- [ ] Git tag matches the version (`v0.1.0`).

## Migrations

- [ ] `crates/storage/migrations` remains additive. Destructive SQL is rejected for this Beta.
- [ ] `cargo test -p cli-master-storage --locked` passes, including reload-from-disk.
- [ ] A copy of a v1 database still opens with `Storage::migrate`.
- [ ] Known migration backup behavior is described in ARCHITECTURE.md; no production database was rewritten by a test.

## Tests

- [ ] `pnpm check` passes on a machine with Tauri system dependencies.
- [ ] Frontend lint, typecheck, unit tests, and production build passed.
- [ ] `cargo test --workspace --locked` passed on Linux.
- [ ] `cargo test --workspace --locked` passed on macOS (Apple Silicon CI job).
- [ ] Tests used `cli-master-fake-agent`, not Codex, Claude, Gemini, or OpenCode.
- [ ] Dirty worktree removal was proven to fail before any clean removal in the acceptance test.

## Build Linux

- [ ] Tauri prerequisites installed (WebKitGTK 4.1, GTK 3, `patchelf`, `pkg-config`).
- [ ] `pnpm tauri:build` produced an AppImage.
- [ ] `.deb` built when `dpkg` tooling is present; otherwise the absence is recorded in the release notes.
- [ ] Runtime depends include `libwebkit2gtk-4.1-0` and `libgtk-3-0`.
- [ ] Desktop entry and icons are present in the bundle.

## Build macOS

- [ ] Built on Apple Silicon (CI `macos-15` or equivalent).
- [ ] `.app` exists and launches.
- [ ] `.dmg` exists when the bundler ran to completion.
- [ ] Bundle identifier is `com.guicybercode.climaster`.
- [ ] Agent PATH limitations are documented for testers (GUI `PATH` vs login shell).

## Smoke tests

- [ ] Linux smoke: start the app, add a temporary Git repo, start the fake agent, type input, see output, stop the session.
- [ ] macOS smoke: same flow.
- [ ] Closing the window does not claim that sessions survive a daemon crash (they do not).
- [ ] A dirty worktree cannot be removed from the UI confirmation path without an explicit confirm after status review.

## Artifacts

- [ ] AppImage and/or `.deb` attached.
- [ ] `.app` and/or `.dmg` attached.
- [ ] No Windows artifacts.
- [ ] Fake-agent is **not** shipped as a user-facing product binary; it stays a test fixture.

## Checksums

- [ ] SHA-256 file published next to each artifact.
- [ ] Checksums regenerated after any rebuild; old sums discarded.

## Changelog

- [ ] User-facing changelog lists setup, known gaps, and adapter coverage.
- [ ] Internal session reports (`docs/codex/`) are not a substitute for the changelog.

## Known issues

- [ ] Daemon sidecar packaging status is stated.
- [ ] PTY reattachment across daemon restarts is listed as unsupported.
- [ ] Signing and notarization status is stated.

## Future signing

- [ ] No secrets were added to the repository or to CI for this tag.
- [ ] `bundle.macOS.signingIdentity` is still `null` unless a human release manager set a real identity locally.
- [ ] Notarization remains a manual follow-up. See [PACKAGING.md](PACKAGING.md).
