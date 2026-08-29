import { useMemo, useState } from "react";

import { workspaceCommands, type WorkspaceCommandId } from "../../workspace/model";

interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
  onRun: (commandId: WorkspaceCommandId) => void;
}

/** Keyboard-first command launcher shared by menus and shortcuts. */
export function CommandPalette({ open, onClose, onRun }: CommandPaletteProps) {
  const [query, setQuery] = useState("");
  const commands = useMemo(
    () =>
      workspaceCommands.filter((command) =>
        command.label.toLowerCase().includes(query.trim().toLowerCase()),
      ),
    [query],
  );

  if (!open) {
    return null;
  }

  function close() {
    setQuery("");
    onClose();
  }

  return (
    <div className="dialog-backdrop">
      <div
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="command-palette-title"
      >
        <h2 id="command-palette-title">Command palette</h2>
        <label className="dialog__field">
          Search commands
          <input
            autoFocus
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            aria-controls="command-palette-results"
          />
        </label>
        <ul id="command-palette-results" role="listbox" aria-label="Commands">
          {commands.map((command) => (
            <li key={command.id} role="option">
              <button
                type="button"
                className="button button--secondary button--full"
                onClick={() => {
                  onRun(command.id);
                  close();
                }}
              >
                {command.label}
              </button>
            </li>
          ))}
        </ul>
        <button className="button button--secondary" type="button" onClick={close}>
          Close
        </button>
      </div>
    </div>
  );
}
