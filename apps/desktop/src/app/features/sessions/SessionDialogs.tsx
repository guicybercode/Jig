import { useEffect, useId, useRef, useState, type FormEvent } from "react";

import type {
  ApiErrorData,
  GitStatus,
  GitTarget,
  Session,
  Worktree,
  WorktreeRemovalPreparation,
  WorktreeRemovalBlocker,
} from "../../../ipc/types";
import { Dialog } from "../../components/Dialog";
import { Icon } from "../../components/Icon";
import { errorData } from "../../utils";
import { InlineRequestError } from "../projects/ProjectDialogs";

interface RenameSessionDialogProps {
  readonly open: boolean;
  readonly session: Session;
  readonly onClose: () => void;
  readonly onRename: (sessionId: string, name: string) => Promise<Session>;
}

export function RenameSessionDialog({ open, session, onClose, onRename }: RenameSessionDialogProps) {
  const inputId = useId();
  const inputRef = useRef<HTMLInputElement>(null);
  const lock = useRef(false);
  const [name, setName] = useState(session.name);
  const [error, setError] = useState<ApiErrorData>();
  const [submitting, setSubmitting] = useState(false);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (lock.current || !name.trim()) {
      inputRef.current?.focus();
      return;
    }
    lock.current = true;
    setSubmitting(true);
    setError(undefined);
    try {
      await onRename(session.id, name.trim());
      onClose();
    } catch (failure) {
      setError(errorData(failure));
    } finally {
      lock.current = false;
      setSubmitting(false);
    }
  }

  return (
    <Dialog open={open} title="Rename Session" description="The process and working directory do not change." onClose={onClose} closeDisabled={submitting} initialFocusRef={inputRef} footer={<><button className="button button--secondary" type="button" disabled={submitting} onClick={onClose}>Cancel</button><button className="button button--primary" type="submit" form="rename-session-form" disabled={submitting || !name.trim()}>{submitting ? "Saving…" : "Save Name"}</button></>}>
      <form id="rename-session-form" className="form-stack" onSubmit={(event) => void submit(event)}>
        {error ? <InlineRequestError error={error} /> : null}
        <div className="field"><label htmlFor={inputId}>Session name</label><input ref={inputRef} id={inputId} className="text-input" value={name} required maxLength={120} onChange={(event) => setName(event.target.value)} /></div>
      </form>
    </Dialog>
  );
}

interface StopSessionDialogProps {
  readonly open: boolean;
  readonly session: Session;
  readonly onClose: () => void;
  readonly onStop: (sessionId: string) => Promise<Session>;
}

export function StopSessionDialog({ open, session, onClose, onStop }: StopSessionDialogProps) {
  const cancelRef = useRef<HTMLButtonElement>(null);
  const lock = useRef(false);
  const [error, setError] = useState<ApiErrorData>();
  const [submitting, setSubmitting] = useState(false);
  async function stop() {
    if (lock.current) return;
    lock.current = true;
    setSubmitting(true);
    setError(undefined);
    try {
      await onStop(session.id);
      onClose();
    } catch (failure) {
      setError(errorData(failure));
    } finally {
      lock.current = false;
      setSubmitting(false);
    }
  }
  return (
    <Dialog open={open} title="Stop Process" description={`Stop the agent process for ${session.name}?`} onClose={onClose} closeDisabled={submitting} initialFocusRef={cancelRef} footer={<><button ref={cancelRef} className="button button--secondary" type="button" disabled={submitting} onClick={onClose}>Keep Running</button><button className="button button--danger" type="button" disabled={submitting} aria-busy={submitting} onClick={() => void stop()}>{submitting ? "Stopping…" : "Stop Process"}</button></>}>
      {error ? <InlineRequestError error={error} /> : null}
      <div className="destructive-copy"><Icon name="stop" /><div><strong>Only the live process stops.</strong><p>Session metadata and any Git worktree remain available. You can restart this session later.</p></div></div>
    </Dialog>
  );
}

interface DeleteSessionDialogProps {
  readonly open: boolean;
  readonly session: Session;
  readonly worktree?: Worktree;
  readonly onClose: () => void;
  readonly onDelete: (sessionId: string) => Promise<void>;
}

export function DeleteSessionDialog({ open, session, worktree, onClose, onDelete }: DeleteSessionDialogProps) {
  const cancelRef = useRef<HTMLButtonElement>(null);
  const lock = useRef(false);
  const [error, setError] = useState<ApiErrorData>();
  const [submitting, setSubmitting] = useState(false);
  async function remove() {
    if (lock.current) return;
    lock.current = true;
    setSubmitting(true);
    setError(undefined);
    try {
      await onDelete(session.id);
      onClose();
    } catch (failure) {
      setError(errorData(failure));
    } finally {
      lock.current = false;
      setSubmitting(false);
    }
  }
  return (
    <Dialog open={open} title="Delete Session" description={`Delete metadata for ${session.name}?`} onClose={onClose} closeDisabled={submitting} initialFocusRef={cancelRef} footer={<><button ref={cancelRef} className="button button--secondary" type="button" disabled={submitting} onClick={onClose}>Keep Session</button><button className="button button--danger" type="button" disabled={submitting} aria-busy={submitting} onClick={() => void remove()}>{submitting ? "Deleting…" : "Delete Session"}</button></>}>
      {error ? <InlineRequestError error={error} /> : null}
      <div className="destructive-copy"><Icon name="trash" /><div><strong>This deletes session metadata only.</strong><p>It does not stop a running process, delete project files, or remove an associated worktree. A running session must be stopped first.</p></div></div>
      {worktree ? <p className="information-box"><Icon name="worktree" /> Worktree retained at <span className="mono">{worktree.path}</span></p> : null}
    </Dialog>
  );
}

interface RemoveWorktreeDialogProps {
  readonly open: boolean;
  readonly worktree: Worktree;
  readonly onClose: () => void;
  readonly onPrepare: (worktreeId: string) => Promise<WorktreeRemovalPreparation>;
  readonly onRemove: (preparation: WorktreeRemovalPreparation) => Promise<void>;
}

export function RemoveWorktreeDialog({ open, worktree, onClose, onPrepare, onRemove }: RemoveWorktreeDialogProps) {
  const cancelRef = useRef<HTMLButtonElement>(null);
  const lock = useRef(false);
  const [preparation, setPreparation] = useState<WorktreeRemovalPreparation>();
  const [error, setError] = useState<ApiErrorData>();
  const [loading, setLoading] = useState(true);
  const [submitting, setSubmitting] = useState(false);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let current = true;
    setLoading(true);
    setError(undefined);
    void onPrepare(worktree.id)
      .then((result) => { if (current) setPreparation(result); })
      .catch((failure: unknown) => { if (current) setError(errorData(failure)); })
      .finally(() => { if (current) setLoading(false); });
    return () => { current = false; };
  }, [attempt, onPrepare, worktree.id]);

  async function remove() {
    if (!preparation || lock.current) return;
    lock.current = true;
    setSubmitting(true);
    setError(undefined);
    try {
      await onRemove(preparation);
      onClose();
    } catch (failure) {
      setError(errorData(failure));
    } finally {
      lock.current = false;
      setSubmitting(false);
    }
  }
  const blocked = !preparation || preparation.status !== "ready";
  return (
    <Dialog open={open} title="Remove Worktree" description="This is separate from stopping or deleting a session." onClose={onClose} closeDisabled={submitting} initialFocusRef={cancelRef} footer={<><button ref={cancelRef} className="button button--secondary" type="button" disabled={submitting} onClick={onClose}>Keep Worktree</button><button className="button button--danger" type="button" disabled={loading || submitting || blocked} aria-busy={submitting} onClick={() => void remove()}>{submitting ? "Removing…" : "Remove Worktree"}</button></>}>
      {loading ? <div className="loading-state" role="status">Checking worktree safety…</div> : null}
      {error ? <><InlineRequestError error={error} /><button className="button button--secondary" type="button" onClick={() => setAttempt((value) => value + 1)}>Check Again</button></> : null}
      {preparation ? <div className="form-stack"><p className="path-context mono">{worktree.path}</p>{preparation.status === "blocked" ? <><div className="notice notice--error" role="alert"><Icon name="warning" /><div><strong>Removal is blocked</strong><ul>{preparation.blockers.map((blocker) => <li key={blocker}>{worktreeBlockerLabel(blocker)}</li>)}</ul></div></div>{preparation.isDirty ? <div className="notice notice--error" role="alert"><Icon name="warning" /><div><strong>This worktree has content that Git would remove.</strong><p>Commit, stash, or clean it outside Jig, then check again. The Beta v1 contract has no unsafe-removal bypass.</p></div></div> : null}</> : <div className="information-box"><Icon name="check" /><p>The safety check found no uncommitted content or active process use.</p></div>}<p className="document-note">Removing the worktree does not delete its Git branch. The daemon rechecks state before removal.</p></div> : null}
    </Dialog>
  );
}

interface GitStatusDialogProps {
  readonly open: boolean;
  readonly session: Session;
  readonly onClose: () => void;
  readonly onLoad: (target: GitTarget) => Promise<GitStatus>;
}

export function GitStatusDialog({ open, session, onClose, onLoad }: GitStatusDialogProps) {
  const [status, setStatus] = useState<GitStatus>();
  const [error, setError] = useState<ApiErrorData>();
  const [loading, setLoading] = useState(true);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let current = true;
    setLoading(true);
    setError(undefined);
    void onLoad({ kind: "session", sessionId: session.id })
      .then((result) => { if (current) setStatus(result); })
      .catch((failure: unknown) => { if (current) setError(errorData(failure)); })
      .finally(() => { if (current) setLoading(false); });
    return () => { current = false; };
  }, [attempt, onLoad, session.cwd, session.worktreePath]);
  return (
    <Dialog open={open} title="Git Status" description={session.name} size="large" onClose={onClose} footer={<button className="button button--secondary" type="button" onClick={onClose}>Close</button>}>
      {loading ? <div className="loading-state" role="status">Reading Git status…</div> : null}
      {error ? <><InlineRequestError error={error} /><button className="button button--secondary" type="button" onClick={() => setAttempt((value) => value + 1)}>Retry</button></> : null}
      {status ? <div className="git-status"><div className="git-status__summary"><span><Icon name="branch" /> {status.branch ?? "Detached HEAD"}</span><strong>{status.isDirty ? "Worktree dirty" : "Worktree clean"}</strong></div>{status.files.length === 0 ? <p className="git-status__empty">No staged, tracked, or untracked changes.</p> : <ul className="git-file-list">{status.files.map((file) => <li key={`${file.kind}:${file.path}`}><span className={`git-file-kind git-file-kind--${file.kind}`}>{file.kind}</span><span className="mono">{file.path}</span><span>{file.staged ? "staged" : file.unstaged ? "unstaged" : "untracked"}</span></li>)}</ul>}</div> : null}
    </Dialog>
  );
}

function worktreeBlockerLabel(blocker: WorktreeRemovalBlocker): string {
  switch (blocker) {
    case "running":
      return "A live session is still using this worktree.";
    case "staged_changes":
      return "Staged changes are present.";
    case "tracked_changes":
      return "Unstaged tracked changes are present.";
    case "untracked_files":
      return "Untracked files are present.";
    case "ignored_files":
      return "Ignored files are present and would be deleted.";
    case "assume_unchanged":
      return "An index entry is marked assume-unchanged.";
    case "skip_worktree":
      return "An index entry is marked skip-worktree.";
    case "locked":
      return "Git has locked this worktree.";
    case "in_use":
      return "Another session or operation is using this worktree.";
    case "unknown":
      return "The daemon reported an unrecognized safety blocker.";
  }
}
