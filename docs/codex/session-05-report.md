# Session 05 — Extensible agent adapters

This session implements the Beta v0.1 agent adapter layer in `crates/agents`,
with structured `CommandSpec` fields in `crates/core`. The crate is now a
workspace member so CI includes it.

The PTY/session crate is unchanged. Adapters produce a launch specification;
they do not spawn interactive agent sessions.

## Adapters

`AgentAdapter` is the only vendor boundary:

| Method | Role |
|---|---|
| `definition()` | Static catalog row (id, name, executable, default args, source, capabilities). Detection fields are unset. |
| `detect(environment)` | PATH / absolute-path lookup. Never starts a process. |
| `resolve_definition(environment)` | Catalog row plus `installed` / `resolved_path` / warning. No spawn. |
| `build_command(context)` | Structured `CommandSpec` for the session layer. |
| `diagnostics(environment)` | UI-safe install report, with a version probe only when the adapter opts in. |
| `capabilities()` | `interactive`, `requires_pty`, `supports_version_probe`, `extra_args_allowed`. |

Built-ins are four unit structs registered in `AgentRegistry::new()`:

| Key | Display name | Executable |
|---|---|---|
| `codex` | Codex | `codex` |
| `claude` | Claude Code | `claude` |
| `gemini` | Gemini CLI | `gemini` |
| `opencode` | OpenCode | `opencode` |

Adding a fifth built-in is two localized edits: one `built_in_adapter!` line and
one `insert_builtin` call. No PATH, placeholder, or PTY code changes.

Built-ins launch with an empty default argument list. They do not invent
`--yolo`, `--dangerously-skip-permissions`, or other unconfirmed flags. Users
may append extra arguments through `LaunchContext::with_extra_args`; those
values stay as array elements.

Missing built-ins are not an error at registry construction. Detection returns
`NotFound`, snapshots report `installed: false`, and diagnostics return an
actionable launch-test status.

Custom agents use the same trait via `CustomAgentAdapter`. Validated fields:

- key (ASCII token) and non-empty display name
- executable: absolute path, `~/…`, a known placeholder, or a bare PATH name
- args as a `Vec<String>` (JSON objects with a shell string are rejected)
- environment additions and optional removals
- optional default cwd, icon, color
- `requires_pty` (default `true`)

Session extra args, env, title, startup input, and an absolute executable
override live on `LaunchContext`, not in a concatenated command string.

## PATH behavior

GUI apps (Linux desktop files, macOS Finder) often inherit a truncated PATH.
`LaunchEnvironment` uses a fixed search order:

1. Inherited process `PATH`
2. User-configured extra directories
3. Standard fallbacks that currently exist on disk

Standard fallbacks (appended, never sourced from dotfiles):

- `~/.local/bin`, `~/bin`
- `~/.cargo/bin` and `$CARGO_HOME/bin`
- npm/pnpm user prefixes when those directories exist (`~/.npm-global/bin`,
  `~/.local/share/pnpm`, `~/Library/pnpm`, `$PNPM_HOME`)
- mise/asdf shims (`~/.local/share/mise/shims`, `~/.mise/shims`, `~/.asdf/shims`,
  `$XDG_DATA_HOME/mise/shims`)
- Apple Silicon Homebrew `/opt/homebrew/bin` and `/opt/homebrew/sbin`
- `/usr/local/bin`, `/usr/local/sbin`, Linuxbrew, `/usr/bin`, `/bin`

`LaunchEnvironment::from_search_paths` is isolated (no standard fallbacks) so
tests and negative detection never observe a host `codex`.
`LaunchEnvironment::desktop()` is what the daemon should use at runtime.

An absolute executable on a custom definition or
`LaunchContext::with_executable_override` skips PATH lookup. Paths with spaces
are `PathBuf` values, not split strings.

Optional login-shell import (`read_login_shell_path`) runs a **constant**
command:

```text
$SHELL -lc 'printf … BEGIN; printf %s "$PATH"; printf … END'
```

That executes the user's startup files. The full environment is discarded; only
the marked PATH payload is returned. The UI must preview it before persisting.
This is never done automatically.

`path_diagnostics()` explains inherited / extra / standard / effective
directories. It does not dump the process environment or tokens.

Leading `~/` is expanded from `HOME`. `~user` forms are rejected.

## Placeholders

Expansion is a single-pass string rewrite. There is no `eval`, no shell, and no
second pass over replacement values.

Allowed names:

- `${PROJECT_PATH}`
- `${WORKTREE_PATH}`
- `${SESSION_ID}`
- `${SESSION_NAME}`

`$$` becomes a literal `$`. Unknown names and known-but-unset names are errors.
Placeholders expand independently inside args, environment **values**, terminal
title, executable templates, and default cwd. Environment **keys** are never
expanded.

## CommandSpec

Every launch is:

```text
executable + args[] + cwd + env additions + env removals
+ optional terminal title + optional startup input
```

Avoid:

```text
command: "codex --foo '${user_input}'"
```

Prefer:

```text
executable: "codex"
args: ["--foo", user_input]
```

`startup_input` is raw PTY input after attach (size-capped, redacted in
`Debug`). It is not a shell snippet. The session layer should write it to the
PTY master only when `capabilities.requires_pty` is true.

`Debug` redacts argument contents, environment values, and startup input.

## Diagnostics and test executable

`test_executable` / `diagnostics`:

1. Resolve the binary (absolute or PATH).
2. Optionally spawn `argv[0] --version` with stdin closed.
3. Enforce a timeout (default 2s) and kill the child on hang.
4. Capture at most 4 KiB and keep the first line as a version preview.

Built-ins opt into `--version`. Custom agents do not; setup UI can still call
`test_executable` with `ProbeOptions` when the user asks. No prompt is written.
`--version` is the only probe flag, because it is the conventional
non-destructive query. If a CLI ignores it and hangs, the timeout wins.

Example UI payload:

```text
Claude Code
- installed: yes
- path: /home/user/.local/bin/claude
- version: claude 1.2.3
- launch test: success
```

Errors include a stable `ApiError` code (`AGENT_EXECUTABLE_NOT_FOUND`, …) and a
suggested action. Searched directories may be listed. Tokens and the full
environment are not.

## Risks

- Desktop PATH fallbacks can select a different binary than an interactive
  shell. Show effective PATH in diagnostics and allow extra dirs or an absolute
  executable.
- Login-shell import runs dotfiles. Keep it explicit and preview-only.
- User-supplied extra args can include dangerous vendor flags. This crate does
  not add those flags itself; the UI should not present “skip confirmations”
  presets.
- `--version` on a hostile custom binary is still a process start. Custom
  diagnostics skip it unless requested. Timeout + kill bounds the hang case.
- Placeholder values are not re-scanned. A project path containing `${…}` stays
  literal after expansion.
- `HOME` must be set for `~/` expansion.
- Killing a version probe does not use a process group (no `unsafe`). Hang
  fixtures and well-behaved `--version` implementations are a single process.
- Persistence of custom agents in SQLite is still the storage/daemon layer.
  Definitions are serializable and ready to store.

## Tests

CI must not require Codex, Claude, Gemini, or OpenCode. Fixtures are temporary
scripts with mode `0700`.

Covered:

- positive and negative detection
- non-executable candidate, later PATH entry wins
- executable path containing spaces
- args with spaces and shell metacharacters stay separate
- `--version` success and hang timeout
- custom register, PTY capability, placeholders, env additions
- unknown placeholder rejection
- cwd missing
- binary removed after registration
- duplicate keys / display names, protected built-ins
- JSON args must be an array, not a shell string
- isolated PATH does not see `/usr/bin`
- extra paths searched before standard fallbacks

Run:

```bash
cargo test -p cli-master-core -p cli-master-agents --all-targets
cargo clippy -p cli-master-core -p cli-master-agents --all-targets -- -D warnings
```

## Integration instructions

Daemon / session layer:

1. Construct `LaunchEnvironment::desktop()`, optionally plus user extra paths
   from settings.
2. Seed `AgentRegistry::new()` and `register_custom` rows from SQLite.
3. For `agent.list`, call `registry.snapshots(&environment)` (no version spawn).
4. For `agent.detect` / diagnostics UI, call `registry.diagnostics(key, env, options)`.
5. On `session.create`:
   - resolve cwd (`CustomAgentDefinition::resolve_cwd` or project/worktree path)
   - build `PlaceholderContext`
   - `LaunchContext::new(cwd, env).with_extra_args(…).with_placeholders(…)`
   - `adapter.build_command(&context)`
   - if `capabilities.requires_pty`, open a PTY and spawn
     `CommandSpec.executable` with `args` (no `sh -c`)
   - apply `env()` additions and `env_removals()`
   - set the terminal title from `terminal_title()`
   - after attach, write `startup_input()` if present
6. Map `AgentError::api_error()` onto IPC responses.

Frontend:

- Render diagnostics fields only; do not invent command strings.
- Custom-agent form: name, executable, args array, cwd, env key/value rows,
  optional icon/color, requires PTY. Call a test-executable IPC before save.
- PATH settings: show `path_diagnostics`, extra directories, optional
  login-shell import with a warning that startup files will run.

Do not implement login or token storage. Agents inherit the user's existing
local authentication environment, minus explicit removals.
