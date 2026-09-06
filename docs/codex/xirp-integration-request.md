# XIRP S3 integration request — 2026-09-05

S3 is active in `/Users/eguimacs/cli-master-xirp`, branch `feat/xirp-local-workflows`, based on latest `origin/refactor/canvas-only-shell` (`0ac8dd7`). The user explicitly requested coordination before shared changes. This file is a coordination message, not part of your commit unless useful.

## S2 runtime / shared-contract owner

Please reply in `/Users/eguimacs/cli-master-xirp/docs/codex/xirp-coordination-reply.md` or append below in this copy. I will inspect both main and runtime copies.

Proposed S3 ownership: new `knowledge/` modules under core/storage/daemon; isolated React components under `app/features/knowledge/`; domain tests and XIRP report. No parallel process executor, file service, database, or Tauri client.

First vertical increment: global/project saved prompts (UUIDv7, title/body/projectId nullable, created/updated epoch-ms, integer revision) and reusable local context documents (same scope/revision plus kind). Explicit insert creates composer draft, never hidden send. Suggest `knowledge.list`, `knowledge.save`, `knowledge.delete` with tagged kind `prompt|context`; optimistic revision on update/delete; `knowledge.updated` event for refresh. Stable existing `invalid_input`/not_found + agreed conflict error. Storage tables in current SQLite with project FK; no runtime side effects.

Next: `knowledge.discover`/`knowledge.read` for explicitly known rule/skill paths, bounded UTF-8/no credentials/no symlink traversal; edit via your file service once available (no duplicate generic file service). Please advise your file contract/reusable safe reader.

Organization metadata: propose separate project/session organization records for pinned/archived, session workflow (backlog/in_progress/in_review/blocked/done) separate from process status, initial objective + explicit local context references. Do you own these fields as planned? Please reserve/implement contracts or delegate an agreed narrow patch. Initial objective execution must use SessionManager/adapters; S3 can own drafting/context composition.

Please confirm migration allocation, shared lib.rs registration, wire/mirrors/dispatch ownership. Can you delegate a small additive S3 patch to these files after review of this proposal, or supply a commit? No shared files have been changed by S3.

## S1 canvas owner

Will deliver isolated components with typed callbacks for saved prompts/context library, rules/skills inspector, and organization controls. Please identify preferred integration surface (canvas contextual panel/toolbar/palette) and coordinate real mounting/composer insertion. Your AppShell/canvas/global styles remain yours. S3 will publish commits and integration API in `docs/codex/xirp-context-report.md`; main canvas remains Maestri.

## Responses

Pending owner acknowledgment.
