# Contributing to Jig

Jig welcomes bug reports, documentation improvements, tests, and focused
changes that preserve terminal correctness and Git safety. You do not need to
be a Rust developer to contribute.

## Before you begin

Read these files:

1. [README.md](README.md) for what Jig does and how to run it.
2. [docs/install.md](docs/install.md) for native build prerequisites.
3. [ARCHITECTURE.md](ARCHITECTURE.md) for accepted system boundaries.
4. [AGENTS.md](AGENTS.md) for crate ownership and the IPC catalog.
5. [design-system/cli-master/MASTER.md](design-system/cli-master/MASTER.md) for
   interface and accessibility rules.

Current development targets Beta v0.2 for Linux and macOS. Discuss substantial
features, new dependencies, or platform support in an issue before starting a
large change. Windows is outside the Beta scope.

## Report a bug or suggest a feature

Search [existing issues](https://github.com/guicybercode/Jig/issues) first.
For a bug, include:

- Jig version or commit, operating system, and CPU architecture.
- Whether you used a release bundle or `pnpm tauri dev`.
- Small, reproducible steps and the expected versus actual result.
- Relevant sanitized diagnostics; never paste tokens, complete environments,
  private repository contents, or confidential terminal output.

For a feature, explain the workflow and the problem it solves. If it changes
process lifecycle, filesystem access, or Git behavior, describe the safety
implications too.

**Do not report vulnerabilities in a public issue.** Follow
[SECURITY.md](SECURITY.md) instead.

## Set up your development environment

Use Node.js 24, the pinned pnpm 11.9.0, stable Rust, Git, and your platform's
Tauri dependencies as described in the [README](README.md#run-from-source).
Fork the repository if you do not have push access, then clone your fork and
create a focused branch from the current `main`.

```bash
pnpm install --frozen-lockfile
pnpm tauri dev
```

`pnpm dev` is only the browser frontend; it is not a substitute for running
the native desktop app. Internal Cargo names, the daemon executable, and the
`@cli-master/desktop` workspace package still use the historical CLI Master
name. Use those exact identifiers in commands.

## Validate your change

From the repository root:

```bash
pnpm check:versions
bash scripts/stage-sidecar.sh --debug
cargo build -p cli-master-fake-agent --locked
pnpm check
pnpm --filter @cli-master/desktop exec playwright install chromium
pnpm --filter @cli-master/desktop test:e2e
```

On Linux, installing Chromium's system dependencies may require
`pnpm --filter @cli-master/desktop exec playwright install --with-deps chromium`.

`pnpm check` runs frontend type checks, unit tests, and the Vite build, followed
by Rust formatting, Clippy, workspace tests, and documentation with warnings
treated as errors. Version checks and Playwright are separate. CI runs quality
and packaging jobs on both Linux and macOS.

For a faster feedback loop while editing:

```bash
pnpm test:frontend
cargo test -p cli-master-session --locked
```

Run relevant targeted tests as you work, then the complete gate before
requesting review. For documentation-only changes, verify commands against
the scripts, check links, and run `git diff --check`; explain skipped runtime
checks in the pull request. Never claim to have run checks you skipped.

## Open a pull request

1. Keep one coherent change per pull request. Do not mix formatting,
   dependency upgrades, and a feature.
2. Add or update tests for changed public behavior. Bug fixes need a
   regression test that fails without the fix.
3. Update affected documentation and all IPC mirrors when relevant.
4. Use a descriptive Conventional Commit title, such as
   `fix(worktrees): block dirty removal` or `docs: explain daemon recovery`.
5. Target `main`. Explain the problem, solution, tests run, and known limits.
   Include screenshots for visible UI changes and link the related issue.
6. Wait for review and CI. Do not weaken assertions or bypass safety checks
   to make a failing job pass.

## Code and test expectations

- Prefer small modules connected through explicit typed interfaces.
- Keep public APIs documented and actionable errors specific.
- Do not use TypeScript `any` or untyped Rust serialization boundaries.
- Keep `crates/core` free of I/O. The daemon, not React or Tauri, owns sessions.
- Keep PTY output outside React state and SQLite write loops.
- Pass executables and arguments separately; do not introduce a generic shell
  execution IPC command or interpolate commands into `sh -c`.
- Preserve Linux and macOS behavior in shared abstractions and tests.
- Update authoritative Rust wire types before the TypeScript and JSON
  mirrors; do not add a domain-specific Tauri command.

Backend tests use real temporary directories, real Git, real SQLite, and
short-lived child programs. Interactive runtime acceptance uses
`cli-master-fake-agent` through `CommandSpec`. Wait on observable readiness
or state, not arbitrary sleeps to hide a race.

Frontend tests query semantic roles and labels and mock the project-owned IPC
client rather than scattered Tauri APIs. Do not unit-test xterm.js internals.

## User interface changes

- Preserve complete keyboard operation and visible focus.
- Include text or shape with status colors and respect reduced motion.
- Keep terminal control chords available while xterm has focus.
- Do not add remote fonts, decorative animation frameworks, or simulated
  terminal output.
- Verify layouts at 375px, 768px, 1024px, and 1440px widths.

## Git and process safety

Changes that stop processes or modify worktrees need tests for failure and
partial completion. Never introduce automatic force deletion, `git reset
--hard`, PID-only reattachment, or recursive deletion of an unvalidated path.

Worktree removal requires preparation and a state-bound confirmation token.
Deleting session metadata must not delete a worktree; removing a project must
not delete its directory. Closing a window or removing a canvas card must not
stop daemon-owned sessions.

When an operation spans SQLite and Git, represent intermediate states and make
recovery visible. Do not claim atomic behavior the system cannot provide.

## Documentation and releases

Write in clear, direct English and keep shell examples safe to paste. Update
setup commands when tooling changes. Architectural changes require an ADR;
do not silently rewrite accepted boundaries.

Packaging changes need matching updates to [docs/PACKAGING.md](docs/PACKAGING.md)
and [docs/RELEASE_CHECKLIST.md](docs/RELEASE_CHECKLIST.md). `pnpm package` produces
local unsigned bundles; it does not publish a GitHub Release. Release
publication is a separate maintainer action. Never commit signing secrets,
notarization credentials, or vendor API keys.

## License

Jig is licensed under [MIT](LICENSE). Submit only code and documentation you
have the right to contribute, under the project's license. Preserve required
notices for third-party material and identify it in your pull request.
