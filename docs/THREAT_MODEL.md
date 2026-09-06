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
| Main desktop webview | Trusted application UI, but still an untrusted request source to Rust; it uses typed IPC only |
| Remote browser webview | Fully untrusted web content; it receives no Tauri capability and cannot own application state |
| Tauri process | Generic daemon transport plus one ephemeral native browser surface; it does not own domain state or sessions |
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
- untrusted HTTP(S) content loaded in the native canvas browser
- browser URL persistence, native geometry, permissions, and explicit handoff

Security of remote sites and their traffic, sandboxing an intentionally
selected CLI, Windows, and defending against an already-compromised same-user
process are out of scope. Jig does not proxy browser or coding-agent traffic.

## Threats and mitigations

| ID | Threat | Current mitigation | Residual risk |
|---|---|---|---|
| T1 | Shell injection through concatenated commands | `CommandSpec` and agent commands preserve executable, `args[]`, cwd, env additions, and env removals as separate values. After agent executable resolution, known POSIX shells, PowerShell, `cmd`, and direct `env`/BusyBox wrappers are refused when their arguments request a command string. Git and PTY adapters call process APIs directly. The only login-shell use imports PATH with a constant command through a canonical absolute supported POSIX shell outside `CommandSpec`. Probe/import helpers use an isolated process group, bounded output, and one deadline covering both process exit and pipe drain. | Interactive shells without a command string remain valid PTY programs. An intentionally selected CLI can invoke its own shell; preventing that requires a sandbox or executable policy. |
| T2 | Secrets persisted or exposed by command metadata | Wire validation rejects secret-bearing custom environment keys. `Debug` for command types reports counts and keys, not argument, startup-input, or environment values. PTY bytes remain outside React state and diagnostic DTOs. | Inherited process environment and terminal output are inherently available to the launched CLI. |
| T3 | Malformed or unexpected webview IPC | Request DTOs use bounded validated value types and `deny_unknown_fields`. The daemon rejects unknown methods. Tauri exposes a generic `daemon_invoke` bridge instead of parallel domain commands. | A newly added method can still be unsafe if its backend validation is incomplete. |
| T4 | Cross-user access to the daemon socket | Runtime/data directories are private, the socket is mode `0600`, and Linux validates the peer UID. | macOS relies on filesystem ownership and socket permissions; a same-user process remains in the trusted OS account boundary. |
| T5 | Path traversal or symlink escape | Git operations canonicalize existing roots, resolve intended missing suffixes from a canonical ancestor, require managed descendants, and refuse the primary checkout as a removable worktree. | A narrow filesystem TOCTOU window remains between validation and Git opening a path. |
| T6 | Silent loss of dirty or hidden worktree content | Removal checks staged, tracked, untracked, ignored, `assume-unchanged`, `skip-worktree`, lock, running, and in-use state. It re-inspects immediately before mutation and compares the full preparation. `git worktree remove --force` is never used. | A future daemon handler must delegate both removal methods to the session/worktree saga instead of duplicating its safety decisions. |
| T7 | Confirmation replay or force escalation | The v1 wire shape separates `worktree.prepare_remove` from `worktree.remove`; blocked preparations cannot contain a token. The session/worktree saga issues a CSPRNG-backed, short-lived, single-use token held only in process memory and bound to the exact preparation, worktree, and session association. Restart invalidates all tokens. Requests reject `allowDirty` and `force`; the Git service accepts no force flag. | A future daemon handler must delegate token issuance and consumption to the saga and must not implement token logic in the UI or Tauri bridge. |
| T8 | Signaling the wrong process | `SessionManager` owns PTY children and process groups; the UI cannot signal PIDs. Stop uses the held child/group handles and bounded escalation. Recovery marks stale live rows `unknown` rather than signaling persisted PIDs. | Full PTY reattachment after daemon death is out of scope for Beta. |
| T9 | Secrets in errors or diagnostics | Specialized crate errors remain internal. `ApplicationError` retains redacted technical/source context only for local logs and projects an `ApiError` without those fields. `ApiError` recursively redacts secret-like keys and free-form token/header patterns. The daemon re-redacts diagnostic issues. Clipboard JSON is generated in Rust from the bounded diagnostic DTO after home-prefix replacement. | Pattern redaction cannot identify every unlabelled custom secret. |
| T10 | Raw diagnostics copied after export failure | The React dialog copies only backend-generated `exportText`; it never serializes the on-screen response as a fallback and never renders loader error details. | Exact local paths are intentionally visible on screen to the local user. |
| T11 | Sensitive terminal data entering logs | Daemon and Tauri tracing output is JSON. Session logs contain identifiers, dimensions, state transitions, and redacted process metadata, not PTY bytes or command arguments. Tauri bridge logs method, request ID, frame size, and safe status only. Dynamic application errors and retained diagnostic issues are redacted before logging. | A future debug statement can regress this rule; review logging changes as security-sensitive. |
| T12 | Unbounded replay or a slow client exhausting memory | Session replay and daemon event queues are bounded. Events are sequenced, lag is reported as a sanitized diagnostic issue, and clients recover from a cursor or fresh snapshot. | A client can miss events beyond the retained window and must resynchronize. |
| T13 | Crash leaving contradictory durable state | SQLite migrations are explicit. Startup reconciliation changes stale live sessions to `unknown`; recovery does not signal stale PIDs or delete repositories/worktrees. | Metadata may require user-visible reconciliation after an abrupt crash. |
| T14 | A remote page invokes application or daemon IPC | The generated `AppManifest` enumerates every custom command. The capability targets only `webviews: ["main"]`, has no `remote` scope, and Rust independently requires the bundled main webview for browser and daemon commands. Daemon and browser events target only `main`. | This boundary requires a hostile-page smoke test whenever Tauri, Wry, capabilities, plugins, or commands change. |
| T15 | A page escapes browser navigation policy | Rust accepts only bounded HTTP(S) URLs without userinfo, blocks the application/dev origins and internal protocol hosts on every redirect, denies new windows and downloads, and disables link previews and devtools. | HTTP pages and remote sites remain hostile. File upload, clipboard, and ordinary page behavior follow the platform web engine; users must review what they disclose. |
| T16 | Browser URLs leak credentials | Signed or OAuth-bearing navigation URLs remain transient inside the active surface. Canvas persistence and explicit handoffs strip fragments and sensitive query keys; native location events are redacted and never update the durable document. Browser URLs are excluded from logs and diagnostics. | Redaction is a denylist and cannot reliably identify secrets embedded in a path. Users must review addresses before sharing them. |
| T17 | A native child webview covers trusted controls or captures input during movement | Only one surface may exist. React measures window-relative bounds and fails closed when clipped, obstructed, zoomed, moved, resized, panned, or covered by navigation/dialog chrome. Rust hides existing child surfaces before returning invalid-bounds errors. | Native WebKit does not obey DOM clipping or z-index. Platform smoke tests remain mandatory after layout changes. |
| T18 | Browser-to-terminal handoff becomes command injection | Handoff requires a user gesture on an explicit canvas connection. The address is sanitized and encoded as one POSIX single-quoted literal. No newline or Enter is sent, so the user must review and submit it. | Insertion occurs at the terminal's current cursor and may combine with existing interactive input; it is convenience text, not a structured agent tool call. |
| T19 | Remote content obtains camera, microphone, screen, geolocation, or notifications | The child is incognito and receives a document-start, all-frame guard that denies sensitive browser APIs. macOS usage-description keys are intentionally absent. | Wry 0.55 has no cross-platform native permission-deny handler and WKWebView may grant at its delegate layer. A packaged malicious-page test is a release gate; replace the script guard when Tauri exposes native denial. |
| T20 | Remote storage survives a browser-card lifecycle | Tauri creates the single child with `incognito(true)` and closes it on deactivation or unmount. React persists only redacted requested metadata, never cookies, content, history, or native handles. | WebKit store behavior differs by platform. Packaged tests must verify cookies, cache, local storage, and service workers disappear after close/reopen. |
| T21 | Browser focus traps keyboard users | Focus enters only after a user activates the page. Escape attempts a denied internal `jig-focus://main/` navigation that only returns native focus to the trusted main webview; it grants no remote IPC. | VoiceOver and Orca focus traversal still require packaged macOS/Linux acceptance testing. |

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

Revisit this model when adding an IPC method, capability, plugin, browser
event, webview navigation behavior, remote permission, a new process-spawn
path, any filesystem mutation, log fields derived from external input,
support-bundle content, or daemon recovery behavior. Re-run hostile browser
smoke tests after Tauri/Wry or WebKitGTK minimum-version changes.
