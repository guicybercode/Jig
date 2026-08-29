# Contributing to CLI Master

CLI Master welcomes focused changes that move the Beta v0.1 acceptance flow
forward while preserving terminal correctness and Git safety.

## Before you begin

Read these files:

1. [README.md](README.md) for setup and validation commands.
2. [ARCHITECTURE.md](ARCHITECTURE.md) for accepted system boundaries.
3. [design-system/cli-master/MASTER.md](design-system/cli-master/MASTER.md) for
   interface and accessibility rules.
4. [docs/PACKAGING.md](docs/PACKAGING.md) and
   [docs/RELEASE_CHECKLIST.md](docs/RELEASE_CHECKLIST.md) for ship criteria.

Windows support and the future features listed as out of scope in the
architecture are not accepted for Beta v0.1 unless the core acceptance flow is
already complete and stable.

## Development workflow

1. Create a focused branch from the current `main` branch.
2. Make one coherent change at a time.
3. Add or update tests for every public behavior.
4. Run the complete local gate.
5. Commit with a Conventional Commit title and an explanatory body.

```bash
pnpm install --frozen-lockfile
pnpm check
```

`pnpm check` is the aggregator. It runs frontend lint, typecheck, tests, and
build, then Rust format, clippy, the fake-agent build, and the workspace tests.
Individual commands are listed in the README.

Use commit titles such as:

```text
feat(projects): validate repository roots
fix(worktrees): block dirty removal
test(session): cover PTY resize
docs: explain daemon recovery
```

Keep commits reviewable. Do not combine formatting, dependency upgrades, and a
feature in one commit.

## Code expectations

- Prefer small modules connected through explicit typed interfaces.
- Keep public APIs documented and actionable errors specific.
- Do not use TypeScript `any` or untyped Rust serialization boundaries.
- Keep PTY output outside React state and SQLite write loops.
- Pass executables and arguments separately; do not introduce a generic shell
  execution IPC command.
- Do not add a dependency when the platform or standard library already solves
  the problem clearly.
- Preserve Linux and macOS behavior in shared abstractions.
- Do not depend on Codex, Claude, Gemini, or OpenCode being installed.

## Tests

Tests must be headless, deterministic, and independent. Use temporary
directories and databases for filesystem, Git, and SQLite integration tests.
Use `cli-master-fake-agent` for interactive PTY lifecycle tests. Do not use
timeouts so short that a loaded CI runner flakes.

Frontend tests should query semantic roles and labels. Mock the project-owned
IPC client rather than scattering Tauri mocks through components. Do not
unit-test xterm.js internals. Unmounting a terminal pane must unsubscribe and
must not stop the session.

Bug fixes require a regression test that fails without the fix.

Do not hide failures with `continue-on-error` in CI.

## User interface changes

- Preserve complete keyboard operation and visible focus.
- Include text or shape with status colors.
- Keep terminal control chords available while xterm has focus.
- Respect reduced-motion preferences.
- Do not add remote fonts, decorative animation frameworks, or simulated
  terminal output.

Verify responsive behavior at 375px, 768px, 1024px, and 1440px widths even
though the packaged desktop window has a larger minimum size. Narrow browser
views remain useful for accessibility and layout regression testing.

## Git and process safety

Changes that can stop processes or modify worktrees need tests for failure and
partial completion. Never introduce automatic force deletion, `git reset
--hard`, PID-only reattachment, or recursive deletion of an unvalidated path.

When an operation spans SQLite and Git, represent intermediate states and make
recovery visible. Do not claim atomic behavior that the system cannot provide.

A test may remove a dirty worktree only after it has already proven that
unconfirmed dirty removal is rejected.

## Documentation

Write in clear, direct language. Update README setup commands when tooling
changes and update ARCHITECTURE.md only when an accepted boundary or tradeoff
changes. Keep examples safe to paste into a shell. Do not configure real
signing secrets in the repository.
