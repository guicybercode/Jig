# ADR 0003: Persistence and recovery

## Status

Accepted for Beta v0.1.

## Context

Session metadata has to survive an app restart. Live terminals cannot, once
the process that holds the PTY master is gone. Mixing those two facts into
one "the session is running" bit is how you get UIs that think a dead PID
is still Codex.

SQLite already stored `starting|running|idle|exited|failed|unknown`. The
product also needs `created` (metadata, no process yet) and `stopping`
(stop requested, process group still being signaled).

## Decision

SQLite is authoritative for metadata. `SessionManager` memory is
authoritative for PTY handles, child process groups, and the replay buffer.
Status in SQLite is a snapshot written on transitions, not on every output
chunk.

Wire timestamps are RFC 3339 UTC strings, matching the SQL schema. Epoch
milliseconds leaked JavaScript convenience into the protocol and are gone
from public DTOs.

On daemon start, any row still in a live status (`starting`, `running`,
`idle`, `stopping`) from another instance id becomes `unknown`. Created,
exited, and failed rows stay as they are. The new daemon never signals a
PID it did not spawn.

Closing the UI does not change session status. With no clients and no live
sessions, the daemon may idle-exit after five minutes. That is a later
implementation detail. The rule is already: UI disconnect is not stop.

Schema check constraints now accept `created` and `stopping`. This is still
migration version 1. Nothing has shipped. We edited `0001_initial.sql`
instead of rebuilding the table in a second migration. After this freeze,
schema changes are additive numbered files.

## Consequences

`session.create` can persist without spawning. `session.start` is a real
transition. Restart is `exited|failed|unknown -> starting`, never a jump
to `running`.

A crash mid-stop still lands in `unknown`. The UI should offer restart or
delete, not pretend the process is stopping.

Terminal scrollback remains in-memory and bounded. Reloading the UI after
the daemon is still up can replay. Reloading after the daemon died cannot.
