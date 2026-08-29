# Threat model — CLI Master Beta v0.1

CLI Master starts user-installed coding-agent CLIs in local PTYs and manages
Git worktrees. It is a local orchestration tool, not a sandbox. A CLI that the
user intentionally starts has the same file and network access as that user.

This model focuses on accidental data loss, local IPC misuse, secret leakage,
and ambiguous destructive operations on Linux and macOS.

## Trust boundaries

| Component | Trust and responsibility |
|---|---|
| Local user | Trusted to select repositories and executables |
| Agent CLI | Allowed to run; its arguments, environment, and PTY output are sensitive |
| Desktop webview | Untrusted request source; it uses typed IPC only |
| Tauri process | Generic transport bridge; it does not own domain state or sessions |
| Daemon | Owns durable coordination, event sequencing, and live session composition |
| `crates/git` | Owns Git argv, canonical path checks, and clean-only worktree removal |
| Other OS users | Untrusted; the private socket and directories must exclude them |

## In scope

- structured agent definitions and process launch
- the private Unix socket and versioned IPC envelopes
- repository and worktree path validation
- session process groups and crash recovery
- structured errors, logs, diagnostics, and clipboard export
- SQLite metadata and bounded event replay

Remote vendor traffic, sandboxing an intentionally selected CLI, Windows, and
defending against an already-compromised same-user process are out of scope.

## Threats and mitigations

| ID | Threat | Current mitigation | Residual risk |
|---|---|---|---|
| T1 | Shell injection through concatenated commands | `CommandSpec` and agent commands preserve executable, `args[]`, cwd, env additions, and env removals as separate values. Git and PTY adapters call process APIs directly. No domain IPC accepts a shell command string. | A user may deliberately configure a shell as the executable. That is intentional CLI execution, not a sandbox escape. |
| T2 | Secrets persisted or exposed by command metadata | Wire validation rejects secret-bearing custom environment keys. `Debug` for command types reports counts and keys, not argument, startup-input, or environment values. PTY bytes remain outside React state and diagnostic DTOs. | Inherited process environment and terminal output are inherently available to the launched CLI. |
| T3 | Malformed or unexpected webview IPC | Request DTOs use bounded validated value types and `deny_unknown_fields`. The daemon rejects unknown methods. Tauri exposes a generic `daemon_invoke` bridge instead of parallel domain commands. | A newly added method can still be unsafe if its backend validation is incomplete. |
| T4 | Cross-user access to the daemon socket | Runtime/data directories are private, the socket is mode `0600`, and Linux validates the peer UID. | macOS relies on filesystem ownership and socket permissions; a same-user process remains in the trusted OS account boundary. |
| T5 | Path traversal or symlink escape | Git operations canonicalize existing roots, resolve intended missing suffixes from a canonical ancestor, require managed descendants, and refuse the primary checkout as a removable worktree. | A narrow filesystem TOCTOU window remains between validation and Git opening a path. |
| T6 | Silent loss of dirty or hidden worktree content | Removal checks staged, tracked, untracked, ignored, `assume-unchanged`, `skip-worktree`, lock, running, and in-use state. It re-inspects immediately before mutation and compares the full preparation. `git worktree remove --force` is never used. | A future daemon handler must keep the wire confirmation token bound to that exact clean preparation. |
| T7 | Confirmation replay or force escalation | The v1 wire shape separates `worktree.prepare_remove` from `worktree.remove`; blocked preparations cannot contain a token. Requests reject `allowDirty` and `force`. The Git service accepts no force flag. | Token storage/expiry belongs to daemon composition and must not be implemented in the UI or Tauri bridge. |
| T8 | Signaling the wrong process | `SessionManager` owns PTY children and process groups; the UI cannot signal PIDs. Stop uses the held child/group handles and bounded escalation. Recovery marks stale live rows `unknown` rather than signaling persisted PIDs. | Full PTY reattachment after daemon death is out of scope for Beta. |
| T9 | Secrets in errors or diagnostics | `ApiError` recursively redacts secret-like keys and free-form token/header patterns. The daemon re-redacts diagnostic issues. Clipboard JSON is generated in Rust from the bounded diagnostic DTO after home-prefix replacement. | Pattern redaction cannot identify every unlabelled custom secret. |
| T10 | Raw diagnostics copied after export failure | The React dialog copies only backend-generated `exportText`; it never serializes the on-screen response as a fallback and never renders loader error details. | Exact local paths are intentionally visible on screen to the local user. |
| T11 | Sensitive terminal data entering logs | Session logs contain identifiers, dimensions, state transitions, and process metadata, not PTY bytes or command arguments. Tauri bridge logs method, request ID, frame size, and safe status only. | A future debug statement can regress this rule; review logging changes as security-sensitive. |
| T12 | Unbounded replay or a slow client exhausting memory | Session replay and daemon event queues are bounded. Events are sequenced, lag is reported as a sanitized diagnostic issue, and clients recover from a cursor or fresh snapshot. | A client can miss events beyond the retained window and must resynchronize. |
| T13 | Crash leaving contradictory durable state | SQLite migrations are explicit. Startup reconciliation changes stale live sessions to `unknown`; recovery does not signal stale PIDs or delete repositories/worktrees. | Metadata may require user-visible reconciliation after an abrupt crash. |

## Destructive-operation invariants

| Operation | File deletion | Required invariant |
|---|---|---|
| Remove project | None | Removes metadata only; repository directory remains |
| Delete session | None | Session must not be live; worktree deletion is separate |
| Stop session | None | Daemon-owned process group only; no PID supplied by UI |
| Remove worktree | Managed worktree only | Exact registered descendant, clean and unused twice, state-bound confirmation, never `--force` |
| Remove custom agent | None | Referenced definitions remain protected by storage/domain checks |

There is no dirty-delete consent, `allowDirty`, force-removal flag, generic
filesystem delete method, or raw shell-execution IPC in Beta v1.

## Diagnostics export contract

The diagnostics response contains only daemon/protocol/schema identity,
home-relative application paths, executable search directories when available,
and a bounded list of sanitized issues. It never contains:

- environment maps or secret values
- command arguments or startup input
- prompts or terminal output
- database rows, repository file contents, or Git diffs
- confirmation tokens

The backend recursively redacts the export immediately before serialization.
The frontend treats that string as the only clipboard-authorized representation.

## Security review triggers

Revisit this model when adding an IPC method, a new process-spawn path, any
filesystem mutation, log fields derived from external input, support-bundle
content, or daemon recovery behavior.
