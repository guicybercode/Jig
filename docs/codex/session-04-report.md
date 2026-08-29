# Session 04 report: Git worktree isolation

This session added `crates/git` (`cli-master-git`). The crate is the only place that runs Git. The desktop UI still does not invoke Git.

Git is the system binary resolved from `PATH`. Every invocation is `Command::new(executable)` plus a separate argument array. Paths, refs, and branch names never go through a shell string.

## Git commands

Read operations prefix Git with `--no-optional-locks --no-pager -c core.quotepath=false -C <path>` and set `GIT_TERMINAL_PROMPT=0`, `GIT_PAGER=cat`, `PAGER=cat`, and `LC_ALL=C`.

| Operation | Git arguments |
|---|---|
| Inside a work tree | `rev-parse --is-inside-work-tree` |
| Bare check | `rev-parse --is-bare-repository` |
| Repository root | `rev-parse --show-toplevel` |
| Git directory | `rev-parse --absolute-git-dir` |
| Common directory | `rev-parse --git-common-dir` |
| Current branch | `symbolic-ref --short HEAD` |
| Current commit | `rev-parse --verify HEAD` |
| Resolve a base ref | `rev-parse --verify --end-of-options <ref>^{commit}` |
| Local branches | `for-each-ref --format=%(refname:short) refs/heads` |
| Status | `status --porcelain=v2 -z --branch --untracked-files=all` |
| Diff (unstaged) | `diff --no-color --no-ext-diff` |
| Diff (staged) | `diff --no-color --no-ext-diff --cached` |
| Binary detection | `diff --numstat --no-ext-diff` (and `--cached` when staged) |
| List worktrees | `worktree list --porcelain -z` |
| Create worktree | `worktree add --no-track -b <branch> <path> <commit>` |
| Remove worktree | `worktree remove <path>` (no `--force`) |
| Rollback branch | `branch -d <branch>` (no `-D`) |

The crate never runs `git reset --hard`, `git clean`, `git worktree remove --force`, or `git branch -D`.

## Branch and directory names

Session title `Implement OAuth Refresh` becomes branch `agent/implement-oauth-refresh` and directory leaf `implement-oauth-refresh`.

The slug path:

1. Unicode lowercase, then NFKD.
2. Keep ASCII letters and digits.
3. Turn separators and path punctuation into a single hyphen, which collapses `../` into ordinary slug pieces instead of traversal.
4. Cap length at 48 characters.
5. Replace empty or reserved names (`HEAD`, `CON`, `.`, `..`, and the usual Windows device names) with `session`.

The worktree directory is always `<managed-root>/<project-id>/<leaf>`. After normalization the path must stay under the managed root.

If `agent/<slug>` is taken, the allocator appends the last eight hex characters of the session UUID, then `-2`, `-3`, and so on. Two sessions with the same title therefore cannot receive the same branch by accident. `ExistingBranchBehavior::Reject` fails instead of suffixing, which is the explicit collision policy for callers that want that.

## Worktree creation and rollback

Creation:

1. Detect a non-bare repository.
2. Refuse unborn `HEAD` when the base ref is `HEAD`.
3. Resolve the base ref to a hex object id, after rejecting values that start with `-`. The object id is what later goes to `worktree add`, not the original user string.
4. Allocate a unique branch and directory.
5. Create missing parent directories under the managed root only.
6. Run `git worktree add --no-track -b <branch> <path> <commit>`.
7. Inspect the new worktree and check branch and `HEAD`.
8. Return path, branch, and commit. The crate does not write SQLite. Persistence is the caller's job after this success.

If `worktree add` fails, empty parent directories created in this call are removed. If add succeeds and confirmation fails, rollback runs `git worktree remove <path>` without `--force`, removes an empty leftover directory, and deletes the new branch only with `git branch -d`. If that cleanup also fails, the error is `GIT_ROLLBACK_FAILED` and includes both failures so a human can inspect leftovers.

## Removal

Removal is two-step and has no hidden force flag.

`prepare_remove_worktree` inspects the path and returns findings. A confirmation token is issued only when there are no blockers. Dirty directory removal never gets a token.

Blockers:

- the path is not a Git worktree of this repository
- the path is the primary checkout
- the path is outside the managed root
- the requested worktree path itself is a symlink
- Git reports the worktree as locked
- a session is still using it (`session_is_active`)
- the worktree is dirty (staged, unstaged, or untracked), for `RemoveScope::Directory`

`remove_worktree` requires the token, rechecks the fingerprint (path, `HEAD`, dirty, locked, scope), and refuses if anything changed. Directory scope runs `git worktree remove` without `--force`. `RemoveScope::MetadataOnly` leaves Git and the directory alone so the caller can drop application metadata.

Tokens live five minutes in process memory. An empty token is `GIT_CONFIRMATION_REQUIRED`.

## Status and diff

Status is porcelain v2 with NUL delimiters. The parser never reads localized `git status` text. Ahead/behind come from `# branch.ab` when Git already has an upstream. There is no fetch.

Diff is capped at 2 MiB by default. Crossing the cap sets `truncated` and kills the Git process so a huge patch cannot fill memory. `git diff --numstat` lines of `-	-	<path>` are recorded as binary paths. `Binary files` / `GIT binary patch` lines are dropped. Non-UTF-8 stdout sets `invalid_output` and returns an empty patch instead of panicking.

## Protections

- Executable and arguments stay separate. A path like `name; true` is a missing directory, not a shell command.
- Refs cannot start with `-`. `--end-of-options` is passed to `rev-parse`.
- Worktree paths are checked with lexical normalization plus resolving the longest existing prefix. That keeps `/var` vs `/private/var` on macOS from looking like an escape, without assuming a case-insensitive disk.
- A symlink passed as the worktree path is reported as a blocker for either removal scope, so prepare-remove never issues it a token. System ancestor links such as macOS `/var` → `/private/var` are not treated as an attack.
- The primary repository worktree cannot be removed.
- Dirty worktrees cannot be removed through this crate.

## Tests

Unit tests cover slug generation, collision suffixes, and porcelain v2 parsing.

Integration tests in `crates/git/tests/git_service.rs` use real temporary repositories and the system Git binary:

- valid root and subdirectory
- missing path and non-Git directory
- unborn repository, then first commit
- isolated worktree creation
- two sessions with the same title getting different branches
- `Reject` when the unsuffixed branch exists
- clean status, dirty status, and textual diff
- binary files and truncated diffs
- dirty removal refused, including an empty token
- clean removal after an explicit token
- metadata-only removal leaving the directory, unless a session is active
- primary worktree, active session, and symlink blockers
- `worktree add` failure on a read-only parent, with no leftover branch
- paths with spaces and Unicode session names
- detached `HEAD` as a base ref
- recovery action on `ApiError`

## Limitations

- The crate talks to Git only. SQLite `creating` / `active` / `orphaned` states belong to storage and the daemon, which are not wired in this session.
- Dirty removal cannot be confirmed. Architecture.md mentions an explicit `allowDirty` path. This crate refuses dirty directory removal outright so there is no force-shaped flag to misuse.
- Ahead/behind is whatever porcelain v2 already knows. There is no network fetch.
- `git worktree add --no-track` needs a Git that forwards `--no-track` from `worktree add -b`. Linux and current macOS CI images do. Very old Git is not a target.
- Rollback cannot be transactional with SQLite. A daemon caller still needs the `creating` row from the architecture if it persists before Git returns.
- Worktrees created outside the managed root can be listed, but directory removal of those paths is blocked.
- Binary diffs list paths and omit patch bytes. There is no hex dump.
- Confirmation tokens are in-memory only. A daemon restart forgets pending prepare-remove tokens.
