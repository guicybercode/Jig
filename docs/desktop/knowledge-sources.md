# Local rules and skills inspector

Open **Prompts and context** from the canvas toolbar or command palette, then
choose **Rules & skills**. Opening the saved library alone does not scan sources.
The inspector inventories supported global/administrator locations and the
selected registered project's supported root and nested configuration locations.
The preview shows the owning project-relative directory for each source.

Select an entry to request a read-only text preview. File contents are never
rendered as HTML, executed, saved, inserted into a prompt or sent to a terminal
by this inspector. Search only filters the returned metadata. Discovery does
not establish that Codex, Claude Code or Cursor loaded the source; origin and
precedence notes are explanatory, not observed activation state.

The saved-content editor remains mounted when switching these two sections,
so its unsaved text is retained. The inspector is unmounted when closed:
reopening obtains a fresh inventory rather than reusing a hidden capability.
Reconnect and project changes invalidate previews; reading again requires
selecting the source again. Panel navigation and preview keys do not act on
selected canvas cards.

## Safety and limits

The client sends only a registered project ID for discovery and opaque
scan/entry IDs for reading. It cannot submit a filesystem path. The daemon
keeps at most four scans, expiring after five minutes, and checks that a
project remains registered before reading its sources.

Inventory is bounded to 512 candidates, 10,000 enumerated entries and cumulative
directory depth 16. The selected root is checked first, followed by global/admin
locations and then project descendants. Source metadata
and issue lists have separate serialized byte limits. Preview is bounded to
64 KiB of UTF-8 text; truncation, unsupported scope and unavailable files are
visible, not represented as a complete or successful empty inventory.

Known skill-directory links may resolve to an external directory. Discovery
captures that target; an existing scan does not follow a later retargeting.
Rule/SKILL file symlinks and nonregular files are not read. Descriptor-relative
opening and identity checks detect replaced parents, changed files and edits
that restore the old mtime. A changed source requires rescan; the inspector is
not an arbitrary file reader or editor.

Nested traversal skips exact VCS/dependency directory names (`.git`,
`node_modules`, `vendor`, `.venv`, `venv`) and provider configuration trees
(`.agents`, `.claude`, `.cursor`, `.codex`). The supported rule/skill locations
inside the first three provider trees are scanned by their dedicated readers.
A registered root with a normally skipped basename is still eligible. Ordinary
directory links are skipped; the displayed scan policy explains these limits.

Sources above the registered project, effective session-cwd ancestry and bundled
plugins are not inventoried. Discovery does not evaluate native activation,
process imports, or inspect transcripts, credentials and agent configuration
files. Rule editing and broader source coverage remain separate parity work. Offline discovery fails visibly and
does not fabricate a source list.

## Verification boundary

Real filesystem/socket tests cover scan expiry, project removal, daemon
restart, symlinks, file changes and limits. Component tests cover explicit
reads, literal text, stale responses and reconnect. Chromium tests cover the
compact offline panel, focus on reopening, and retention of library edits.
Those tests do not prove native CLI loading or packaged WebKit behavior.
