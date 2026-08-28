# Contributing to CLI Master

CLI Master welcomes focused changes that move the Beta v0.1 acceptance flow
forward while preserving terminal correctness and Git safety.

## Before you begin

Read these files:

1. [README.md](README.md) for setup and validation commands.
2. [ARCHITECTURE.md](ARCHITECTURE.md) for accepted system boundaries.
3. [design-system/cli-master/MASTER.md](design-system/cli-master/MASTER.md) for
   interface and accessibility rules.

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
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

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

## Tests

Tests must be headless, deterministic, and independent. Use temporary
directories and databases for filesystem, Git, and SQLite integration tests.
Use short-lived real child programs for PTY lifecycle tests.

Frontend tests should query semantic roles and labels. Mock the project-owned
backend client rather than scattering Tauri mocks through components. Do not
unit-test xterm.js internals.

Bug fixes require a regression test that fails without the fix.

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

## Documentation

Write in clear, direct language. Update README setup commands when tooling
changes and update ARCHITECTURE.md only when an accepted boundary or tradeoff
changes. Keep examples safe to paste into a shell.
