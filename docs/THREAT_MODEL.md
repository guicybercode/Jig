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
| T1 | Misconfigured custom agent (`bash -c`, relative `../` executable, secret env) | High — unexpected shell, path escape, leaked env | Medium | Custom definitions require an absolute path or a bare PATH name. Args stay an array. `sh`/`bash`/`zsh -c` is refused at spawn except the documented PATH-import helper. Env values are not logged. | The user can still launch a dangerous absolute executable. That is intended. |
| T2 | Command injection via concatenated shell strings | High — arbitrary commands | Low after this change | Spawns use `executable` + `args[]` + `cwd` + explicit env. Metacharacters in arguments are not interpreted. | Agent CLIs may invoke shells themselves. |
| T3 | Path traversal (`../`) in worktree or project paths | High — delete or inspect the wrong tree | Medium | Lexical normalize plus resolve of the existing prefix. `..` components are preserved while walking missing suffixes. Managed worktree paths must stay under `{data-dir}/worktrees`. | New code paths that skip `resolve_path` could regress. |
| T4 | Symlink escape out of the managed root | High — delete or follow an unintended directory | Medium | Existing prefixes are canonicalized. Last-component symlinks are recorded. `is_within` uses resolved paths. | TOCTOU between resolve and `git worktree remove`. |
| T5 | Recursive delete of `/`, `$HOME`, or a project repository | Critical — user data loss | Low | `assert_not_critical` refuses `/`, home, `/home` or `/Users`, the data dir, and registered project roots. Project unregistration is metadata-only. Worktree delete must be a managed child of the worktree root, never the root itself. | A future helper that calls `remove_dir_all` without these guards. |
| T6 | Silent removal of a dirty worktree | High — lost uncommitted work | Medium | Dirty detection via porcelain v2 including untracked files. Removal without `allowDirty` returns `WORKTREE_DIRTY`. Git `--force` is passed only after explicit confirmation. The UI requires a checkbox for dirty trees. | User can still confirm dirty removal. |
| T7 | Removing a worktree used by a live session | High — kill the wrong files under a running agent | Medium | `WORKTREE_IN_USE` blocks prepare and confirm. Force is not offered while in use. | Session Manager is not wired yet; callers must pass `in_use` honestly. |
| T8 | Secret in logs (`TOKEN`, `PASSWORD`, cookies, full env) | High — credentials in a support bundle | Medium | Structured logs redact sensitive names and assignment-style values. `CommandSpec` debug omits args and env values. Diagnostics export never includes the environment map, prompts, or PTY output. | Unknown secret names that do not match the denylist. Terminal contents are still in memory for live sessions. |
| T9 | Full environment exposed in diagnostics | Medium — PATH plus accidental secrets | Medium | Diagnostics lists OS, arch, app paths, Git version, agent detection, resolved helper executables, counts, and sanitized logs. `PWD` and env maps are omitted. | Search-path directories can still reveal username layout. |
| T10 | Sensitive terminal output copied into logs | High | Medium | Logs must not record PTY bytes. Replay buffers stay in the session layer. Diagnostics does not export terminal contents. | A future debug flag could reintroduce this. |
| T11 | PID reuse: signaling a new process that reused a stale PID | Critical — kill an unrelated process | Medium | Stop requires the current daemon instance ID plus a start-time token from `/proc/<pid>/stat` (Linux) or `ps -o lstart=` (macOS). Mismatch returns `PROCESS_IDENTITY_MISMATCH` and does not signal. SIGKILL is never implied. | macOS start tokens are weaker than Linux starttime. pidfd is not used yet. |
| T12 | Abandoned child after daemon crash | Medium — orphaned agent | Medium (accepted) | Architecture: v0.1 cannot reattach PTYs after daemon death. Stop uses a process-group oriented plan once sessions exist. Git/diagnostics children are waited and killed on timeout. | Orphans may need manual inspection. Documented v0.1 limit. |
| T13 | Repository moved while the stored path remains | Medium — operate on the wrong tree | Medium | `GitService::confirm_stored_root` compares Git toplevels and returns `REPOSITORY_MOVED`. Registration uses `rev-parse --show-toplevel`. | Callers must invoke the check on open/refresh. |
| T14 | SQLite path outside the data directory or unreadable DB | Medium — metadata loss / confusion | Low | Platform paths keep `cli-master.db` under the data dir. Diagnostics reports existence and schema version without dumping rows. Busy timeout is 5s. | No lock-file check in this session beyond SQLite itself. |
| T15 | Diagnostics export containing secrets | High | Medium | Export is pretty-printed JSON of the sanitized report. Copy UI states that env vars were excluded. Tests inject `TOKEN=` and `COOKIE=` and assert they are redacted. | User-pasted secrets in custom log messages that use unknown key names. |
| T16 | Compromised frontend sending invalid IPC (`eval`, `command`, `../`) | High if the backend trusted the webview | Medium | Unknown methods are rejected. Payload keys `command` and `shell` are refused. Path-like strings with `..` must resolve inside a known root. There is no generic shell-exec IPC method. | The webview is same-user; this is defense in depth, not a sandbox. |

## Inevitable exceptions

Login-shell PATH import is the only supported `zsh -lc` / `bash -lc` / `sh -c`
invocation. The command string is the constant `printf '%s\n' "$PATH"`. The
shell path must be absolute and one of `zsh`, `bash`, or `sh`. Custom agents
cannot use this exception.

## Destructive operations

| Operation | Deletes files? | Guards |
|---|---|---|
| Remove project | No | Rejected while sessions/worktrees exist |
| Delete session | No | Rejected while the process is live |
| Stop process | No | Identity check; SIGKILL only after explicit force |
| Remove worktree | Yes, that worktree only | Managed path, not dirty unless `allowDirty`, not in use, confirmation token |
| Delete custom agent | No | Rejected while referenced by a session |

Confirmation tokens expire after five minutes and are single-use. Confirm
rechecks the fingerprint (kind, path, branch, session, dirty, in-use).

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
