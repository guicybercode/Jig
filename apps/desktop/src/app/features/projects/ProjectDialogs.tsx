import { useId, useRef, useState, type FormEvent } from "react";

import type { AddProjectInput, ApiErrorData, Project } from "../../../ipc/types";
import { Dialog } from "../../components/Dialog";
import { Icon } from "../../components/Icon";
import { errorData, isAbsoluteLocalPath } from "../../utils";
import { browseForProjectDirectory } from "./project-directory-picker";

interface AddProjectDialogProps {
  readonly open: boolean;
  readonly onClose: () => void;
  readonly onAdd: (input: AddProjectInput) => Promise<Project>;
  readonly onBrowseDirectory?: () => Promise<string | null>;
}

/** Registers a daemon-validated repository selected visually or entered by path. */
export function AddProjectDialog({
  open,
  onClose,
  onAdd,
  onBrowseDirectory = browseForProjectDirectory,
}: AddProjectDialogProps) {
  const pathId = useId();
  const nameId = useId();
  const pathRef = useRef<HTMLInputElement>(null);
  const browseRef = useRef<HTMLButtonElement>(null);
  const inFlight = useRef(false);
  const [path, setPath] = useState("");
  const [name, setName] = useState("");
  const [pathError, setPathError] = useState<string>();
  const [requestError, setRequestError] = useState<ApiErrorData>();
  const [browseError, setBrowseError] = useState<string>();
  const [browsing, setBrowsing] = useState(false);
  const [manualEntryOpen, setManualEntryOpen] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const selectedDirectory = describeDirectory(path);

  function validatePath(): boolean {
    const trimmedPath = path.trim();
    const nextError = !trimmedPath
      ? "Enter a project directory."
      : !isAbsoluteLocalPath(trimmedPath)
        ? "Use an absolute Linux or macOS path beginning with /."
        : undefined;
    setPathError(nextError);
    return nextError === undefined;
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (inFlight.current || !validatePath()) {
      pathRef.current?.focus();
      return;
    }
    inFlight.current = true;
    setSubmitting(true);
    setRequestError(undefined);
    try {
      await onAdd({
        path: path.trim(),
        name: name.trim() || undefined,
      });
      onClose();
    } catch (error) {
      setRequestError(errorData(error));
    } finally {
      inFlight.current = false;
      setSubmitting(false);
    }
  }

  async function handleBrowse() {
    setBrowsing(true);
    setBrowseError(undefined);
    try {
      const selection = await onBrowseDirectory();
      if (selection) {
        setPath(selection);
        setPathError(undefined);
        setRequestError(undefined);
      }
    } catch {
      setBrowseError(
        "The folder browser could not be opened. Enter an absolute path instead.",
      );
      setManualEntryOpen(true);
    } finally {
      setBrowsing(false);
    }
  }

  return (
    <Dialog
      open={open}
      title="Add Project"
      description="Register a local project folder with Jig."
      size="medium"
      closeDisabled={submitting}
      initialFocusRef={browseRef}
      onClose={onClose}
      footer={
        <>
          <button className="button button--secondary" type="button" disabled={submitting} onClick={onClose}>Cancel</button>
          <button className="button button--primary" type="submit" form="add-project-form" disabled={submitting} aria-busy={submitting}>
            {submitting ? "Validating…" : "Add Project"}
          </button>
        </>
      }
    >
      <form id="add-project-form" className="form-stack" noValidate onSubmit={(event) => void handleSubmit(event)}>
        {requestError ? <InlineRequestError error={requestError} /> : null}
        <section className="project-directory-picker" aria-labelledby={`${pathId}-title`}>
          <div className="project-directory-picker__heading">
            <span id={`${pathId}-title`}>Project folder <span aria-hidden="true">*</span></span>
            <small>Local project folder</small>
          </div>

          {selectedDirectory ? (
            <div className="project-directory-picker__selection">
              <span className="project-directory-picker__icon" aria-hidden="true"><Icon name="folder" /></span>
              <span className="project-directory-picker__path">
                <strong>{selectedDirectory.name}</strong>
                <small className="mono">{selectedDirectory.parent}</small>
              </span>
              <button className="button button--secondary" type="button" disabled={browsing || submitting} onClick={() => void handleBrowse()}>
                {browsing ? "Opening…" : "Change"}
              </button>
            </div>
          ) : (
            <button
              ref={browseRef}
              className="project-directory-picker__browse"
              type="button"
              disabled={browsing || submitting}
              onClick={() => void handleBrowse()}
            >
              <span className="project-directory-picker__icon" aria-hidden="true"><Icon name="folder" /></span>
              <span><strong>{browsing ? "Opening folder browser…" : "Choose a project folder"}</strong><small>Browse folders on this Mac</small></span>
              <Icon name="chevronRight" />
            </button>
          )}

          {browseError ? <p className="field__error" role="alert">{browseError}</p> : null}
          {pathError && !manualEntryOpen ? <p className="field__error" role="alert">{pathError}</p> : null}

          <button
            className="project-directory-picker__manual-toggle"
            type="button"
            aria-expanded={manualEntryOpen}
            aria-controls={`${pathId}-manual`}
            onClick={() => setManualEntryOpen((visible) => !visible)}
          >
            <Icon name="chevronDown" />
            {manualEntryOpen ? "Hide manual path" : "Enter a path manually"}
          </button>

          {manualEntryOpen ? (
            <div id={`${pathId}-manual`} className="field project-directory-picker__manual">
              <label htmlFor={pathId}>Directory <span aria-hidden="true">*</span></label>
              <input
                ref={pathRef}
                id={pathId}
                className="text-input mono"
                value={path}
                required
                aria-invalid={pathError ? true : undefined}
                aria-describedby={`${pathId}-hint${pathError ? ` ${pathId}-error` : ""}`}
                placeholder="/Users/you/code/project"
                onChange={(event) => {
                  setPath(event.target.value);
                  if (pathError) setPathError(undefined);
                  if (browseError) setBrowseError(undefined);
                }}
                onBlur={validatePath}
              />
              <p id={`${pathId}-hint`} className="field__hint">Paste an absolute local path.</p>
              {pathError ? <p id={`${pathId}-error`} className="field__error" role="alert">{pathError}</p> : null}
            </div>
          ) : null}
        </section>
        <div className="field">
          <label htmlFor={nameId}>Display name <span className="field__optional">Optional</span></label>
          <input id={nameId} className="text-input" value={name} maxLength={120} placeholder="Defaults to the folder name" onChange={(event) => setName(event.target.value)} />
          <p className="field__hint">This changes only how the project appears in Jig.</p>
        </div>
        <div className="information-box">
          <Icon name="repository" />
          <p>The folder is only registered in Jig. Its files are never changed or deleted.</p>
        </div>
      </form>
    </Dialog>
  );
}

function describeDirectory(path: string): { readonly name: string; readonly parent: string } | null {
  const normalizedPath = path.trim().replace(/\/+$/, "");
  if (!normalizedPath) return null;
  const separatorIndex = normalizedPath.lastIndexOf("/");
  if (separatorIndex < 0) return { name: normalizedPath, parent: "Manual path" };
  return {
    name: normalizedPath.slice(separatorIndex + 1) || "/",
    parent: normalizedPath.slice(0, separatorIndex) || "/",
  };
}

interface RenameProjectDialogProps {
  readonly open: boolean;
  readonly project: Project;
  readonly onClose: () => void;
  readonly onRename: (projectId: string, name: string) => Promise<Project>;
}

export function RenameProjectDialog({ open, project, onClose, onRename }: RenameProjectDialogProps) {
  const inputId = useId();
  const inputRef = useRef<HTMLInputElement>(null);
  const inFlight = useRef(false);
  const [name, setName] = useState(project.name);
  const [error, setError] = useState<ApiErrorData>();
  const [submitting, setSubmitting] = useState(false);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (inFlight.current) return;
    if (!name.trim()) {
      inputRef.current?.focus();
      return;
    }
    inFlight.current = true;
    setSubmitting(true);
    setError(undefined);
    try {
      await onRename(project.id, name.trim());
      onClose();
    } catch (failure) {
      setError(errorData(failure));
    } finally {
      inFlight.current = false;
      setSubmitting(false);
    }
  }

  return (
    <Dialog open={open} title="Rename Project" description="Only the display name changes." onClose={onClose} closeDisabled={submitting} initialFocusRef={inputRef} footer={<><button className="button button--secondary" type="button" onClick={onClose} disabled={submitting}>Cancel</button><button className="button button--primary" type="submit" form="rename-project-form" disabled={submitting || !name.trim()}>{submitting ? "Saving…" : "Save Name"}</button></>}>
      <form id="rename-project-form" className="form-stack" onSubmit={(event) => void handleSubmit(event)}>
        {error ? <InlineRequestError error={error} /> : null}
        <div className="field"><label htmlFor={inputId}>Display name</label><input ref={inputRef} id={inputId} className="text-input" value={name} maxLength={120} required onChange={(event) => setName(event.target.value)} /></div>
        <p className="path-context">Repository: <span className="mono">{project.repositoryRoot ?? project.path}</span></p>
      </form>
    </Dialog>
  );
}

interface RemoveProjectDialogProps {
  readonly open: boolean;
  readonly project: Project;
  readonly onClose: () => void;
  readonly onRemove: (projectId: string) => Promise<void>;
}

export function RemoveProjectDialog({ open, project, onClose, onRemove }: RemoveProjectDialogProps) {
  const inFlight = useRef(false);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const [error, setError] = useState<ApiErrorData>();
  const [submitting, setSubmitting] = useState(false);

  async function handleRemove() {
    if (inFlight.current) return;
    inFlight.current = true;
    setSubmitting(true);
    setError(undefined);
    try {
      await onRemove(project.id);
      onClose();
    } catch (failure) {
      setError(errorData(failure));
    } finally {
      inFlight.current = false;
      setSubmitting(false);
    }
  }

  return (
    <Dialog open={open} title="Remove Project" description={`Remove ${project.name} from Jig?`} onClose={onClose} closeDisabled={submitting} initialFocusRef={cancelRef} footer={<><button ref={cancelRef} className="button button--secondary" type="button" disabled={submitting} onClick={onClose}>Keep Project</button><button className="button button--danger" type="button" disabled={submitting} aria-busy={submitting} onClick={() => void handleRemove()}>{submitting ? "Removing…" : "Remove from App"}</button></>}>
      {error ? <InlineRequestError error={error} /> : null}
      <div className="destructive-copy"><Icon name="warning" /><div><strong>Files stay on disk.</strong><p>This removes only Jig’s project registration. It does not delete the repository, branches, or files. Existing session or worktree references may prevent removal.</p></div></div>
      <p className="path-context mono">{project.repositoryRoot ?? project.path}</p>
    </Dialog>
  );
}

export function InlineRequestError({ error }: { readonly error: ApiErrorData }) {
  return (
    <div className="notice notice--error" role="alert">
      <Icon name="warning" />
      <div><strong>{error.message}</strong>{error.action ? <p>{error.action}</p> : null}</div>
    </div>
  );
}
