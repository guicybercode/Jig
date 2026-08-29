import { useId, useRef, useState, type KeyboardEvent } from "react";

import { Dialog } from "../../components/Dialog";

/** Describes one action exposed by the command palette. */
export interface CommandPaletteCommand {
  readonly id: string;
  readonly label: string;
  readonly onSelect: () => void;
  readonly disabled?: boolean;
  readonly disabledReason?: string;
}

interface CommandPaletteProps {
  readonly open: boolean;
  readonly commands: readonly CommandPaletteCommand[];
  readonly onClose: () => void;
}

/** Keyboard-first launcher for actions backed by the current workspace controller. */
export function CommandPalette({
  open,
  commands,
  onClose,
}: CommandPaletteProps) {
  const [query, setQuery] = useState("");
  const searchId = useId();
  const resultsId = useId();
  const resultsRef = useRef<HTMLUListElement>(null);
  const normalizedQuery = query.trim().toLocaleLowerCase();
  const matchingCommands = commands.filter((command) =>
    command.label.toLocaleLowerCase().includes(normalizedQuery),
  );
  const availableCommandCount = matchingCommands.filter(
    (command) => !command.disabled,
  ).length;

  function close() {
    setQuery("");
    onClose();
  }

  function run(command: CommandPaletteCommand) {
    if (command.disabled) {
      return;
    }
    close();
    command.onSelect();
  }

  function focusResult(index: number) {
    getResultButtons(resultsRef.current)[index]?.focus();
  }

  function handleSearchKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    const buttons = getResultButtons(resultsRef.current);
    if (buttons.length === 0) {
      return;
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      focusResult(0);
      return;
    }

    if (event.key === "ArrowUp") {
      event.preventDefault();
      focusResult(buttons.length - 1);
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

  const resultSummary = `${matchingCommands.length} matching ${
    matchingCommands.length === 1 ? "command" : "commands"
  }; ${availableCommandCount} available.`;

  return (
    <Dialog title="Command palette" open={open} onClose={close}>
      <div className="command-palette">
        <label htmlFor={searchId}>Search commands</label>
        <input
          id={searchId}
          type="search"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={handleSearchKeyDown}
          aria-controls={resultsId}
          autoComplete="off"
        />
        <p
          className="command-palette__summary"
          role="status"
          aria-live="polite"
          aria-atomic="true"
        >
          {resultSummary}
        </p>
        <ul
          ref={resultsRef}
          id={resultsId}
          className="command-palette__results"
          aria-label="Commands"
          onKeyDown={handleResultsKeyDown}
        >
          {matchingCommands.length === 0 ? (
            <li className="command-palette__empty">No matching commands.</li>
          ) : (
            matchingCommands.map((command, index) => {
              const reasonId = `${resultsId}-reason-${index}`;
              return (
                <li key={command.id}>
                  <button
                    type="button"
                    className="button button--secondary button--full"
                    disabled={command.disabled}
                    aria-describedby={
                      command.disabled && command.disabledReason
                        ? reasonId
                        : undefined
                    }
                    onClick={() => run(command)}
                  >
                    {command.label}
                  </button>
                  {command.disabled && command.disabledReason ? (
                    <span
                      id={reasonId}
                      className="command-palette__unavailable"
                    >
                      {command.disabledReason}
                    </span>
                  ) : null}
                </li>
              );
            })
          )}
        </ul>
        <div className="dialog__actions">
          <button
            className="button button--secondary"
            type="button"
            onClick={close}
          >
            Close
          </button>
        </div>
      </div>
    </Dialog>
  );
}

/** Returns enabled command buttons in visual and keyboard order. */
function getResultButtons(list: HTMLUListElement | null): HTMLButtonElement[] {
  if (!list) {
    return [];
  }

  return Array.from(
    list.querySelectorAll<HTMLButtonElement>("button:not([disabled])"),
  );
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
