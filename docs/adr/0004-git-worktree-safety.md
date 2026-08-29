# ADR 0004: Git worktree safety

## Status

Accepted for Beta v0.1.

## Context

The app will create extra working copies so two agents can edit the same
project without sharing an index. Git worktrees are easy to get wrong:
deleting a dirty tree, deleting the main repo, parsing localized `git
status` text, or treating a session delete as a filesystem delete.

## Decision

Git operations go through the `crates/git` service, which runs the system
`git` binary with an argument array. No shell. No `libgit2` for v0.1. We want
the user's Git, including hooks and config they already have.

A `Worktree` row is not a `Session`. Sessions may point at a worktree.
Deleting session metadata sets `worktrees.session_id` null. It does not
remove the directory. Removing a project is rejected while sessions or
worktrees still reference it, and it never `rm -rf`s the repository.

Worktree lifecycle is `creating | active | remove_pending | orphaned`
because Git and SQLite cannot share a transaction. Partial failure has to
be visible.

Removal is two-step in the v1 contract. `worktree.prepare_remove` re-inspects
daemon-owned state and returns either explicit blockers or a short-lived token
bound to that exact clean state. `worktree.remove` requires the token and
rechecks before deletion. No request field can bypass dirty, ignored, locked,
or in-use blockers.

Structured Git status is observed runtime data. The public project DTO may
carry the latest daemon-observed repository root and branch for display, but
destructive operations never trust those client-returned fields. Branch names
also live on worktrees and structured status snapshots.

Generated branches use a slug plus a short suffix. Collisions create a new
name. They do not reuse an existing branch.

## Consequences

The Git crate implements these rules without renegotiating them. Status parsing
uses porcelain v2 with NUL delimiters. Diffs are size-capped and report
`truncated`.

Linux filesystems are case-sensitive. Path checks after normalization have
to stay inside the managed worktree root. A case-insensitive compare would
be a macOS-only accident waiting to eat a Linux tree.
