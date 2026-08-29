import { Dialog } from "../../components/Dialog";
import { useDialogs, useWorkspaceActions } from "../../workspace";
import { SessionCreateForm } from "./SessionCreateForm";

/** Accessible new-session overlay opened from the header. */
export function NewSessionDialog() {
  const dialogs = useDialogs();
  const actions = useWorkspaceActions();

  return (
    <Dialog
      title="New session"
      open={dialogs.newSession}
      onClose={() => actions.closeDialog("newSession")}
    >
      <SessionCreateForm
        formId="new-session-dialog-form"
        onSuccess={() => actions.closeDialog("newSession")}
      />
      <div className="dialog__actions">
        <button
          className="button button--secondary"
          type="button"
          onClick={() => actions.closeDialog("newSession")}
        >
          Cancel
        </button>
      </div>
    </Dialog>
  );
}
