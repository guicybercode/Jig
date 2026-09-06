# XIRP local parity

Sources checked 2026-09-05 against XIRP changelog through v0.25.0 (2026-09-03).
Baseline: `0ac8dd7`, `origin/refactor/canvas-only-shell`.
This matrix covers the user's local XIRP request. It complements the main
session's `docs/maestri-xirp-parity.md`; Portal product features are not silently
added to this implementation's scope.

| ID | Requirement and source | Owner / integration | Evidence / remaining work |
| --- | --- | --- | --- |
| X01 | Local project pin, rename, remove registration, non-Git folders, discover child repositories. [Projects](https://backstage.spotify.com/docs/xirp/projects) | S2 project base, S3 organization UI, S1 canvas | Baseline real daemon tests register both Git projects and plain folders; pin/archive metadata and child discovery remain pending. Project archive is a user-requested extension. |
| X02 | Initial objective delivered to agent, attachments, checkout/worktree and agent-specific options. [Sessions](https://backstage.spotify.com/docs/xirp/sessions) | S2 execution/contracts, S3 draft/context selection, S1 composer | Goal delivery requires verified runtime path, not metadata only. Attachments/options pending runtime contract. |
| X03 | Resume/fork, agent switch preserving supported history, linked shell. [Sessions](https://backstage.spotify.com/docs/xirp/sessions), [Changelog](https://backstage.spotify.com/docs/xirp/changelog) | S2 adapters/runtime, S1 canvas | Current adapter capabilities do not expose verified conversation IDs, fork or usage. Await trusted native contracts and runtime integration. Local context reuse is a separate feature. |
| X04 | Working/idle/needs-input attention separate from process lifecycle. [Sessions](https://backstage.spotify.com/docs/xirp/sessions) | S2 event source, S3 indicators, S1 canvas | PTY silence is not proof of approval/input need. Explicit hooks or native signals required. |
| X05 | Local preferences, shortcuts, supported native settings, diagnostics. [Settings](https://backstage.spotify.com/docs/xirp/settings) | S2 settings/runtime, S3 rules/skills, S1 UI | Existing diagnostics/custom-agent creation are baseline only; do not expose unsupported options. Credentials excluded from editors. |
| X06 | Global/project rules and skills, including symlinked skill directories. [Projects](https://backstage.spotify.com/docs/xirp/projects), [Changelog](https://backstage.spotify.com/docs/xirp/changelog) | S3 discovery, S2 file boundary | Need verified location inventory and safe bounded reads with origins/scopes; docs do not enumerate complete formats or precedence. Symlink policy and editing depend on file service. |
| X07a | Saved global/project prompts, searchable picker and insertion. [Changelog v0.19.1](https://backstage.spotify.com/docs/xirp/changelog) | S3 knowledge, S2 shared registration, S1 canvas insertion | Persisted CRUD and paginated search verified through real SQLite and daemon socket; isolated panel CRUD/draft tests and responsive Chromium preview passed; canvas and Linux evidence pending. See S3 report. |
| X07b | Session archive and workflow: backlog/in_progress/in_review/blocked/done. [Changelog](https://backstage.spotify.com/docs/xirp/changelog) | S2 organization metadata, S3 controls, S1 filtering | Workflow must not mutate lifecycle. Session pin is a user-requested extension. |
| X07c | Tokens/spend per session/day. [Changelog v0.18.0](https://backstage.spotify.com/docs/xirp/changelog) | S2 verified native data, S3 presentation | Source documents feature existence, not a collector/format/pricing algorithm. Currently unavailable, not zero. No proxy or PTY-based estimates. |
| X07d | PR review, Doctor, cleanup. [Changelog](https://backstage.spotify.com/docs/xirp/changelog) | S2 runtime/Git, S1 UI | Existing Git status/diagnostics/removal-token safety are baseline; review and expanded cleanup require their real contracts. |
| X08 | Agent-specific options, including Cursor. [Changelog](https://backstage.spotify.com/docs/xirp/changelog) | S2 adapter registry, S1 UI | No flags invented from agent display name; support requires installed-version tests and authoritative CLI sources. |
| X09-local | Explicit reusable local context between sessions (user request). | S3 knowledge, S1 composer, S2 delivery | Local text records persist and survive daemon restart; isolated draft insertion component and tests passed; canvas consumption remains pending. No transcript capture, autonomous summarization, or Portal connection implied. |
| X09-Portal | Shared workspaces, members, catalog links, resources, decisions/wiki. [Workspaces](https://backstage.spotify.com/docs/xirp/workspaces) | Separate authenticated connector | Portal-dependent; no account/API verification supplied. Not implemented or simulated. |
| X10 | Workspace context via MCP, manual transcript sharing, configured external integrations. [Workspaces](https://backstage.spotify.com/docs/xirp/workspaces) | Separate authenticated connector + S2 transport | Requires real account, scopes/API and explicit outbound preview. No silent publishing, account linking or telemetry. |

The [XIRP overview](https://backstage.spotify.com/docs/xirp) confirms local
projects, terminals, files, rules, skills and worktrees can operate without
Portal. XIRP's macOS platform statement does not replace CLI Master's Linux
requirement. A macOS test does not count as Linux verification.

## Completion evidence required

Each implemented row must link its commit, relevant automated tests, persistent
or filesystem effect, real socket behavior and canvas consumption. Portable
source code alone does not demonstrate execution on both supported platforms.
Unknown capabilities remain open dependencies rather than claimed parity.
