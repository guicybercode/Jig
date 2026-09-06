import { useId, type FormEvent } from "react";

import type { OrganizationTarget, OrganizationWorkflow } from "../../../ipc/domain";
import { isOrganizationWorkflow, organizationTargetKey } from "../../../ipc/organization-schema";
import type { SessionStatus } from "../../../ipc/types";
import { StatusBadge, getSessionStatusLabel } from "../../components/StatusBadge";
import { isLiveStatus } from "../../utils";
import type { OrganizationDraft, OrganizationPanelProps } from "./organization-types";
import { hasOrganizationChanges, useOrganization } from "./useOrganization";
import "./organization.css";

/** Explicit organization controls; the host retains canvas selection and runtime ownership. */
export function OrganizationPanel(props: OrganizationPanelProps) {
  const organization = useOrganization(props);
  const id = useId();
  const selected: { target: OrganizationTarget; name: string; status?: SessionStatus }[] = [];
  if (props.currentProject) selected.push({ target: { kind: "project", id: props.currentProject.id }, name: props.currentProject.name });
  if (props.currentSession) selected.push({ target: { kind: "session", id: props.currentSession.id }, name: props.currentSession.name, status: props.currentSession.status });

  return (
    <section className="organization-panel" aria-labelledby={`${id}-heading`}>
      <header className="organization-panel__header">
        <div><h2 id={`${id}-heading`}>Organization</h2><p>Pin, archive and track progress for the current selection.</p></div>
        <button className="button button--secondary" type="button" disabled={!selected.length || organization.loading} onClick={organization.refresh}>Refresh organization</button>
      </header>
      {!selected.length ? <p className="organization-panel__empty">Select a project or session to organize it.</p> : null}
      {organization.loading ? <p className="organization-panel__notice" role="status">Loading organization…</p> : null}
      {organization.error ? <p className="organization-panel__error" role="alert">{organization.error}</p> : null}
      <div className="organization-panel__cards">
        {selected.map(({ target, name, status }) => {
          const key = organizationTargetKey(target);
          return <OrganizationCard
            key={key}
            target={target}
            name={name}
            status={status}
            draft={organization.drafts[key]}
            loading={organization.loading || Boolean(organization.error)}
            operation={organization.operationFor(target)}
            onUpdate={(patch) => organization.update(target, patch)}
            onDiscard={() => organization.discard(target)}
            onSave={() => void organization.change(target, "saving")}
            onRefreshRevision={() => void organization.change(target, "refreshing")}
          />;
        })}
      </div>
    </section>
  );
}

interface OrganizationCardProps {
  readonly target: OrganizationTarget;
  readonly name: string;
  readonly status?: SessionStatus;
  readonly draft?: OrganizationDraft;
  readonly loading: boolean;
  readonly operation?: "saving" | "refreshing";
  readonly onUpdate: (patch: Partial<Pick<OrganizationDraft, "pinned" | "archived" | "workflow">>) => void;
  readonly onDiscard: () => void;
  readonly onSave: () => void;
  readonly onRefreshRevision: () => void;
}

function OrganizationCard({ target, name, status, draft, loading, operation, onUpdate, onDiscard, onSave, onRefreshRevision }: OrganizationCardProps) {
  const id = useId();
  const disabled = loading || Boolean(operation) || !draft;
  const dirty = draft ? hasOrganizationChanges(draft) : false;
  const kindLabel = target.kind === "project" ? "Project" : "Session";

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!disabled && dirty && !draft?.needsRebase) onSave();
  }

  return (
    <form className="organization-card" aria-labelledby={`${id}-heading`} onSubmit={submit}>
      <header className="organization-card__header">
        <div><p>{kindLabel}</p><h3 id={`${id}-heading`}>{name}</h3></div>
        {dirty ? <span className="organization-card__hint">Unsaved changes</span> : null}
      </header>
      {status ? <p className="organization-card__process">Daemon process: <StatusBadge status={status} compact /></p> : null}
      <fieldset disabled={disabled} className="organization-card__fields">
        <legend>{kindLabel} organization</legend>
        <label><input type="checkbox" checked={draft?.pinned ?? false} onChange={(event) => onUpdate({ pinned: event.currentTarget.checked })} />Pinned</label>
        <label><input type="checkbox" checked={draft?.archived ?? false} onChange={(event) => onUpdate({ archived: event.currentTarget.checked })} />Archived</label>
        {target.kind === "session" ? <label className="organization-card__workflow" htmlFor={`${id}-workflow`}>
          <span>Workflow</span>
          <select id={`${id}-workflow`} value={draft?.workflow ?? "backlog"} onChange={(event) => { if (isOrganizationWorkflow(event.currentTarget.value)) onUpdate({ workflow: event.currentTarget.value }); }}>
            {workflows.map((workflow) => <option key={workflow} value={workflow}>{workflowLabels[workflow]}</option>)}
          </select>
        </label> : null}
      </fieldset>
      {target.kind === "session" ? <p className="organization-card__hint">Workflow tracks your work separately from the daemon process.</p> : null}
      {(draft?.archived || draft?.original.archived) && status && isLiveStatus(status) ? <p className="organization-card__warning">The daemon reports {getSessionStatusLabel(status)}. Archiving changes organization only; this session continues running.</p> : null}
      {(draft?.archived || draft?.original.archived) && status === "unknown" ? <p className="organization-card__warning">Process status is unknown. Archiving does not stop a session.</p> : null}
      {draft?.error ? <div className="organization-panel__error" role="alert">
        <p>{draft.error}</p>
        {draft.needsRebase ? <p>Your choices are preserved. Refresh the revision, review the latest saved settings, and save again.</p> : null}
      </div> : null}
      {draft?.rebased ? <details className="organization-card__saved" open>
        <summary>Latest saved settings</summary>
        <p>Pinned: {draft.original.pinned ? "Yes" : "No"}. Archived: {draft.original.archived ? "Yes" : "No"}.{draft.original.workflow ? ` Workflow: ${workflowLabels[draft.original.workflow]}.` : ""}</p>
      </details> : null}
      <p className="organization-card__notice" role="status">{operation === "saving" ? "Saving organization…" : operation === "refreshing" ? "Refreshing the saved revision…" : draft?.notice ?? ""}</p>
      <div className="organization-card__actions">
        <button className="button button--primary" type="submit" disabled={disabled || !dirty || draft?.needsRebase}>{operation === "saving" ? "Saving…" : "Save organization"}</button>
        <button className="button button--secondary" type="button" disabled={disabled} onClick={onRefreshRevision}>Refresh revision</button>
        <button className="button button--quiet" type="button" disabled={disabled || !dirty} onClick={onDiscard}>Discard changes</button>
      </div>
    </form>
  );
}

const workflows: readonly OrganizationWorkflow[] = ["backlog", "in_progress", "in_review", "blocked", "done"];
const workflowLabels: Readonly<Record<OrganizationWorkflow, string>> = { backlog: "Backlog", in_progress: "In progress", in_review: "In review", blocked: "Blocked", done: "Done" };
