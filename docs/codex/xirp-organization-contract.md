# S3 organization contract proposal

Status: S2 approved ownership and reserved migration 0005_organization.sql
in xirp-coordination-reply.md. This document records the proposal; no
organization IPC names or handlers are implemented by this document.

The original user task assigns new organization/context modules to S3 and
shared contracts to S2. This proposal resolves the earlier replies' ownership
difference with one bounded patch, independent of runtime/session execution.

## Suggested wire contract

- `organization.get {targets: OrganizationTarget[]}` returns `{entries}` in
  request order. Between 1 and 100 distinct targets per call. Clients already
  obtain project/session identities from the existing snapshot/list methods;
  they can batch those IDs without a second session/project registry.
- `organization.save {target,expectedRevision,pinned,archived,workflow}` returns
  one `OrganizationEntry`. A project requires `workflow:null`; a session
  requires one of `backlog|in_progress|in_review|blocked|done`. No partial patch
  semantics: the optimistic revision protects the entire organization record.
- A target is `{kind:"project",id:ProjectId}` or `{kind:"session",id:SessionId}`.
- An entry has `{target,pinned,archived,workflow,revision,updatedAtMs}`. Existing
  entities without organization rows return defaults: false/false, null for
  project or backlog for session, revision 0, updatedAtMs null. No DB row is
  written by reads. The first explicit save requires expectedRevision 0 and
  creates revision 1 with epoch-ms updatedAtMs. Subsequent saves require the
  current positive JS-safe revision and increment it, even for reverting all
  flags to defaults; rows are not deleted during ordinary edits.

Unknown entities fail with `project_not_found`/`session_not_found` (the batch
is all-or-error). Stale saves return `organization_conflict`. Exhausted
revisions fail rather than wrap. Malformed payloads return `invalid_payload`.
No native process status, PID, worktree state, transcript or text body is
accepted by this API.

## Persistence and concurrency

Proposed migration number must be allocated by S2. Add two tables,
`project_organization` and `session_organization`, each keyed by its existing
entity ID with an ON DELETE CASCADE foreign key. Keep metadata separate from
the process-owned session row. Use immediate write transactions for compare
and save, and one read snapshot for a get batch. Database constraints enforce
booleans, workflow values, revision/timestamp ranges and typed relationships.

Pin and archive preserve files, saved knowledge, PTYs and session status.
Archiving a running session is permitted and changes only visibility metadata;
UI makes continued execution apparent. Workflow changes never start, stop or
signal any process. Deleting the underlying metadata removes its organization
row through FK only; it does not delete a repository/worktree directory.

## UI and integration

S3 can provide isolated selected-project/session controls and a typed batch
reader for S1's canvas/palette filters. S1 continues to own selection, sorting,
archived visibility and canvas mounting. Conflicts preserve the desired flags
and offer explicit refresh/retry with the newly observed revision.

General metadata transport remains S2's future work; initial controls refresh
after save/reconnect and provide manual refresh. No unimplemented event name
will be added. Initial objective delivery remains a separate S2 adapter/runtime
contract; source-backed editable drafts are S3/S1's composition boundary.

## Required evidence

Real SQLite: defaults, persistence/reopen, concurrent first insert/update,
revision conflict/exhaustion, batch consistency, invalid/duplicate targets,
FK cascade and file preservation. Real socket: project/session workflow and
archive changes leave runtime snapshots/processes unchanged. UI: explicit
controls, stale response guards, visible running-state warning while archived,
conflict preservation. S1 mounting and Linux/macOS CI remain separate evidence.
