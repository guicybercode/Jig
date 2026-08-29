import { useId, useState } from "react";

import { Icon } from "../../components/Icon";

export interface WorktreeRemovalPreview {
  readonly path: string;
  readonly branch: string;
  readonly dirty: boolean;
  readonly inUse: boolean;
}

interface WorktreeRemoveDialogProps {
  readonly preview: WorktreeRemovalPreview;
  readonly onCancel: () => void;
  readonly onConfirm: (allowDirty: boolean) => void;
}

/** Confirms worktree removal with path, branch, and dirty-state guards. */
export function WorktreeRemoveDialog({
  preview,
  onCancel,
  onConfirm,
}: WorktreeRemoveDialogProps) {
  const titleId = useId();
  const previewKey = JSON.stringify([
    preview.path,
    preview.branch,
    preview.dirty,
    preview.inUse,
  ]);
  const [dirtyApproval, setDirtyApproval] = useState({
    previewKey,
    allowed: false,
  });
  const allowDirty =
    dirtyApproval.previewKey === previewKey && dirtyApproval.allowed;
  const blocked = preview.inUse || (preview.dirty && !allowDirty);

  return (
    <div className="dialog-backdrop">
      <div
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
      >
        <header className="dialog__header">
          <div>
            <p className="dialog__eyebrow">Git worktree</p>
            <h2 id={titleId}>Remove worktree</h2>
          </div>
        </header>
        <p className="dialog__lede">
          This deletes the worktree directory after Git confirms it is safe.
          The main repository is never removed.
        </p>
        <dl className="diagnostics-grid">
          <div className="diagnostics-item">
            <dt>Path</dt>
            <dd>{preview.path}</dd>
          </div>
          <div className="diagnostics-item">
            <dt>Branch</dt>
            <dd>{preview.branch}</dd>
          </div>
        </dl>
        {preview.inUse ? (
          <p className="dialog__error" role="alert">
            A session is still using this worktree. Stop the session first.
            Force removal is not available.
          </p>
        ) : null}
        {preview.dirty ? (
          <label className="dialog__check">
            <input
              type="checkbox"
              checked={allowDirty}
              onChange={(event) => {
                setDirtyApproval({
                  previewKey,
                  allowed: event.target.checked,
                });
              }}
            />
            <span>
              I understand this worktree has uncommitted changes and want to
              remove it anyway.
            </span>
          </label>
        ) : (
          <p className="dialog__lede">
            Git currently reports a clean worktree. Removal still requires this
            confirmation.
          </p>
        )}
        <footer className="dialog__footer">
          <button
            className="button button--secondary"
            type="button"
            onClick={onCancel}
          >
            Cancel
          </button>
          <button
            className="button button--danger"
            type="button"
            disabled={blocked}
            onClick={() => {
              onConfirm(allowDirty);
            }}
          >
            <Icon name="folder" />
            <span>Remove worktree</span>
          </button>
        </footer>
      </div>
    </div>
  );
}
