# Backup and recovery

CLI Master stores application metadata in SQLite. Git repositories stay
where you put them. A backup of CLI Master is not a backup of your code.

## What to copy

Stop the daemon first so SQLite is idle. Closing the desktop window is not
enough if `cli-masterd` is still running.

Linux, using the XDG defaults:

```bash
cp ~/.local/share/cli-master/cli-master.db ~/backups/cli-master.db
cp -a ~/.local/state/cli-master/logs ~/backups/cli-master-logs
```

macOS:

```bash
cp ~/Library/Application\ Support/CLI\ Master/cli-master.db ~/backups/cli-master.db
cp -a ~/Library/Logs/CLI\ Master ~/backups/cli-master-logs
```

Copy the database file only. Do not copy `daemon.sock` or `daemon.lock`.
Those are runtime files.

If this Beta later creates managed worktrees under the data directory,
those directories are extra working copies. Back them up only if you still
need the uncommitted work inside. The original repository is the source of
truth for committed history.

## What not to copy

- Terminal scrollback. It lives in memory and is bounded.
- Agent environment values, tokens, or prompts. They must not be in logs.
- The Unix socket. It is recreated on a clean start.

## Restore

1. Quit CLI Master and stop `cli-masterd` if it is still running.
2. Replace `cli-master.db` with the backup.
3. Start the app again.

Migrations are forward-only. A backup from a newer schema will not open in
an older Beta. Keep the matching `0.1.0` application version.

## Daemon crash

If `cli-masterd` dies, PTY masters are gone. Metadata in SQLite survives.
On the next start, rows that were `starting`, `running`, or `idle` become
`unknown`. The daemon does not send signals to leftover PIDs. PID reuse
makes that unsafe.

You may still see an orphaned agent process in Activity Monitor or `ps`.
Inspect it yourself. Do not assume it still belongs to CLI Master.

## Recovery after a dirty worktree

Worktree removal is two-step and has no force bypass. If removal failed,
the worktree can remain `remove_pending` or `orphaned`. The Git checkout
is still on disk. Finish or abandon that work in Git, then retry removal
from a clean state.

Removing a project from the app never deletes the repository directory.
If the project is gone from the sidebar but the folder is still there, the
app did what it was designed to do.
