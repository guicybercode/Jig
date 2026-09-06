# Uninstall

Uninstalling Jig removes the application. It does not remove your
Git repositories.

## Linux AppImage

Delete the AppImage file. There is no system package to purge.

Optional: remove application state.

```bash
rm -rf ~/.local/share/cli-master
rm -rf ~/.config/cli-master
rm -rf ~/.cache/cli-master
rm -rf ~/.local/state/cli-master
rm -rf "${XDG_RUNTIME_DIR:-/tmp}/cli-master"
```

If you overrode XDG variables, delete those directories instead.

## macOS

Drag `Jig.app` to the Trash, or delete it from Applications.

Optional: remove application state.

```bash
rm -rf ~/Library/Application\ Support/CLI\ Master
rm -rf ~/Library/Caches/CLI\ Master
rm -rf ~/Library/Logs/CLI\ Master
rm -rf /tmp/cli-master-*
```

The `/tmp/cli-master-*` names include a hash of your home path. Removing
only the directory that belongs to your user is enough.

## After uninstall

- Project folders you added are still on disk.
- Agent CLIs you installed separately are still installed.
- A backup of `cli-master.db` is enough to restore metadata later. See
  [backup-and-recovery.md](backup-and-recovery.md).
