# Threat model — CLI Master Beta v0.1

CLI Master runs user-installed coding-agent CLIs in local PTYs and manages Git
worktrees. It is not a sandbox. Arbitrary CLI execution is a product feature.
This document covers **accidents**, **secret leakage**, and **ambiguous
destructive operations** on a machine the user already controls.

A same-user process can already read the user's files. Socket mode `0600` and
peer UID checks (planned for the daemon) reduce cross-user access; they do not
confine untrusted programs the user chose to launch.

## Scope

In scope:

- custom agent definitions
- process spawn
- filesystem paths
- Git worktrees
- structured logs
- diagnostics export
- IPC from the webview

Out of scope for v0.1:

- sandboxing agent CLIs
- multi-user hosts beyond Unix socket UID checks
- remote/network attackers without local code execution
- Windows

## Actors

| Actor | Trust |
|---|---|
| Local user | Fully trusted to run CLIs they install |
| Custom agent definition | User-authored; may be misconfigured |
| Installed agent CLI | Trusted to run, not trusted to be logged in full |
| Desktop webview / frontend | Untrusted as an IPC source; payloads are validated |
| Other OS users | Untrusted |

## Threats

| ID | Threat | Impact | Probability | Current mitigation | Residual risk |
|---|---|---|---|---|---|
| T1 | Misconfigured custom agent (`bash -c`, relative `../` executable, secret env) | High — unexpected shell, path escape, leaked env | Medium | Custom definitions require an absolute path or a bare PATH name. Args stay an array. Command-string flags for known POSIX shells, PowerShell, and `cmd` are refused, including combined flags, canonical shell aliases, and direct `env`/BusyBox wrappers. Env values are not logged. | The user can still launch a dangerous executable or route a shell through an arbitrary interpreter/wrapper that is not explicitly modeled. Preventing that requires an executable policy or sandbox and is out of scope. |
| T2 | Command injection via concatenated shell strings | High — arbitrary commands | Low after this change | Spawns use `executable` + `args[]` + `cwd` + explicit env. Metacharacters in arguments are not interpreted. | Agent CLIs may invoke shells themselves. |
| T3 | Path traversal (`../`) in worktree or project paths | High — delete or inspect the wrong tree | Medium | Lexical normalize plus resolve of the existing prefix. Leading `..` is preserved, and broken symlink prefixes fail closed. Managed worktree paths must stay under `{data-dir}/worktrees`. | New code paths that skip `resolve_path` could regress. |
| T4 | Symlink escape out of the managed root | High — delete or follow an unintended directory | Medium | Existing prefixes and protected roots are canonicalized. A final-component symlink is refused. Confirmation binds the canonical path plus device/inode and Git revalidates it immediately before removal. | A narrow TOCTOU window remains between the final identity check and Git opening the path. Fully closing it requires descriptor-relative deletion support from the mutating layer. |
| T5 | Recursive delete of `/`, `$HOME`, or a project repository | Critical — user data loss | Low | `assert_not_critical` refuses `/`, home and every ancestor of home, `/home` or `/Users`, the data/worktree roots, and canonical registered project roots. Project unregistration is metadata-only. Worktree delete must be a managed child, never the root itself. | A future helper that calls `remove_dir_all` without these guards. |
| T6 | Silent removal of a dirty worktree | High — lost uncommitted work | Medium | Dirty detection uses porcelain v2 including untracked files. Prepare exposes that explicit consent is required; confirm consumes a typed, single-use capability. Git rechecks branch and dirty state, and passes `--force` only when the confirmed state was dirty with `allowDirty`. UI consent is keyed to the exact preview and resets when it changes. | User can still deliberately confirm dirty removal. |
| T7 | Removing a worktree used by a live session | High — kill the wrong files under a running agent | Medium | `WORKTREE_IN_USE` blocks prepare and confirm. Force is not offered while in use. | Session Manager is not wired yet; callers must pass `in_use` honestly. |
| T8 | Secret in logs (`TOKEN`, `PASSWORD`, cookies, full env) | High — credentials in a support bundle | Medium | Structured logs and all API-error strings/details redact sensitive names, authorization/cookie headers, private-key blocks, and common provider token formats. Debug output omits command args, env values, process output, and confirmation tokens. Diagnostics never includes env maps, prompts, or PTY output. | Unknown unlabelled secret formats can evade pattern-based redaction. Terminal contents are still in memory for live sessions. |
| T9 | Full environment or username exposed in diagnostics | Medium — accidental disclosure in support bundles | Medium | Env maps and `PWD` are omitted. The native export replaces the home-directory prefix recursively. The UI never falls back to copying the raw report and warns against copying on-screen paths if export fails. | The local on-screen view intentionally shows exact paths to the machine's user. |
| T10 | Sensitive terminal output copied into logs | High | Medium | Logs must not record PTY bytes. Replay buffers stay in the session layer. Diagnostics does not export terminal contents. | A future debug flag could reintroduce this. |
| T11 | PID reuse: signaling a new process that reused a stale PID | Critical — kill an unrelated process | Medium | Stop rejects PIDs 0/1 and requires the current daemon instance plus a start-time token from `/proc/<pid>/stat` (Linux) or `ps -o lstart=` (macOS). Identity is polled during grace and rechecked before SIGKILL. A non-force confirmation token cannot be elevated to force. | macOS start tokens are weaker than Linux starttime. A signal-time race remains without Linux `pidfd` or an equivalent platform handle. |
| T12 | Abandoned child after daemon crash | Medium — orphaned agent | Medium (accepted) | Architecture: v0.1 cannot reattach PTYs after daemon death. Helper processes launch in their own process group; timeout or a descendant retaining an output pipe kills the group and bounds reader waits. | Agent PTY orphans after daemon crash may need manual inspection. Documented v0.1 limit. |
| T13 | Repository moved while the stored path remains | Medium — operate on the wrong tree | Medium | `GitService::confirm_stored_root` compares Git toplevels and returns `REPOSITORY_MOVED`. Registration uses `rev-parse --show-toplevel`. | Callers must invoke the check on open/refresh. |
| T14 | SQLite path outside the data directory or unreadable DB | Medium — metadata loss / confusion | Low | Platform paths keep `cli-master.db` under the data dir. Diagnostics reports existence and schema version without dumping rows. Busy timeout is 5s. | No lock-file check in this session beyond SQLite itself. |
| T15 | Diagnostics export containing secrets | High | Medium | Export is backend-generated pretty JSON of the sanitized report. Home paths are replaced, the UI has no raw fallback, and native/load errors are not rendered verbatim. Tests cover labelled values, authorization/cookie headers, private keys, common token patterns, and export failure. | User-pasted secrets using an unknown, unlabelled format can evade the pattern denylist. |
| T16 | Compromised frontend sending invalid IPC (`eval`, `command`, `../`) | High if the backend trusted the webview | Medium | Unknown methods are rejected. Shell/command key aliases are refused for every method. Path fields are validated by method against exact registered projects or managed worktrees; alternate key casing/separators are normalized. There is no generic shell-exec IPC method. | This generic validator is defense in depth. Downstream IPC DTOs still need typed schemas and `deny_unknown_fields` as they are implemented. |

## Inevitable exceptions

Login-shell PATH import is the only supported `zsh -lc` / `bash -lc` / `sh -c`
invocation. The command string is the constant `printf '%s\n' "$PATH"`. The
shell path and its canonical target must be absolute and one of `zsh`, `bash`,
or `sh`. Custom agents cannot use this exception.

## Destructive operations

| Operation | Deletes files? | Guards |
|---|---|---|
| Remove project | No | Rejected while sessions/worktrees exist |
| Delete session | No | Rejected while the process is live |
| Stop process | No | Identity check; SIGKILL only after explicit force |
| Remove worktree | Yes, that worktree only | Managed path, not dirty unless `allowDirty`, not in use, confirmation token |
| Delete custom agent | No | Rejected while referenced by a session |

Confirmation tokens expire after five minutes, are capped at 128 pending
entries, and are consumed on every confirm attempt. The binding covers the
operation, canonical path and device/inode, branch, all target IDs, dirty and
in-use state, and force mode. Dirty removal is the only confirmation-time
consent flag; its need is stated in the prepared plan and rechecked against Git.

## Timeouts and limits

| Operation | Limit |
|---|---|
| Git commands | 15s, 64 KiB captured output (diff 2 MiB) |
| Version probes | 3s |
| Diagnostics assembly | 5s budget for probes |
| Process stop grace | 3s then optional SIGKILL (2s) |
| PTY replay buffer | 8 MiB (session layer) |
| Log file | 5 MiB × 3 rotations, mode `0600` |
| Recent logs / errors | 200 / 50 |

Exceeded Git or version probes return `COMMAND_TIMEOUT` with a suggested action.
