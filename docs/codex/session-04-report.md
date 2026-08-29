# Session 04 report: Git worktree isolation

This session established `cli-master-git` as the only crate that invokes Git.
The implementation described here is the reconciled version that follows
[ADR 0004](../adr/0004-git-worktree-safety.md); the desktop and daemon consume
the crate instead of constructing Git commands themselves.

## Process boundary

Git is discovered on `PATH`, verified with `git --version`, and invoked directly
with an executable plus an argument array. No operation builds a shell command.
Every child receives `GIT_TERMINAL_PROMPT=0`, `GIT_PAGER=cat`, and `LC_ALL=C`.

Commands have bounded stdout and stderr, a default 15-second timeout, and no
stdin. On Unix, a timed-out Git process and its helpers are terminated as a
process group so inherited output pipes cannot hang the application.

The relevant command shapes are:

| Operation | Git arguments |
|---|---|
| Repository root | `rev-parse --path-format=absolute --show-toplevel` |
| Common directory | `rev-parse --path-format=absolute --git-common-dir` |
| Current branch | `symbolic-ref --quiet --short HEAD` |
| Starting commit | `rev-parse --verify HEAD^{commit}` |
| Verify planned commit | `cat-file -e <oid>^{commit}` |
| Validate branch | `check-ref-format --branch <branch>` |
| Check branch collision | `show-ref --verify --quiet refs/heads/<branch>` |
| Status | `status --porcelain=v2 --branch -z --untracked-files=all` |
| Removal status | status above plus `--ignored=matching` |
| Hidden index flags | `ls-files -v -z` |
| Diff | `-c color.ui=false diff --no-color --no-ext-diff --no-textconv --text HEAD --` |
| List worktrees | `worktree list --porcelain -z` |
| Create worktree | `worktree add -b <branch> <path> <commit>` |
| Remove worktree | `worktree remove <path>` |

The crate exposes no reset, clean, branch deletion, or forced worktree removal
operation.

## Branch and directory names

A task such as `Implementação OAuth` with short ID `abc123` becomes
`agent/implementacao-oauth-abc123`. The worktree directory starts with the
branch leaf, `implementacao-oauth-abc123`.

Naming is deterministic:

1. Normalize the task label with NFKD and lowercase it.
2. Preserve ASCII letters and digits, discard combining marks, and collapse
   other separators into `-`.
3. Cap the task slug at 48 bytes.
4. Map empty labels and reserved Git/path names (`HEAD`, `.git`, `CON`, Windows
   device names, and related sentinels) to `task`.
5. Normalize the caller-provided short ID to at most 12 ASCII alphanumeric
   characters and always include it in the base branch.
6. Add `-2`, `-3`, and so on only when an existing local branch collides.

Inputs such as `../../../etc/passwd` therefore become the ordinary leaf
`etc-passwd`; they cannot carry traversal components into the destination.
Git still validates every generated ref with `check-ref-format --branch`.

## Planning, creation, and recovery

Planning is side-effect free. A `WorktreePlan` binds the operation to:

- the canonical repository root and physical Git common-directory identity;
- the exact full `HEAD` object ID;
- a canonically intended managed root;
- a collision-free branch and descendant destination.

Creation revalidates those values immediately before `git worktree add`. It
then confirms the registered path, branch, commit, detached state, and prunable
state. This prevents a stale plan from targeting a recreated repository,
retargeted symlink, occupied path, new branch, or unavailable commit.

Git and SQLite cannot share a transaction, so a failed or timed-out add is
reconciled against three observable effects: branch existence, path existence,
and the Git worktree registry. Cleanup runs only when all three prove the exact
planned identity and the worktree is clean. Cleanup uses `git worktree remove`
without `--force` and deliberately preserves the branch. Ambiguous or partial
state returns an actionable partial-worktree error and is never retried or
deleted automatically.

## Removal

Removal is conservative and state-bound. `prepare_remove` requires a registered
linked worktree below the canonical managed root and rejects the primary
checkout. Its snapshot records Git identity, status, ignored files,
`assume-unchanged` and `skip-worktree` entries, lock state, and caller-supplied
runtime usage.

`remove_worktree` reads that complete snapshot twice while the caller retains
its session exclusion guard. Any difference aborts removal. The following are
blockers:

- staged, tracked, untracked, or ignored content;
- `assume-unchanged` or `skip-worktree` index entries;
- a Git worktree lock;
- a running agent or any other live owner.

Only two identical, blocker-free snapshots reach `git worktree remove`, and
the crate never passes `--force` or deletes the associated branch.

## Status, diff, and tests

Status and worktree data use NUL-delimited porcelain output rather than
localized human text. Repository-relative paths preserve non-UTF-8 bytes on
Unix. Diff output includes staged and unstaged changes relative to `HEAD` (or
the index relative to the empty tree for an unborn repository), disables color
and external/textconv drivers, and never exceeds the caller's byte limit after
lossy UTF-8 conversion.

Unit and real-repository integration tests under `crates/git/tests/` cover:

- repository inspection, naming/transliteration, collisions, and unborn HEAD;
- bounded diff and structured dirty status;
- side-effect-free plans and exact-OID creation;
- symlink, path occupancy, repository-replacement, branch, and destination races;
- post-effect failures, timeouts, conservative compensation, and repeated stress;
- dirty, ignored, hidden-index, locked, running, in-use, and out-of-root removal
  blockers;
- proof that cleanup/removal never receives `--force` and preserves branches.

Persistence remains a daemon/storage responsibility. This crate does not fetch
remotes, delete branches, or infer whether a session is live; callers must
supply runtime usage under their own exclusion guard.
