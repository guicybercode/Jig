import { useId, useRef, useState, type FormEvent } from "react";

import { ModalDialog } from "../../components/ModalDialog";

const DEFAULT_SESSION_NAME = "New session";
const DEFAULT_AGENT_ID = "fake";

/** Values submitted when the user requests a local agent session. */
export interface CreateSessionFormValues {
  readonly name: string;
  readonly agentId: string;
  readonly isolateWorktree: boolean;
}

interface CreateSessionDialogProps {
  readonly open: boolean;
  readonly projectName: string;
  readonly onCancel: () => void;
  readonly onCreate: (input: CreateSessionFormValues) => Promise<void>;
}

/** Collects the fields required to start a session without launching a vendor CLI. */
export function CreateSessionDialog({
  open,
  projectName,
  onCancel,
  onCreate,
}: CreateSessionDialogProps) {
  const [name, setName] = useState(DEFAULT_SESSION_NAME);
  const [agentId, setAgentId] = useState(DEFAULT_AGENT_ID);
  const [isolateWorktree, setIsolateWorktree] = useState(true);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const nameRef = useRef<HTMLInputElement>(null);
  const titleId = useId();
  const descriptionId = useId();
  const errorId = useId();

  if (!open) {
    return null;
  }

  function resetForm() {
    setName(DEFAULT_SESSION_NAME);
    setAgentId(DEFAULT_AGENT_ID);
    setIsolateWorktree(true);
    setSubmitError(null);
  }

  function cancel() {
    if (isSubmitting) {
      return;
    }

    resetForm();
    onCancel();
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (isSubmitting) {
      return;
    }

    const trimmedName = name.trim();
    if (trimmedName.length === 0) {
      setSubmitError("Enter a session name.");
      nameRef.current?.focus();
      return;
    }

    setIsSubmitting(true);
    setSubmitError(null);

    try {
      await onCreate({ name: trimmedName, agentId, isolateWorktree });
      resetForm();
    } catch {
      setSubmitError(
        "Could not create the session. Check the local daemon and try again.",
      );
    } finally {
      setIsSubmitting(false);
    }
  }

  return (
    <ModalDialog
      labelledBy={titleId}
      describedBy={`${descriptionId}${submitError ? ` ${errorId}` : ""}`}
      initialFocusRef={nameRef}
      onDismiss={cancel}
    >
      <form
        className="dialog__form"
        aria-labelledby={titleId}
        aria-busy={isSubmitting ? "true" : undefined}
        onSubmit={handleSubmit}
      >
        <h2 id={titleId}>Create session</h2>
        <p id={descriptionId}>
          Starts in {projectName} using a structured command, not a shell string.
        </p>
        <label className="dialog__field">
          Session name
          <input
            ref={nameRef}
            value={name}
            onChange={(event) => setName(event.target.value)}
            disabled={isSubmitting}
            required
          />
        </label>
        <label className="dialog__field">
          Agent
          <select
            value={agentId}
            onChange={(event) => setAgentId(event.target.value)}
            disabled={isSubmitting}
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
            disabled={isSubmitting}
          />
          Isolate in a Git worktree
        </label>
        {isSubmitting ? (
          <p className="dialog__status" role="status" aria-live="polite">
            Creating session…
          </p>
        ) : null}
        {submitError ? (
          <p id={errorId} className="dialog__error" role="alert">
            {submitError}
          </p>
        ) : null}
        <div className="dialog__actions">
          <button
            className="button button--secondary"
            type="button"
            disabled={isSubmitting}
            onClick={cancel}
          >
            Cancel
          </button>
          <button
            className="button button--primary"
            type="submit"
            disabled={isSubmitting}
          >
            {isSubmitting ? "Creating session…" : "Create session"}
          </button>
        </div>
      </form>
    </ModalDialog>
  );
}
