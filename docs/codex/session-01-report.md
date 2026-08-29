# Session 01 report: architecture contracts

> Historical implementation report. Its proposed ID, status, and catalog
> decisions were superseded by the authoritative Beta wire contract in
> `crates/core/src/wire` and the updated ADRs. Keep it only as session history.

## Diagnosis

The repo already had a serious architecture document and a `cli-master-core`
crate. The implementation had started to disagree with both, and the gaps
were exactly the kind that make parallel Codex sessions invent a second
system.

Frontend and backend were barely coupled, which sounds healthy until you
notice they were coupled by *nothing*. Tauri still shipped `greet`. React
had no IPC module. There were no TypeScript DTOs. The next session that
needed "list projects" would have added a one-off command.

`crates/agents` was real code that `cargo test --workspace` never built.
Its adapters use catalog keys (`codex`). Core used UUID `AgentId`. That
collision would have shown up the first time someone persisted a built-in.

Session status in SQLite and in Rust omitted `created` and `stopping`.
The public `Session` DTO carried `ptyId` and `pid`. `Project` mixed
persisted path with observed branch. `Worktree` had `isDirty` while SQL
had `state`. Architecture examples used `{ ok: false }`; the crate used a
tagged `status` union.

Nothing was spawning processes from React yet. That is luck, not a
boundary. There was also no `SessionManager`, so concurrent agents were
still a wish.

## Decisions

- Keep the stack. Tauri 2, React, SQLite, portable-pty later. No rewrite.
- `cli-masterd` still owns PTYs. Documented in ADR 0001. Not implemented.
- `AgentId` is a string. Built-ins keep stable keys.
- Public session DTOs drop PID and PTY handles.
- Session machine includes `created` and `stopping`. Only the daemon
  writes status. Recovery of live rows is `unknown`.
- IPC v1 catalog is an enum plus `protocol/catalog.json` plus TypeScript
  `IPC_METHODS`. Responses keep `status: success | error`.
- `worktree.remove` is one method with an optional confirmation token
  instead of a second catalog name.
- `session.subscribe`, `session.kill`, and `diagnostics.export` wait.
  Snapshot plus live events cover reload. Extra names need a catalog
  change.
- TypeScript types are handwritten and contract-tested. No codegen crate.
- Schema version stays 1. `0001_initial.sql` now accepts the new statuses
  because nothing has shipped.

## Files modified

- `Cargo.toml`, `Cargo.lock`: agents crate in the workspace; desktop
  depends on core
- `crates/core/**`: domain DTOs, session transitions, IPC catalog, payloads
- `crates/storage/migrations/0001_initial.sql`, `crates/storage/src/lib.rs`
- `apps/desktop/src/ipc/**`, `apps/desktop/src-tauri/**`
- `protocol/catalog.json`
- `AGENTS.md`, `ARCHITECTURE.md`, `README.md`, `CONTRIBUTING.md`
- `docs/adr/0001-session-ownership.md`
- `docs/adr/0002-ipc-protocol.md`
- `docs/adr/0003-persistence-and-recovery.md`
- `docs/adr/0004-git-worktree-safety.md`
- `docs/codex/session-01-report.md`

## Commands executed

```bash
git checkout -b codex/01-architecture-contracts
rustup toolchain install stable --profile minimal --component rustfmt,clippy
cargo fmt --all
cargo clippy -p cli-master-core -p cli-master-storage -p cli-master-agents --all-targets -- -D warnings
cargo test -p cli-master-core -p cli-master-storage -p cli-master-agents
pnpm install --frozen-lockfile
pnpm --filter @cli-master/desktop check
```

`cargo test --workspace` and `cargo clippy --workspace` were not fully
run because this environment lacks GTK/WebKit (`gdk-3.0` pkg-config).
The desktop crate still type-checks on the JS side via `pnpm check`.

## Tests executed

- `cli-master-core`: 29 unit tests (28 plus catalog fixture after the
  shared JSON file)
- `cli-master-storage`: 6 tests, including `created`/`stopping` inserts
- `cli-master-agents`: 7 adapter tests, now in the workspace
- desktop: 8 Vitest tests (4 existing shell tests + 4 IPC contract tests)
- desktop `tsc --noEmit` and `vite build`

## Remaining risks

- No daemon, no PTY crate, no Git crate. Concurrent sessions are specified,
  not proven.
- Desktop `protocol_info` can be mistaken for `system.hello`. `AGENTS.md`
  says it is not.
- Handwritten TypeScript can still drift on payload *fields* even with the
  method-name catalog lock. Next session should add golden JSON fixtures
  for a few full envelopes if that starts happening.
- Editing `0001_initial.sql` in place is wrong the moment a user database
  exists. After this freeze, only additive migrations.
- Linux PATH and `SO_PEERCRED` are still documentation. GUI-launched
  daemons will miss executables until the agents session wires
  `LaunchEnvironment`.
- Orphaned children after a daemon crash remain a documented hole.

## Instructions for the next sessions

1. Create `crates/session` against `SessionStatus::can_transition_to`.
   Do not put PTY handles in Tauri or React.
2. Create `crates/daemon` that speaks the envelopes in `crates/core`.
   Handshake is `system.hello` then `state.snapshot`.
3. Create `crates/git` with argv-only Git and two-step worktree remove.
4. Extend the catalog in Rust, `protocol/catalog.json`, and
   `apps/desktop/src/ipc/methods.ts` together if you need `session.kill`
   or `session.subscribe`.
5. Wire `apps/desktop/src/ipc` to Tauri invoke by implementing `IpcClient`.
   Do not add `greet` back. Do not spawn from a component.
6. Keep Linux and macOS as one code path with platform path helpers, not
   `cfg` forks in domain types.
