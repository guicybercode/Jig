import { useEffect, useId, useRef, useState } from "react";
import type { KeyboardEvent as ReactKeyboardEvent } from "react";

import { Dialog } from "../../components/Dialog";

/** One searchable command exposed by the application command palette. */
export interface CommandPaletteCommand {
  /** Uniquely identifies the command across palette renders. */
  readonly id: string;
  /** Names the action in the list. */
  readonly label: string;
  /** Explains the action's result. */
  readonly description?: string;
  /** Adds non-visible search terms such as synonyms. */
  readonly keywords?: readonly string[];
  /** Displays the command's keyboard shortcut. */
  readonly shortcut?: string;
  /** Prevents execution while retaining command discoverability. */
  readonly disabled?: boolean;
  /** Explains how to make a disabled command available. */
  readonly disabledReason?: string;
  /** Runs the enabled command after the palette requests dismissal. */
  readonly onSelect: () => void;
}

/** Props for the controlled application command palette. */
export interface CommandPaletteProps {
  /** Controls whether the command palette is displayed. */
  readonly open: boolean;
  /** Supplies searchable commands in their preferred display order. */
  readonly commands: readonly CommandPaletteCommand[];
  /** Requests that the owner close the controlled palette. */
  readonly onClose: () => void;
  /** Overrides the visible dialog title. */
  readonly title?: string;
  /** Overrides the search-input hint. */
  readonly placeholder?: string;
  /** Overrides the message displayed when no commands match. */
  readonly emptyMessage?: string;
}

/** Renders a keyboard-navigable combobox and listbox command palette. */
export function CommandPalette({
  open,
  commands,
  onClose,
  title = "Command palette",
  placeholder = "Search commands",
  emptyMessage = "No matching commands",
}: CommandPaletteProps) {
  const [query, setQuery] = useState("");
  const [activeCommandId, setActiveCommandId] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const inputId = useId();
  const listboxId = useId();

  useEffect(() => {
    if (!open) {
      setQuery("");
      setActiveCommandId(null);
    }
  }, [open]);

  const normalizedQuery = query.trim().toLocaleLowerCase();
  const filteredCommands = commands.filter((command) =>
    commandMatchesQuery(command, normalizedQuery),
  );
  const matchedActiveIndex = filteredCommands.findIndex(
    (command) => command.id === activeCommandId,
  );
  const activeIndex =
    matchedActiveIndex >= 0 ? matchedActiveIndex : filteredCommands.length ? 0 : -1;
  const activeCommand =
    activeIndex >= 0 ? filteredCommands[activeIndex] : undefined;
  const activeOptionId =
    activeIndex >= 0 ? getOptionId(listboxId, activeIndex) : undefined;

  function closePalette() {
    setQuery("");
    setActiveCommandId(null);
    onClose();
  }

  function executeCommand(command: CommandPaletteCommand | undefined) {
    if (!command || command.disabled) {
      return;
    }

    closePalette();
    command.onSelect();
  }

  function moveActiveCommand(offset: -1 | 1) {
    if (!filteredCommands.length) {
      return;
    }

    const nextIndex =
      (Math.max(activeIndex, 0) + offset + filteredCommands.length) %
      filteredCommands.length;
    const nextCommand = filteredCommands[nextIndex];
    if (nextCommand) {
      setActiveCommandId(nextCommand.id);
    }
  }

  function handleInputKeyDown(event: ReactKeyboardEvent<HTMLInputElement>) {
    if (event.defaultPrevented || event.nativeEvent.isComposing) {
      return;
    }

    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        moveActiveCommand(1);
        break;
      case "ArrowUp":
        event.preventDefault();
        moveActiveCommand(-1);
        break;
      case "Home": {
        const firstCommand = filteredCommands[0];
        if (firstCommand) {
          event.preventDefault();
          setActiveCommandId(firstCommand.id);
        }
        break;
      }
      case "End": {
        const lastCommand = filteredCommands[filteredCommands.length - 1];
        if (lastCommand) {
          event.preventDefault();
          setActiveCommandId(lastCommand.id);
        }
        break;
      }
      case "Enter":
        if (activeCommand) {
          event.preventDefault();
          executeCommand(activeCommand);
        }
        break;
    }
  }

  return (
    <Dialog
      open={open}
      title={title}
      description="Search available actions and press Enter to run one."
      size="medium"
      initialFocusRef={inputRef}
      onClose={closePalette}
    >
      <div className="command-palette">
        <label className="command-palette__label" htmlFor={inputId}>
          Search commands
        </label>
        <input
          ref={inputRef}
          id={inputId}
          className="command-palette__input"
          type="search"
          role="combobox"
          autoComplete="off"
          spellCheck="false"
          value={query}
          placeholder={placeholder}
          aria-autocomplete="list"
          aria-controls={listboxId}
          aria-expanded="true"
          aria-activedescendant={activeOptionId}
          onChange={(event) => setQuery(event.currentTarget.value)}
          onKeyDown={handleInputKeyDown}
        />
        <ul
          id={listboxId}
          className="command-palette__list"
          role="listbox"
          aria-label="Commands"
        >
          {filteredCommands.length ? (
            filteredCommands.map((command, index) => {
              const optionId = getOptionId(listboxId, index);
              const descriptionId = command.description
                ? `${optionId}-description`
                : undefined;
              const disabledReason = command.disabled
                ? command.disabledReason ??
                  "Unavailable in the current context."
                : undefined;
              const disabledReasonId = disabledReason
                ? `${optionId}-disabled-reason`
                : undefined;
              const describedBy = joinIds(descriptionId, disabledReasonId);
              const isActive = index === activeIndex;

              return (
                <li
                  id={optionId}
                  key={command.id}
                  className={getOptionClassName(isActive, command.disabled)}
                  role="option"
                  aria-selected={isActive}
                  aria-disabled={command.disabled ? true : undefined}
                  aria-describedby={describedBy}
                  onMouseDown={(event) => event.preventDefault()}
                  onMouseEnter={() => setActiveCommandId(command.id)}
                  onClick={() => executeCommand(command)}
                >
                  <span className="command-palette__option-heading">
                    <span className="command-palette__option-label">
                      {command.label}
                    </span>
                    {command.shortcut ? (
                      <kbd className="command-palette__shortcut">
                        {command.shortcut}
                      </kbd>
                    ) : null}
                  </span>
                  {command.description ? (
                    <span
                      id={descriptionId}
                      className="command-palette__description"
                    >
                      {command.description}
                    </span>
                  ) : null}
                  {disabledReason ? (
                    <span
                      id={disabledReasonId}
                      className="command-palette__disabled-reason"
                    >
                      {disabledReason}
                    </span>
                  ) : null}
                </li>
              );
            })
          ) : (
            <li className="command-palette__empty">{emptyMessage}</li>
          )}
        </ul>
      </div>
    </Dialog>
  );
}

/** Matches a command against its visible copy, aliases, and disabled reason. */
function commandMatchesQuery(
  command: CommandPaletteCommand,
  normalizedQuery: string,
): boolean {
  if (!normalizedQuery) {
    return true;
  }

  return [
    command.label,
    command.description,
    command.disabledReason,
    ...(command.keywords ?? []),
  ]
    .filter((value): value is string => Boolean(value))
    .some((value) => value.toLocaleLowerCase().includes(normalizedQuery));
}

/** Creates a stable active-descendant target for a filtered option. */
function getOptionId(listboxId: string, index: number): string {
  return `${listboxId}-option-${index}`;
}

/** Joins only the descriptions that are present for an option. */
function joinIds(...ids: readonly (string | undefined)[]): string | undefined {
  const presentIds = ids.filter((id): id is string => Boolean(id));
  return presentIds.length ? presentIds.join(" ") : undefined;
}

/** Builds option state classes without a runtime styling dependency. */
function getOptionClassName(
  isActive: boolean,
  isDisabled: boolean | undefined,
): string {
  return [
    "command-palette__option",
    isActive ? "command-palette__option--active" : undefined,
    isDisabled ? "command-palette__option--disabled" : undefined,
  ]
    .filter((className): className is string => Boolean(className))
    .join(" ");
}
