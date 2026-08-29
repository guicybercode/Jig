import { useId, useRef, type FormEvent } from "react";

import { Dialog } from "../../components/Dialog";
import type { AgentRecord } from "../../../ipc/agentTypes";
import { statusSummary } from "./agentStatus";

interface CreateSessionDialogProps {
  readonly agents: AgentRecord[];
  readonly hasProject: boolean;
  readonly initialAgentId?: string;
  readonly onClose: () => void;
  readonly onCreate: (draft: { name: string; agentId: string }) => void;
}

/** Collects a session name and a UUIDv7 agent id without constructing a shell command. */
export function CreateSessionDialog({
  agents,
  hasProject,
  initialAgentId,
  onClose,
  onCreate,
}: CreateSessionDialogProps) {
  const nameId = useId();
  const agentId = useId();
  const hintId = useId();
  const errorId = useId();
  const nameRef = useRef<HTMLInputElement>(null);
  const selectable = agents.filter((agent) => agent.enabled);
  const defaultAgent =
    selectable.find((agent) => agent.id === initialAgentId) ??
    selectable.find((agent) => agent.installed) ??
    selectable[0];

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!hasProject) {
      return;
    }
    const form = new FormData(event.currentTarget);
    const name = String(form.get("session-name") ?? "").trim();
    const selectedId = String(form.get("agent-id") ?? "");
    if (!name) {
      nameRef.current?.focus();
      return;
    }
    onCreate({ name, agentId: selectedId });
  }

  return (
    <Dialog title="New session" open onClose={onClose} initialFocusRef={nameRef}>
      <form className="form" onSubmit={handleSubmit}>
        <p id={hintId} className="form__hint">
          Choose an agent by its saved identity. Adapter keys such as codex are
          not session identifiers.
        </p>
        <label className="field" htmlFor={nameId}>
          <span>Session name</span>
          <input
            ref={nameRef}
            id={nameId}
            name="session-name"
            required
            autoComplete="off"
            aria-describedby={hintId}
          />
        </label>
        <label className="field" htmlFor={agentId}>
          <span>Agent</span>
          <select
            id={agentId}
            name="agent-id"
            required
            defaultValue={defaultAgent?.id}
            disabled={selectable.length === 0}
            aria-describedby={errorId}
          >
            {selectable.map((agent) => (
              <option key={agent.id} value={agent.id}>
                {agent.displayName} ({statusSummary(agent)})
              </option>
            ))}
          </select>
        </label>
        <p id={errorId} className="form__hint">
          {hasProject
            ? selectable.length === 0
              ? "Enable an agent before creating a session."
              : "Missing agents can be selected, but the session will not start until the CLI is installed."
            : "Add a project first. The session is not started from this dialog until a repository is registered."}
        </p>
        <div className="dialog__actions">
          <button className="button button--secondary" type="button" onClick={onClose}>
            Cancel
          </button>
          <button
            className="button button--primary"
            type="submit"
            disabled={!hasProject || selectable.length === 0}
            aria-describedby={!hasProject ? errorId : undefined}
          >
            Create session
          </button>
        </div>
      </form>
    </Dialog>
  );
}
