# Session 20 report: Beta packaging and release prep

Work landed on `codex/20-beta-packaging-release` from `origin/main`. Other
open PRs were not merged into this branch.

## What this session built

- Version lock across Cargo, npm, Tauri, and `protocol/catalog.json`
  (`applicationVersion` `0.1.0`, protocol `1`)
- Linux XDG data/config/cache/state/runtime directories and macOS
  Application Support, Caches, and Logs, all `0700`, logs `0600`
- `cli-masterd --preflight` / `--version` (Git required, agent CLIs optional)
- Tauri sidecar `cli-masterd` in AppImage and `Contents/MacOS`
- Packaging CI that uploads unsigned artifacts plus `SHA256SUMS`
- Bundle smoke test that runs the packaged daemon, not only `cargo build`
- Install, first use, backup, recovery, troubleshooting, uninstall,
  packaging, and release-checklist docs

## What this session did not do

- Sign or notarize anything
- Create a GitHub Release
- Merge E2E acceptance from parallel branches
- Connect the Tauri `protocol_info` command to daemon `system.hello`
- Add Windows bundles

The packaging prerequisite ("E2E acceptance green") is still owned by the
session/PTY and GUI work on other branches. This branch smoke-tests the
bundle layout and `cli-masterd` preflight.

## Commands run here

```bash
bash scripts/check-versions.sh
cargo test -p cli-master-core --locked
cargo test -p cli-master-daemon --all-targets
cargo test -p cli-master-git --all-targets
cargo clippy -p cli-master-daemon -p cli-master-git --all-targets -- -D warnings
pnpm --filter @cli-master/desktop exec vitest run src/ipc/protocol.test.ts
bash scripts/stage-sidecar.sh --debug
apps/desktop/src-tauri/binaries/cli-masterd-x86_64-unknown-linux-gnu --version
```

This environment has no WebKitGTK, so `cli-master-desktop` and
`pnpm package` were not run here. Linux/macOS packaging CI must do that.

## Remaining risks

- Full `tauri build` is proven only in CI on this branch.
- GUI PATH vs login-shell PATH still surprises macOS testers.
- Unsigned macOS builds need a manual Gatekeeper exception.
- Sidecar location is implemented. Auto-start and IPC forwarding are not.
