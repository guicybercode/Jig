# Session 08 — local safety, errors, logs, diagnostics

Branch: `cursor/08-safety-diagnostics-2d64` (session target `codex/08-safety-diagnostics`).

This session audited the current tree, then implemented guardrails. The product
was not turned into a sandbox. Agent CLIs still launch as structured processes.

## What existed

- `CommandSpec` already kept executable and args separate and redacted Debug.
- Custom agents already rejected relative executables with a directory component.
- `ApiError` had `code`, `message`, `action`, `details`.
- No Git crate, no path guards, no structured logger, no diagnostics surface,
  no destructive confirmation, no spawn helper with timeouts.
- `crates/agents` existed but was missing from the Cargo workspace.

## Vulnerabilities and gaps found

1. **No real process spawn layer.** Future Git/PTY code could have introduced
   `sh -c`. There was no central refusal of shell command strings.
2. **No canonical path policy.** Worktree paths with `../`, symlinks, spaces, or
   Unicode had no shared resolver. `Path::file_name()` returns `None` for `..`,
   which would have dropped parent-dir components while walking missing paths.
3. **No protection for `/`, home, or project roots** on delete-style operations.
4. **No dirty/in-use worktree gate** and no confirmation token.
5. **PID reuse** was documented in ARCHITECTURE.md but unimplemented.
6. **Logs** had no structured JSON, redaction of free-form text, or ring buffer
   for diagnostics.
7. **IPC** had no method allow-list or rejection of a `command`/`shell` field
   from a compromised webview.
8. **Errors** did not have title, recoverable, technical (log-only), or source
   chain separation. Suggested actions were optional and unused.
9. **No diagnostics** for app version, OS, Git, SQLite, agents, or sanitized logs.
10. **No timeouts** on helper processes.

## Corrections

| Area | Change |
|---|---|
| Errors | `ApplicationError` + `ErrorCode` (`AGENT_NOT_FOUND`, `WORKTREE_DIRTY`, …). IPC projection remains `ApiError` with optional `title`/`recoverable`. Source chain is log-only. |
| Redaction | Sensitive names, auth/cookie headers, private-key blocks, and common provider token formats. `AUTHOR` is not treated as `AUTH`; API errors and Debug projections are sanitized. |
| Spawn | `SpawnRequest` / `run_command_unchecked`. Refuses command-string flags for known shells, canonical aliases, and direct `env`/BusyBox wrappers. Process-group timeout and bounded readers prevent descendants retaining pipes. Login-shell PATH import is the only exception. |
| Paths | Resolve existing prefixes via canonicalize; keep leading `..`; reject broken/final symlinks; canonicalize every protected root. Worktree confirmation binds device/inode and revalidates immediately before Git. |
| Destructive | Typed prepare/confirm capabilities, all target IDs and force mode bound, single-use on every attempt, five-minute TTL, 128-entry cap. Dirty and in-use gates remain explicit. Project remove is metadata-only. |
| Git | `cli-master-git`: structured `git -C` / args, porcelain dirty/branch recheck, consumed confirmation object, no silent `--force`. |
| Process identity | Start-time token + daemon instance; refuse stale PIDs. |
| Logs | JSON lines with timestamp, level, target, operation, session/project IDs, error code. File mode `0600`, size rotation. |
| Diagnostics | `diagnostics.get` / `diagnostics.export` Tauri commands. Export masks the home prefix; UI copy uses only native sanitized export and fails closed. |
| IPC | Allow-listed methods; reject shell/command aliases and validate method-specific paths against registered/managed roots. |
| Workspace | `crates/agents`, `crates/safety`, `crates/git` are workspace members. |

## Accepted risks

- Agent CLIs can still do anything the OS user can do. That is the product.
- Daemon, SessionManager, and live PTY stop are not in this tree yet.
  Diagnostics reports daemon disconnected and session/worktree counts as zero
  until those crates land.
- Dirty worktree removal is allowed after an explicit `allowDirty` confirmation.
- Login-shell PATH import still runs a constant `-lc` command.
- A narrow TOCTOU remains after the last path identity check and before Git
  opens the worktree. Descriptor-relative mutation would be required to close it.
- Arbitrary executable/interpreter wrappers are not a sandbox boundary. Direct
  known wrappers are modeled, but an exhaustive wrapper denylist is impossible
  without an explicit executable allowlist or sandbox.
- `in_use` remains caller-supplied until `SessionManager` is wired.
- macOS process identity uses `ps` lstart, which is weaker than Linux
  `/proc` starttime. pidfd is deferred.
- Orphaned children after daemon crash remain a v0.1 limitation.
- Secret pattern redaction cannot catch every unlabelled custom format.

## Tests

Rust:

- command args with `;`, `$(...)` are not executed by a shell
- `bash -c` refused
- timeout kills `sleep`
- path traversal, broken/final symlink escape, unicode/spaces
- `/`, `$HOME`, home ancestors, canonical data/worktree/project roots refused
- unmanaged path delete refused
- dirty/branch state revalidated before Git; newly dirty worktree survives
- confirmation is single-use, bound to all IDs plus device/inode, rejects
  same-path replacement, and cannot elevate a non-force stop token
- redaction of labelled values, auth/cookie headers, private keys, provider
  tokens, error fields, and Debug projections
- `ApplicationError` serializes without technical/source fields and with an action
- diagnostics export omits injected secrets and the environment map
- unknown IPC method and `command` field rejected
- descendant retaining stdout cannot hold the runner past its bounded wait;
  the isolated process group is killed

Frontend:

- Diagnostics dialog copies only native sanitized export and fails closed
- Worktree remove is disabled while dirty until the checkbox, and always
  disabled while in use
- `allowDirty` consent resets immediately when path/branch/state changes

## Integration review

When session/daemon crates land they must:

1. Spawn agents only through `CommandSpec` + `run_command` / PTY spawn with the
   same shell refusal (PTY spawn is a separate API; do not wrap in `sh -c`).
2. Pass `in_use` from `SessionManager` into worktree prepare/remove.
3. Record `ProcessIdentity` at spawn and refuse stop on mismatch.
4. Fill diagnostics session/worktree counts and daemon instance ID from live
   state without dumping env or PTY buffers.
5. Validate every IPC method with `validate_method_payload` before I/O.
6. Open SQLite only at `PlatformPaths::database_path`.
7. Call `GitService::confirm_stored_root` when opening a project.

Do not add a generic “run this shell string” Tauri command.
