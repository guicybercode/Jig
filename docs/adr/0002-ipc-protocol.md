# ADR 0002: IPC protocol

## Status

Accepted for Beta v0.1.

## Context

Frontend and backend were already drifting. `ARCHITECTURE.md` sketched
envelopes with `ok: false`. `crates/core` implemented a tagged
`status: success | error` body. Methods were free strings. The Tauri
template shipped `greet`. The agents crate used catalog keys (`codex`)
while core used UUID `AgentId` values. None of that can survive two
parallel Codex sessions.

## Decision

One domain protocol, version 1, owned by `crates/core`.

Requests, responses, and events share an envelope with `kind`, `version`,
and camelCase field names. Success and failure are a tagged union:

```json
{ "kind": "response", "version": 1, "requestId": "...", "status": "error", "error": { "code": "AGENT_EXECUTABLE_NOT_FOUND", "message": "..." } }
```

That shape is stricter than a boolean `ok` field. TypeScript can narrow on
`status`. We kept the implementation and changed the docs to match it.

Method and event names are Rust enums with serde renames (`session.create`,
`session.status_changed`). Unknown names decode as `Unknown`. The daemon
must fail unknown methods. Clients must ignore unknown events.

The catalog is locked in three artifacts that tests compare:

- `IpcMethod` / `IpcEvent` in Rust
- `protocol/catalog.json`
- `apps/desktop/src/ipc/methods.ts`

Payload structs live in `crates/core/src/payloads.rs`. TypeScript mirrors
them by hand. We did not add a codegen crate. Codegen would have been
another moving part before a daemon exists. The shared JSON catalog is
enough to catch a renamed method.

PTY bytes travel as standard base64 in `dataBase64`. JSON is UTF-8. Raw
terminal bytes are not.

The desktop `protocol_info` Tauri command only echoes the catalog. It is
not `system.hello`. Hello still belongs to the daemon handshake.

## Consequences

Adding a command is a cross-crate change, which is the point. A session
that needs `session.kill` or `session.subscribe` must extend the catalog
instead of opening a side channel.

Error codes are additive string constants in `error_codes`. Do not rename
them. Do not put env values, tokens, or terminal contents in `details`.
