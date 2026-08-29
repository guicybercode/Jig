import {
  useId,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";

import { ModalDialog } from "../../components/ModalDialog";
import {
  workspaceCommands,
  type WorkspaceCommandId,
} from "../../workspace/model";

interface CommandPaletteProps {
  readonly open: boolean;
  readonly onClose: () => void;
  readonly onRun: (commandId: WorkspaceCommandId) => void;
}

/** Keyboard-first command launcher shared by menus and shortcuts. */
export function CommandPalette({ open, onClose, onRun }: CommandPaletteProps) {
  const [query, setQuery] = useState("");
  const searchRef = useRef<HTMLInputElement>(null);
  const resultsRef = useRef<HTMLUListElement>(null);
  const titleId = useId();
  const summaryId = useId();
  const normalizedQuery = query.trim().toLowerCase();
  const commands = workspaceCommands.filter((command) =>
    command.label.toLowerCase().includes(normalizedQuery),
  );

  if (!open) {
    return null;
  }

  function close() {
    setQuery("");
    onClose();
  }

  function run(commandId: WorkspaceCommandId) {
    close();
    onRun(commandId);
  }

  function focusResult(index: number) {
    getResultButtons(resultsRef.current)[index]?.focus();
  }

  function handleSearchKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      focusResult(0);
    }

    if (event.key === "ArrowUp") {
      event.preventDefault();
      focusResult(commands.length - 1);
    }
  }

  function handleResultsKeyDown(event: KeyboardEvent<HTMLUListElement>) {
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
      return;
    }

    const buttons = getResultButtons(resultsRef.current);
    if (buttons.length === 0) {
      return;
    }

    const activeIndex = buttons.findIndex(
      (button) => button === document.activeElement,
    );
    const nextIndex = getNextResultIndex(
      event.key,
      activeIndex,
      buttons.length,
    );

    event.preventDefault();
    buttons[nextIndex]?.focus();
  }

  const resultSummary = `${commands.length} ${
    commands.length === 1 ? "command" : "commands"
  } available.`;

  return (
    <ModalDialog
      labelledBy={titleId}
      describedBy={summaryId}
      initialFocusRef={searchRef}
      onDismiss={close}
    >
      <h2 id={titleId}>Command palette</h2>
      <label className="dialog__field">
        Search commands
        <input
          ref={searchRef}
          type="search"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={handleSearchKeyDown}
          aria-controls="command-palette-results"
        />
      </label>
      <p
        id={summaryId}
        className="dialog__status"
        role="status"
        aria-live="polite"
        aria-atomic="true"
      >
        {resultSummary}
      </p>
      <ul
        ref={resultsRef}
        id="command-palette-results"
        className="dialog__results"
        aria-label="Commands"
        onKeyDown={handleResultsKeyDown}
      >
        {commands.map((command) => (
          <li key={command.id}>
            <button
              type="button"
              className="button button--secondary button--full"
              onClick={() => run(command.id)}
            >
              {command.label}
            </button>
          </li>
        ))}
      </ul>
      <button className="button button--secondary" type="button" onClick={close}>
        Close
      </button>
    </ModalDialog>
  );
}

/** Returns command buttons in their visual and keyboard order. */
function getResultButtons(list: HTMLUListElement | null): HTMLButtonElement[] {
  if (!list) {
    return [];
  }

  return Array.from(list.querySelectorAll<HTMLButtonElement>("button"));
}

/** Resolves wraparound arrow-key navigation for command results. */
function getNextResultIndex(
  key: string,
  activeIndex: number,
  resultCount: number,
): number {
  if (key === "Home") {
    return 0;
  }

  if (key === "End") {
    return resultCount - 1;
  }

  if (key === "ArrowDown") {
    return activeIndex < 0 ? 0 : (activeIndex + 1) % resultCount;
  }

  return activeIndex <= 0 ? resultCount - 1 : activeIndex - 1;
}
