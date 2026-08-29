# Session 09 — tests, CI, and packaging

Quality, CI, and release-engineering pass for Beta v0.1. No vendor CLIs are
required to validate the tree.

## Workflows

GitHub Actions: [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml).

- **Quality** matrix: `ubuntu-24.04` and `macos-15` (Apple Silicon).
  Frontend lint, typecheck, tests, production build; `cargo fmt --check`;
  clippy with warnings denied; locked fake-agent build; `cargo test
  --workspace --locked`; `cargo doc` with `-Dwarnings`.
- **Tauri smoke** matrix: same OS list. Installs Tauri system packages on
  Linux (GTK 3, WebKitGTK 4.1, `patchelf`, `pkg-config`, …) then
  `tauri build --debug --no-bundle`.
- Cache: pnpm store via `setup-node`, Rust via `Swatinem/rust-cache` with a
  shared key per OS so quality and smoke can reuse compiled artifacts. System
  packages are always installed on Linux; cache cannot hide a missing
  WebKit/GTK dependency. `--frozen-lockfile` and `--locked` reject missing
  lockfile entries.
- `continue-on-error` is not used.
- No secrets, signing identities, or notarization credentials.

## Tests

| Area | Location | Notes |
|---|---|---|
| CommandSpec / redaction / errors | `crates/core` | NUL rejection; Debug omits secrets; API error JSON |
| AgentAdapter / registry | `crates/agents/tests/adapters.rs` | PATH detection; custom args; missing cwd |
| Storage / migrations / reload | `crates/storage` | WAL, FKs, repository round-trip |
| Git / worktrees | `crates/git/tests/worktrees.rs` | dirty removal refused before clean remove |
| SessionManager / PTY / stop / restart | `crates/session/tests/pty.rs` | fake agent; SIGTERM ignore; unicode; bulk output |
| Acceptance flow | `crates/e2e/tests/main_flow.rs` | repo → worktree → PTY → Git status → reload → dirty protect |
| Frontend workspace | `apps/desktop/src/app/AppShell.test.tsx` | projects, sessions, create dialog, status, terminal subscribe/unsubscribe, grid, palette, errors, disconnected daemon, destructive confirm |

PTY waits use a 10 second ceiling and 25 ms polls. They do not assume
sub-100 ms agent latency.

## Commands

Documented in the README and implemented in root `package.json` plus
`scripts/check.sh`:

- `pnpm lint:frontend`
- `pnpm typecheck:frontend`
- `pnpm test:frontend`
- `pnpm build:frontend`
- `pnpm fmt:rust` (`cargo fmt --all -- --check`)
- `pnpm clippy`
- `pnpm doc:rust`
- `pnpm test:rust`
- `pnpm test:integration`
- `pnpm tauri:build`
- `pnpm check` (aggregator)

## Artifacts

Configured in `apps/desktop/src-tauri/tauri.conf.json`:

- Linux: AppImage, `.deb` (depends `libwebkit2gtk-4.1-0`, `libgtk-3-0`)
- macOS: `.app`, `.dmg`; identifier `com.guicybercode.climaster`; min OS 12
- Icons: existing `src-tauri/icons` set
- Signing identity: `null` (unsigned development/CI builds)

CI smoke does **not** upload packages. Release artifacts are produced locally
or by a future dedicated release workflow using `pnpm tauri:build`.

## Limitations

- `cli-masterd` is not a workspace binary yet; the session crate is the
  testable PTY owner. UI still talks to a disconnected daemon by default.
- Tauri smoke uses `--no-bundle`, so AppImage/deb/dmg generation is not
  exercised in CI. Hosts still need packaging tools for a real release.
- macOS signing and notarization are documented only.
- `cargo test --workspace` compiles the desktop crate, so a full `pnpm check`
  needs Tauri system libraries even when you only changed a Rust library.
- Fake agent is a test fixture, not a shipped product agent.
- Frontend terminal panes subscribe through a mock IPC client; xterm.js is
  not unit-tested.

## Steps before a Beta v0.1 release

1. Complete [docs/RELEASE_CHECKLIST.md](../RELEASE_CHECKLIST.md).
2. Run `pnpm check` on Linux with WebKit/GTK installed.
3. Confirm the macOS quality and Tauri smoke jobs are green.
4. Produce AppImage (and `.deb` if tooling exists) plus `.app`/`.dmg`.
5. Publish SHA-256 sums.
6. Record daemon packaging, unsigned macOS status, and PTY-across-crash
   limits in the changelog.
7. Do not add signing secrets to CI.
