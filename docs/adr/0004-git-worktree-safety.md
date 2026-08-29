# ADR 0004: Git worktree safety

## Status

Accepted for Beta v0.1.

## Context

The app will create extra working copies so two agents can edit the same
project without sharing an index. Git worktrees are easy to get wrong:
deleting a dirty tree, deleting the main repo, parsing localized `git
status` text, or treating a session delete as a filesystem delete.

## Decision

Git operations go through a future `crates/git` service that runs the
system `git` binary with an argument array. No shell. No `libgit2` for
v0.1. We want the user's Git, including hooks and config they already have.

A `Worktree` row is not a `Session`. Sessions may point at a worktree.
Deleting session metadata sets `worktrees.session_id` null. It does not
remove the directory. Removing a project is rejected while sessions or
worktrees still reference it, and it never `rm -rf`s the repository.

Worktree lifecycle is `creating | active | remove_pending | orphaned`
because Git and SQLite cannot share a transaction. Partial failure has to
be visible.

Removal is two-step even though the v1 catalog currently exposes one
`worktree.remove` method. Omitting `confirmationToken` inspects state and
returns a token. Sending the token performs the delete after a recheck.
`allowDirty` defaults to false. Dirty removal is explicit.

`GitStatus` is observed runtime data. It is not stored on `Project`. The
project path is the canonical repository root. Branch names live on
worktrees and on Git status snapshots, not as a stale field on the project
row.

Generated branches use a slug plus a short suffix. Collisions create a new
name. They do not reuse an existing branch.

## Consequences

The Git crate can land later without renegotiating these rules. Status
parsing must use porcelain v2 with NUL delimiters. Diffs are size-capped
and report `truncated`.

Linux filesystems are case-sensitive. Path checks after normalization have
to stay inside the managed worktree root. A case-insensitive compare would
be a macOS-only accident waiting to eat a Linux tree.
