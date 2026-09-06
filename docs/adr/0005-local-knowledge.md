# ADR 0005: Local prompts, reusable context, and organization

Status: proposed; shared contract and migration registration await S2 agreement.

## Context

CLI Master needs global/project prompts, reusable local context, and work
organization without assuming ownership of coding agents. The XIRP reference
separates optional Portal workspaces from local project/terminal operations.
The canvas remains the primary UI. Linux and macOS remain first-class.

## Decision

`knowledge` modules hold explicit user-authored saved prompts and context
records. Core types are pure. Storage uses the existing SQLite database;
daemon handlers expose the agreed versioned wire contract. No additional
Tauri client, file service, process executor, or database is introduced.

A record has a UUIDv7 identity, a `prompt` or `context` kind, a nullable project
ID (null means global), title/body, monotonically increasing integer revision,
and epoch-millisecond timestamps. Project queries include project and global
records; global queries include only global records. Project deletion must
have a documented foreign-key policy, while archive preserves all records.

Creation omits ID and expected revision. Updates/deletes supply both identity
and the last observed revision. A concurrent edit fails with a stable conflict
error and preserves the editor draft. Save-as-copy is explicit. Input and list
response sizes are bounded to fit the daemon's transport limits. Content must
not appear in Debug, error details, or logs.

Selecting a saved prompt or context inserts editable text in the composer.
Only a separate explicit runtime action delivers it to an agent. Saving text
never writes terminal input. Reusing context is not restoring a vendor
conversation and does not persist a terminal transcript. The initial objective
must eventually be delivered through SessionManager/verified adapter options;
a stored text field alone is not completion of that requirement.

Project/session pin/archive and session workflow are organization metadata.
They neither replace process status nor signal a process. Workflow states are
`backlog`, `in_progress`, `in_review`, `blocked`, and `done`; archive is a
separate flag. The runtime owner coordinates their shared types/storage.

Rule/skill discovery inventories explicitly supported locations with origin,
format, scope, and precedence explanations. It reuses the runtime owner's
file-access boundary; discovered content is data and is never executed.
Credential paths are excluded. Symlinks, external changes, size/count limits,
encoding, and edit conflicts must have explicit behavior and real filesystem
tests. Reference support for symlinked skill directories is a parity item,
not permission to traverse arbitrary links or read unrelated home files.

Agent-specific resume/fork/options require verified adapter capabilities.
Attention needs explicit trustworthy events. Token/cost usage needs native
records with provenance; unknown data is unavailable, never inferred from
terminal byte counts, silence, or process duration.

## Integration and dependencies

S2 owns shared Rust wire registration, catalog mirrors, migrations, daemon
composition/dispatch, runtime/adapters, and the generic file service. S3 sends
small contracts before editing those boundaries. S1 owns mounting the isolated
components in the canvas/palette and composer draft insertion. Working
components and green unit tests do not prove their integrated application flow.

Portal authentication, shared workspaces, MCP exposure and transcript sharing
remain separate capabilities requiring real contracts/account verification.
Local context is a CLI Master extension and is not branded as Portal support.

## Verification

Contract tests reject malformed inputs. Persistence tests use real SQLite
files, reopen, competing connections, project scope, and FK behavior. Socket
roundtrips prove handlers and effects. Frontend tests use typed callbacks/the
project IPC client. Canvas integration, Linux/macOS CI and runtime evidence
are tracked separately from isolated component tests.
