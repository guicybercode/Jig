import { useEffect, useId, useRef, useState, type FormEvent } from "react";

import { Dialog } from "../../components/Dialog";
import { Icon } from "../../components/Icon";
import type { AgentRecord, CustomAgentInput } from "../../../ipc/agentTypes";
import {
  hasFieldErrors,
  validateCustomAgent,
  type FieldErrors,
} from "../../../ipc/validateCustomAgent";

interface EnvRow {
  key: string;
  value: string;
}

interface CustomAgentDialogProps {
  readonly agent?: AgentRecord;
  readonly onClose: () => void;
  readonly onSave: (input: CustomAgentInput) => Promise<void>;
}

function toInput(
  displayName: string,
  executable: string,
  args: string[],
  env: EnvRow[],
  defaultCwd: string,
  requiresPty: boolean,
): CustomAgentInput {
  return {
    displayName,
    executable,
    args: args.filter((argument) => argument.length > 0),
    env: env.filter((row) => row.key.trim().length > 0),
    defaultCwd,
    requiresPty,
  };
}

/** Create or edit a custom agent using structured fields, never a shell string. */
export function CustomAgentDialog({
  agent,
  onClose,
  onSave,
}: CustomAgentDialogProps) {
  const nameId = useId();
  const executableId = useId();
  const cwdId = useId();
  const ptyId = useId();
  const formErrorId = useId();
  const nameRef = useRef<HTMLInputElement>(null);
  const [displayName, setDisplayName] = useState(agent?.displayName ?? "");
  const [executable, setExecutable] = useState(agent?.executable ?? "");
  const [args, setArgs] = useState<string[]>(
    agent?.defaultArgs.length ? [...agent.defaultArgs] : [""],
  );
  const [env, setEnv] = useState<EnvRow[]>(
    agent?.envKeys.length
      ? agent.envKeys.map((key) => ({ key, value: "" }))
      : [{ key: "", value: "" }],
  );
  const [defaultCwd, setDefaultCwd] = useState(agent?.defaultCwd ?? "");
  const [requiresPty, setRequiresPty] = useState(agent?.requiresPty ?? true);
  const [errors, setErrors] = useState<FieldErrors>({});
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    nameRef.current?.focus();
  }, []);

  const nameErrorId = `${nameId}-error`;
  const executableErrorId = `${executableId}-error`;
  const argsErrorId = `${executableId}-args-error`;
  const envErrorId = `${executableId}-env-error`;
  const cwdErrorId = `${cwdId}-error`;

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const input = toInput(
      displayName,
      executable,
      args,
      env,
      defaultCwd,
      requiresPty,
    );
    const nextErrors = validateCustomAgent(input);
    setErrors(nextErrors);
    setSubmitError(null);
    if (hasFieldErrors(nextErrors)) {
      if (nextErrors.displayName) {
        nameRef.current?.focus();
      }
      return;
    }
    setSaving(true);
    try {
      await onSave(input);
      onClose();
    } catch (error) {
      setSubmitError(error instanceof Error ? error.message : "Could not save the agent.");
    } finally {
      setSaving(false);
    }
  }

  return (
    <Dialog
      title={agent ? "Edit custom agent" : "Add custom agent"}
      open
      onClose={onClose}
      initialFocusRef={nameRef}
    >
      <form className="form" onSubmit={handleSubmit} noValidate>
        <p className="form__hint">
          Executable and arguments stay separate. Do not paste a shell command.
          Environment values are masked and never written to diagnostics.
        </p>
        <label className="field" htmlFor={nameId}>
          <span>Name</span>
          <input
            ref={nameRef}
            id={nameId}
            value={displayName}
            onChange={(event) => setDisplayName(event.target.value)}
            autoComplete="off"
            aria-invalid={Boolean(errors.displayName)}
            aria-describedby={errors.displayName ? nameErrorId : undefined}
          />
        </label>
        {errors.displayName ? (
          <p id={nameErrorId} className="field-error" role="alert">
            {errors.displayName}
          </p>
        ) : null}

        <label className="field" htmlFor={executableId}>
          <span>Executable</span>
          <input
            id={executableId}
            value={executable}
            onChange={(event) => setExecutable(event.target.value)}
            autoComplete="off"
            spellCheck={false}
            aria-invalid={Boolean(errors.executable)}
            aria-describedby={
              errors.executable ? executableErrorId : `${executableId}-help`
            }
          />
        </label>
        <p id={`${executableId}-help`} className="form__hint">
          Absolute path, ~/…, or a command name on PATH. Not `agent --flag`.
        </p>
        {errors.executable ? (
          <p id={executableErrorId} className="field-error" role="alert">
            {errors.executable}
          </p>
        ) : null}

        <fieldset className="fieldset">
          <legend>Arguments</legend>
          <p className="form__hint" id={argsErrorId}>
            One value per field. Spaces stay inside a single argument.
          </p>
          {args.map((argument, index) => (
            <div className="field-row" key={`arg-${index}`}>
              <label className="field field--grow" htmlFor={`${executableId}-arg-${index}`}>
                <span className="visually-hidden">Argument {index + 1}</span>
                <input
                  id={`${executableId}-arg-${index}`}
                  value={argument}
                  onChange={(event) => {
                    const next = [...args];
                    next[index] = event.target.value;
                    setArgs(next);
                  }}
                  autoComplete="off"
                  spellCheck={false}
                />
              </label>
              <button
                className="button button--secondary"
                type="button"
                onClick={() => {
                  const next = args.filter((_, item) => item !== index);
                  setArgs(next.length > 0 ? next : [""]);
                }}
                aria-label={`Remove argument ${index + 1}`}
              >
                Remove
              </button>
            </div>
          ))}
          <button
            className="button button--secondary"
            type="button"
            onClick={() => setArgs([...args, ""])}
          >
            Add argument
          </button>
          {errors.args ? (
            <p className="field-error" role="alert">
              {errors.args}
            </p>
          ) : null}
        </fieldset>

        <fieldset className="fieldset">
          <legend>Environment overrides</legend>
          <p className="form__hint">
            Values are masked. Existing secrets are not loaded into the form;
            leave a value blank to clear it on save.
          </p>
          {env.map((row, index) => (
            <div className="field-row" key={`env-${index}`}>
              <label className="field" htmlFor={`${executableId}-env-key-${index}`}>
                <span className="visually-hidden">Environment name {index + 1}</span>
                <input
                  id={`${executableId}-env-key-${index}`}
                  value={row.key}
                  onChange={(event) => {
                    const next = [...env];
                    next[index] = { ...row, key: event.target.value };
                    setEnv(next);
                  }}
                  placeholder="NAME"
                  autoComplete="off"
                  spellCheck={false}
                />
              </label>
              <label className="field field--grow" htmlFor={`${executableId}-env-value-${index}`}>
                <span className="visually-hidden">Environment value {index + 1}</span>
                <input
                  id={`${executableId}-env-value-${index}`}
                  type="password"
                  value={row.value}
                  onChange={(event) => {
                    const next = [...env];
                    next[index] = { ...row, value: event.target.value };
                    setEnv(next);
                  }}
                  placeholder={agent?.envKeys.includes(row.key) ? "••••••••" : ""}
                  autoComplete="new-password"
                />
              </label>
              <button
                className="button button--secondary"
                type="button"
                onClick={() => {
                  const next = env.filter((_, item) => item !== index);
                  setEnv(next.length > 0 ? next : [{ key: "", value: "" }]);
                }}
                aria-label={`Remove environment override ${index + 1}`}
              >
                Remove
              </button>
            </div>
          ))}
          <button
            className="button button--secondary"
            type="button"
            onClick={() => setEnv([...env, { key: "", value: "" }])}
          >
            Add variable
          </button>
          {errors.env ? (
            <p id={envErrorId} className="field-error" role="alert">
              {errors.env}
            </p>
          ) : null}
        </fieldset>

        <label className="field" htmlFor={cwdId}>
          <span>Default directory</span>
          <input
            id={cwdId}
            value={defaultCwd}
            onChange={(event) => setDefaultCwd(event.target.value)}
            autoComplete="off"
            spellCheck={false}
            aria-invalid={Boolean(errors.defaultCwd)}
            aria-describedby={errors.defaultCwd ? cwdErrorId : `${cwdId}-help`}
          />
        </label>
        <p id={`${cwdId}-help`} className="form__hint">
          Optional. Absolute path, ~/…, or ${"{PROJECT_PATH}"}.
        </p>
        {errors.defaultCwd ? (
          <p id={cwdErrorId} className="field-error" role="alert">
            {errors.defaultCwd}
          </p>
        ) : null}

        <label className="check" htmlFor={ptyId}>
          <input
            id={ptyId}
            type="checkbox"
            checked={requiresPty}
            onChange={(event) => setRequiresPty(event.target.checked)}
          />
          <span>Requires a PTY</span>
        </label>

        {submitError ? (
          <p id={formErrorId} className="field-error" role="alert">
            {submitError}
          </p>
        ) : null}

        <div className="dialog__actions">
          <button className="button button--secondary" type="button" onClick={onClose}>
            Cancel
          </button>
          <button className="button button--primary" type="submit" disabled={saving}>
            <Icon name="check" />
            <span>{agent ? "Save agent" : "Create agent"}</span>
          </button>
        </div>
      </form>
    </Dialog>
  );
}
