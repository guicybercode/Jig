import { useMemo, useState } from "react";

import { parseAgentId, formatApiError } from "../../../ipc";
import {
  useAgentAvailability,
  useAgents,
  usePending,
  useWorkspaceActions,
} from "../../workspace";

interface SessionCreateFormProps {
  readonly formId: string;
  readonly onSuccess?: () => void;
}

/** Shared new-session fields used by the workspace panel and the dialog. */
export function SessionCreateForm({
  formId,
  onSuccess,
}: SessionCreateFormProps) {
  const agents = useAgents();
  const availability = useAgentAvailability();
  const pending = usePending();
  const actions = useWorkspaceActions();
  const launchable = useMemo(
    () => agents.filter((agent) => agent.enabled),
    [agents],
  );
  const [name, setName] = useState("");
  const [agentId, setAgentId] = useState("");
  const [createWorktree, setCreateWorktree] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const selectedAgentId = launchable.some((agent) => agent.id === agentId)
    ? agentId
    : (launchable[0]?.id ?? "");

  return (
    <form
      id={formId}
      className="stack-form"
      onSubmit={(event) => {
        event.preventDefault();
        setError(null);
        const selected = selectedAgentId.trim();
        if (selected.length === 0) {
          setError("Select an agent.");
          return;
        }
        void actions
          .createSession({
            agentId: parseAgentId(selected),
            name: name.trim() || "Session",
            isolation: createWorktree ? "new_worktree" : "current",
          })
          .then(() => {
            setName("");
            onSuccess?.();
          })
          .catch((caught: unknown) => setError(formatApiError(caught)));
      }}
    >
      <label htmlFor={`${formId}-name`}>Session name</label>
      <input
        id={`${formId}-name`}
        value={name}
        onChange={(event) => setName(event.target.value)}
        autoComplete="off"
      />
      <label htmlFor={`${formId}-agent`}>Agent</label>
      <select
        id={`${formId}-agent`}
        value={selectedAgentId}
        onChange={(event) => setAgentId(event.target.value)}
      >
        {launchable.map((agent) => (
          <option key={agent.id} value={agent.id}>
            {agent.displayName}
            {availability.get(agent.id) === false ? " (not found)" : ""}
          </option>
        ))}
      </select>
      <label className="checkbox">
        <input
          type="checkbox"
          checked={createWorktree}
          onChange={(event) => setCreateWorktree(event.target.checked)}
        />
        Create a new Git worktree
      </label>
      {error ? (
        <p className="form-error" role="alert">
          {error}
        </p>
      ) : null}
      <button
        className="button button--primary"
        type="submit"
        disabled={pending.creatingSession || launchable.length === 0}
      >
        Start session
      </button>
    </form>
  );
}
