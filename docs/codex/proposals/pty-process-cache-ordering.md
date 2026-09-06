# PTY process snapshot ordering proposal

Prepared for S2 review on 2026-09-05. The adjacent patch changes only
`crates/session/src/runtime/process_tree.rs`; it has **not** been applied to
the runtime. No session or process lifecycle test was weakened.

Concurrent process scans can publish in completion order instead of observation
order. A late snapshot taken before a session started can permanently prune its
live root from `ProcessTree::known`. Subsequent fresh scans cannot reconstruct
ancestry after that identity has been discarded. A temporary harness reproduced
this against the unchanged source. The earlier manager-drop test left a live
orphaned shell, but its precise execution was not traced; attribution to this
race remains unconfirmed.

The proposal records scan start and completion times. Cache publication never
replaces a later-started scan with an earlier-started one. Each process tree
accepts cached scans only if collection began after its previous applied scan
finished. The completion boundary also rejects overlapping scans, whose records
need not be atomic or observed in start order. Rejected cache entries trigger a
fresh scan using the remaining original timeout. Collection remains outside the
cache mutex; PID birth, ancestry, zombie and tracking-limit checks are retained.

The strict boundary also rejects reusing the exact snapshot already applied
to that tree, so more refreshes may collect a new OS snapshot. Collection
complexity remains the existing process-snapshot cost; no new per-process scan
is added. S2 should assess cache hit rate and supervisor CPU with many sessions
as part of integration; accepting the same proven snapshot could be a separate
optimization with an unambiguous snapshot identity.

## Validation

The patch applied to a temporary copy with `patch -p1`; `git apply --check`
also passed against the current source. The actual runtime file was compared
byte-for-byte afterward and remained unchanged. Temporary sources and binaries
were removed. No PTY sessions or native agent processes were started for these
checks.

On macOS, a standalone module included the patched file and linked the already
compiled `nix` and `parking_lot` dependencies from `target/debug/deps`:

- `rustfmt --edition 2024`: passed.
- `rustc --edition=2024 --test -D warnings` and the resulting test binary:
  **11 passed**, zero failures; seven new tests and four existing tests.
- `clippy-driver` with the same compile arguments, `-W clippy::all` and
  `-W clippy::pedantic`: passed with warnings denied.
- A temporary mutation replacing the completion boundary with scan start
  failed the overlap regression as expected: only one of two proven processes
  remained. The mutation was discarded.

New tests cover delayed pre-spawn cache publication, previously observed
descendants that escape/reparent, overlapping observations, monotonic cache
publication, current cached absence, forced lifecycle scans, and exhaustion of
the original timeout. Existing tests retain PID-reuse, zombie, ancestry, and
Linux stat-parser coverage. Cache tests use private local caches and synthetic
records, avoiding shared global-cache interference.

This validates the module proposal, not full runtime integration. After S2
applies it, run the session crate tests and lifecycle acceptance on Linux and
macOS, with bounded repetition of the previously failing manager-drop test.

Source SHA-256 before applying:
`0d19016919fc39d0bf7ff121ecb5e92dd27bd15f6e839a51c8aa854e1e71d7f7`.

Validated patched-source SHA-256:
`1b7a551a8c63f8f74e472b87f0e48c9efd17f9677e4ccde080c25b95ed0c89a0`.
