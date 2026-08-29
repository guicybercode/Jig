import { useId, useRef, useState } from "react";
import type { FormEvent } from "react";

import { Dialog } from "../../components/Dialog";
import { Icon } from "../../components/Icon";
import type {
  CanvasTerminalConfiguration,
  TerminalPreset,
} from "./canvas-state";

interface NewCanvasTerminalDialogProps {
  readonly defaultWorkingDirectory?: string;
  readonly onClose: () => void;
  readonly onCreate: (configuration: CanvasTerminalConfiguration) => void;
}

interface TerminalPresetOption {
  readonly value: TerminalPreset;
  readonly label: string;
  readonly shortLabel: string;
  readonly executable?: string;
}

const TERMINAL_PRESETS: readonly TerminalPresetOption[] = [
  { value: "shell", label: "Shell", shortLabel: ">_" },
  { value: "codex", label: "Codex", shortLabel: "Cx", executable: "codex" },
  { value: "claude", label: "Claude", shortLabel: "Cl", executable: "claude" },
  {
    value: "opencode",
    label: "OpenCode",
    shortLabel: "Oc",
    executable: "opencode",
  },
  { value: "custom", label: "Custom", shortLabel: "…" },
];

/** Configures a new spatial terminal without pretending to launch its process. */
export function NewCanvasTerminalDialog({
  defaultWorkingDirectory = "~",
  onClose,
  onCreate,
}: NewCanvasTerminalDialogProps) {
  const formId = useId();
  const nameId = useId();
  const commandId = useId();
  const directoryId = useId();
  const nameRef = useRef<HTMLInputElement>(null);
  const [preset, setPreset] = useState<TerminalPreset>("shell");
  const [name, setName] = useState("");
  const [executable, setExecutable] = useState("");
  const [workingDirectory, setWorkingDirectory] = useState(
    defaultWorkingDirectory,
  );
  const [error, setError] = useState<string>();

  function choosePreset(nextPreset: TerminalPreset) {
    const previous = presetOption(preset);
    const next = presetOption(nextPreset);
    setPreset(nextPreset);
    setName((current) =>
      !current.trim() || current === previous.label ? next.label : current,
    );
    setExecutable(next.executable ?? "");
    setError(undefined);
  }

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (preset === "custom" && !executable.trim()) {
      setError("Enter an executable for the custom terminal.");
      return;
    }
    const selectedPreset = presetOption(preset);
    onCreate({
      title: name.trim() || selectedPreset.label,
      preset,
      executable: executable.trim() || undefined,
      workingDirectory: workingDirectory.trim() || undefined,
    });
  }

  return (
    <Dialog
      open
      title="New Terminal"
      description="Choose what this canvas terminal should run when a live session is attached."
      size="medium"
      initialFocusRef={nameRef}
      onClose={onClose}
      footer={
        <>
          <button
            className="button button--secondary"
            type="button"
            onClick={onClose}
          >
            Cancel
          </button>
          <button
            className="button button--primary"
            type="submit"
            form={formId}
          >
            Create terminal
          </button>
        </>
      }
    >
      <form
        id={formId}
        className="canvas-terminal-form"
        noValidate
        onSubmit={handleSubmit}
      >
        <fieldset className="canvas-terminal-presets">
          <legend>Quick start</legend>
          <div>
            {TERMINAL_PRESETS.map((option) => (
              <label key={option.value}>
                <input
                  type="radio"
                  name="terminal-preset"
                  value={option.value}
                  checked={preset === option.value}
                  onChange={() => choosePreset(option.value)}
                />
                <span className="canvas-terminal-preset__icon" aria-hidden="true">
                  {option.value === "shell" ? (
                    <Icon name="terminal" />
                  ) : (
                    option.shortLabel
                  )}
                </span>
                <span>{option.label}</span>
              </label>
            ))}
          </div>
        </fieldset>

        <div className="canvas-terminal-form__fields">
          <label htmlFor={nameId}>Terminal name</label>
          <input
            ref={nameRef}
            id={nameId}
            value={name}
            maxLength={80}
            placeholder="Terminal name"
            onChange={(event) => setName(event.currentTarget.value)}
          />

          <label htmlFor={commandId}>Command</label>
          <input
            id={commandId}
            value={executable}
            maxLength={256}
            aria-invalid={error ? true : undefined}
            aria-describedby={error ? `${commandId}-error` : undefined}
            placeholder="Login shell or executable"
            onChange={(event) => {
              setExecutable(event.currentTarget.value);
              setError(undefined);
            }}
          />

          <label htmlFor={directoryId}>Working directory</label>
          <input
            id={directoryId}
            value={workingDirectory}
            maxLength={1_024}
            placeholder="~"
            onChange={(event) => setWorkingDirectory(event.currentTarget.value)}
          />
        </div>

        {error ? (
          <p id={`${commandId}-error`} className="field__error" role="alert">
            {error}
          </p>
        ) : null}
        <p className="canvas-terminal-form__note">
          The card is saved now. Its process starts only after a daemon session is
          attached.
        </p>
      </form>
    </Dialog>
  );
}

function presetOption(preset: TerminalPreset): TerminalPresetOption {
  return (
    TERMINAL_PRESETS.find((option) => option.value === preset) ??
    TERMINAL_PRESETS[0]!
  );
}
