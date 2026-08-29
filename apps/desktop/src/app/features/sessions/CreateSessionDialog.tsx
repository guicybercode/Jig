import { useState } from "react";
import type { FormEvent } from "react";

interface CreateSessionDialogProps {
  open: boolean;
  projectName: string;
  onCancel: () => void;
  onCreate: (input: { name: string; agentId: string; isolateWorktree: boolean }) => void;
}

/** Collects the fields required to start a session without launching a vendor CLI. */
export function CreateSessionDialog({
  open,
  projectName,
  onCancel,
  onCreate,
}: CreateSessionDialogProps) {
  const [name, setName] = useState("New session");
  const [agentId, setAgentId] = useState("fake");
  const [isolateWorktree, setIsolateWorktree] = useState(true);

  if (!open) {
    return null;
  }

  function handleSubmit(event: FormEvent) {
    event.preventDefault();
    onCreate({ name, agentId, isolateWorktree });
  }

  return (
    <div className="dialog-backdrop">
      <form
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="create-session-title"
        onSubmit={handleSubmit}
      >
        <h2 id="create-session-title">Create session</h2>
        <p>Starts in {projectName} using a structured command, not a shell string.</p>
        <label className="dialog__field">
          Session name
          <input
            value={name}
            onChange={(event) => setName(event.target.value)}
            required
          />
        </label>
        <label className="dialog__field">
          Agent
          <select
            value={agentId}
            onChange={(event) => setAgentId(event.target.value)}
          >
            <option value="fake">Fake Agent</option>
            <option value="custom">Custom executable</option>
          </select>
        </label>
        <label className="dialog__check">
          <input
            type="checkbox"
            checked={isolateWorktree}
            onChange={(event) => setIsolateWorktree(event.target.checked)}
          />
          Isolate in a Git worktree
        </label>
        <div className="dialog__actions">
          <button className="button button--secondary" type="button" onClick={onCancel}>
            Cancel
          </button>
          <button className="button button--primary" type="submit">
            Create session
          </button>
        </div>
      </form>
    </div>
  );
}
