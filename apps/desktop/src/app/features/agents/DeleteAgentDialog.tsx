import { Dialog } from "../../components/Dialog";
import type { AgentRecord } from "../../../ipc/agentTypes";

interface DeleteAgentDialogProps {
  readonly agent: AgentRecord;
  readonly onClose: () => void;
  readonly onConfirm: () => Promise<void>;
}

/** Confirms custom agent removal without implying files are deleted. */
export function DeleteAgentDialog({
  agent,
  onClose,
  onConfirm,
}: DeleteAgentDialogProps) {
  return (
    <Dialog
      title="Remove custom agent"
      open
      onClose={onClose}
      describedBy="delete-agent-copy"
    >
      <p id="delete-agent-copy" className="form__hint">
        This removes the “{agent.displayName}” definition from CLI Master. The
        executable on disk is not deleted.
      </p>
      <div className="dialog__actions">
        <button className="button button--secondary" type="button" onClick={onClose}>
          Cancel
        </button>
        <button
          className="button button--danger"
          type="button"
          onClick={() => {
            void onConfirm();
          }}
        >
          Remove agent
        </button>
      </div>
    </Dialog>
  );
}
