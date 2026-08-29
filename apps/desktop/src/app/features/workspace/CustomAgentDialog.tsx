import { useState } from "react";

import { Dialog } from "../../components/Dialog";
import { formatApiError } from "../../../ipc";
import {
  useDialogs,
  usePending,
  useWorkspaceActions,
} from "../../workspace";

/** Registers a custom agent through the official `agent.custom.create` payload. */
export function CustomAgentDialog() {
  const dialogs = useDialogs();
  const actions = useWorkspaceActions();
  const pending = usePending();
  const [displayName, setDisplayName] = useState("Custom agent");
  const [executable, setExecutable] = useState("/bin/cat");
  const [args, setArgs] = useState("");
  const [error, setError] = useState<string | null>(null);

  return (
    <Dialog
      title="Custom agent"
      open={dialogs.customAgent}
      onClose={() => actions.closeDialog("customAgent")}
    >
      <form
        className="stack-form"
        onSubmit={(event) => {
          event.preventDefault();
          setError(null);
          const parsedArgs = args
            .split(/\s+/)
            .map((part) => part.trim())
            .filter(Boolean);
          void actions
            .createCustomAgent({
              displayName: displayName.trim(),
              executable: executable.trim(),
              args: parsedArgs,
            })
            .catch((caught: unknown) => setError(formatApiError(caught)));
        }}
      >
        <label htmlFor="custom-agent-name">Display name</label>
        <input
          id="custom-agent-name"
          value={displayName}
          onChange={(event) => setDisplayName(event.target.value)}
        />
        <label htmlFor="custom-agent-executable">Executable</label>
        <input
          id="custom-agent-executable"
          value={executable}
          onChange={(event) => setExecutable(event.target.value)}
        />
        <label htmlFor="custom-agent-args">Arguments</label>
        <input
          id="custom-agent-args"
          value={args}
          onChange={(event) => setArgs(event.target.value)}
          placeholder="one token per word"
        />
        {error ? (
          <p className="form-error" role="alert">
            {error}
          </p>
        ) : null}
        <div className="dialog__actions">
          <button
            className="button button--secondary"
            type="button"
            onClick={() => actions.closeDialog("customAgent")}
          >
            Cancel
          </button>
          <button
            className="button button--primary"
            type="submit"
            disabled={pending.creatingAgent}
          >
            Register custom agent
          </button>
        </div>
      </form>
    </Dialog>
  );
}
