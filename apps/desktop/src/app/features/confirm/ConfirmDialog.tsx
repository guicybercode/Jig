import { useId, useRef } from "react";

import { ModalDialog } from "../../components/ModalDialog";

interface ConfirmDialogProps {
  readonly open: boolean;
  readonly title: string;
  readonly message: string;
  readonly confirmLabel: string;
  readonly onCancel: () => void;
  readonly onConfirm: () => void;
}

/** Requires an explicit confirm for destructive Git or session actions. */
export function ConfirmDialog({
  open,
  title,
  message,
  confirmLabel,
  onCancel,
  onConfirm,
}: ConfirmDialogProps) {
  const cancelRef = useRef<HTMLButtonElement>(null);
  const titleId = useId();
  const descriptionId = useId();

  if (!open) {
    return null;
  }

  return (
    <ModalDialog
      labelledBy={titleId}
      describedBy={descriptionId}
      initialFocusRef={cancelRef}
      onDismiss={onCancel}
    >
      <h2 id={titleId}>{title}</h2>
      <p id={descriptionId}>{message}</p>
      <div className="dialog__actions">
        <button
          ref={cancelRef}
          className="button button--secondary"
          type="button"
          onClick={onCancel}
        >
          Cancel
        </button>
        <button
          className="button button--danger"
          type="button"
          onClick={onConfirm}
        >
          {confirmLabel}
        </button>
      </div>
    </ModalDialog>
  );
}
