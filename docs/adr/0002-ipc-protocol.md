# ADR 0002: Authoritative Beta IPC contract

## Status

Accepted for Beta v0.1.

## Context

Frontend, daemon, and backend crates need one stable local protocol. Earlier
work modeled the catalog as Rust enums plus a separate payload module, while
the finalized Beta contract needs validated request values, bounded terminal
payloads, replay semantics, and state-bound destructive operations.

## Decision

`crates/core/src/wire` is the authoritative Beta v1 contract. Its `method` and
`event_name` modules define names; request, response, event, and validated value
modules define their payloads. `protocol/catalog.json` and
`apps/desktop/src/ipc` are tested mirrors for the desktop boundary.

Requests, responses, and events share versioned camelCase envelopes. Success
and failure are a tagged union:

```json
{ "kind": "response", "version": 1, "requestId": "...", "status": "error", "error": { "code": "executable_not_found", "message": "..." } }
```

Envelope names remain strings for forward compatibility. The daemon checks
known methods against `wire::method`; clients ignore event names they do not
recognize. Payload structs reject unknown fields where accepting them would
hide client/server drift.

Filesystem paths, session cwd, branch names, and destructive bypass flags are
not generic client inputs. Git inspection targets registered project/session
IDs. Worktree deletion requires `worktree.prepare_remove` followed by
`worktree.remove` with a short-lived state-bound token.

Terminal input/output uses canonical padded base64 with explicit decoded-size
limits. Subscriptions carry an output cursor and events make replay completion
or retention gaps explicit.

The desktop `protocol_info` Tauri command only exposes the Rust catalog. It is
not `system.hello`; negotiation remains a daemon request.

## Consequences

Adding or changing a command is an intentional cross-boundary change: update
Rust first, then the JSON and TypeScript mirrors and their contract tests.
Validated newtypes make invalid payloads harder to construct, at the cost of
explicit conversion at storage, Git, daemon, and desktop adapters.

The earlier parallel `IpcMethod`/`IpcEvent` enums and `payloads.rs` DTOs are
removed so there is no second authority. Error codes remain stable strings in
responses but are owned by implementing services rather than a duplicate
catalog in core.

## Alternatives considered

- Generate TypeScript from Rust immediately: deferred until schema generation
  can be added without weakening the validated Rust API.
- Keep both catalogs during migration: rejected because both compiled and could
  disagree while claiming to be authoritative.
- Accept arbitrary paths or a dirty-removal override: rejected because the
  daemon must derive and revalidate destructive targets from owned state.
