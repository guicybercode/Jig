import { useEffect, useRef, useState } from "react";

import type { OrganizationEntry, OrganizationTarget } from "../../../ipc/domain";
import { organizationTargetKey } from "../../../ipc/organization-schema";
import { IpcContractError } from "../../../ipc/schema";
import { errorData } from "../../utils";
import type { OrganizationDraft, OrganizationPanelProps } from "./organization-types";

interface LoadState {
  readonly client: OrganizationPanelProps["client"];
  readonly connectionKey: OrganizationPanelProps["connectionKey"];
  readonly selectionKey: string;
  readonly refreshIndex: number;
  readonly error?: string;
}

interface Operation {
  readonly client: OrganizationPanelProps["client"];
  readonly connectionKey: OrganizationPanelProps["connectionKey"];
  readonly token: number;
  readonly kind: "saving" | "refreshing";
}

/** Preserves each entity's desired flags while correlating asynchronous reads and writes. */
export function useOrganization({ client, currentProject, currentSession, connectionKey, onChanged }: OrganizationPanelProps) {
  const projectId = currentProject?.id;
  const sessionId = currentSession?.id;
  const selectionKey = `${projectId ?? ""}/${sessionId ?? ""}`;
  const [drafts, setDrafts] = useState<Record<string, OrganizationDraft | undefined>>({});
  const [load, setLoad] = useState<LoadState>();
  const [refreshIndex, setRefreshIndex] = useState(0);
  const [operations, setOperations] = useState<Record<string, Operation | undefined>>({});
  const pending = useRef(new Map<string, Operation>());
  const loadEpoch = useRef(0);
  const connectionEpoch = useRef(0);
  const writeEpoch = useRef(0);
  const operationSequence = useRef(0);
  const isCurrent = load?.client === client && load.connectionKey === connectionKey && load.selectionKey === selectionKey && load.refreshIndex === refreshIndex;
  const hasTargets = Boolean(projectId || sessionId);

  useEffect(() => {
    connectionEpoch.current += 1;
    return () => { connectionEpoch.current += 1; };
  }, [client, connectionKey]);

  useEffect(() => {
    const epoch = ++loadEpoch.current;
    const beforeWrites = writeEpoch.current;
    const targets: OrganizationTarget[] = [];
    if (projectId) targets.push({ kind: "project", id: projectId });
    if (sessionId) targets.push({ kind: "session", id: sessionId });
    async function read() {
      if (!targets.length) return;
      try {
        const result = await client.getOrganization({ targets });
        if (epoch !== loadEpoch.current || beforeWrites !== writeEpoch.current) return;
        if (result.entries.length !== targets.length || result.entries.some((entry, index) => organizationTargetKey(entry.target) !== organizationTargetKey(targets[index]))) throw new IpcContractError("Organization response changed the selected targets");
        setDrafts((previous) => {
          const next = { ...previous };
          for (const entry of result.entries) {
            const key = organizationTargetKey(entry.target);
            const existing = previous[key];
            if (existing && (hasOrganizationChanges(existing) || existing.needsRebase)) continue;
            next[key] = { ...fromEntry(entry), notice: existing?.original.revision === entry.revision ? existing.notice : undefined };
          }
          return next;
        });
        setLoad({ client, connectionKey, selectionKey, refreshIndex });
      } catch (error) {
        if (epoch === loadEpoch.current && beforeWrites === writeEpoch.current) setLoad({ client, connectionKey, selectionKey, refreshIndex, error: errorData(error).message });
      }
    }
    void read();
    return () => { loadEpoch.current += 1; };
  }, [client, connectionKey, projectId, sessionId, selectionKey, refreshIndex]);

  function update(target: OrganizationTarget, patch: Partial<Pick<OrganizationDraft, "pinned" | "archived" | "workflow">>) {
    const key = organizationTargetKey(target);
    setDrafts((previous) => {
      const draft = previous[key];
      return draft ? { ...previous, [key]: { ...draft, ...patch, notice: undefined } } : previous;
    });
  }

  function discard(target: OrganizationTarget) {
    const key = organizationTargetKey(target);
    setDrafts((previous) => {
      const draft = previous[key];
      return draft ? { ...previous, [key]: fromEntry(draft.original) } : previous;
    });
  }

  async function change(target: OrganizationTarget, kind: Operation["kind"]) {
    const key = organizationTargetKey(target);
    const draft = drafts[key];
    const active = pending.current.get(key);
    if (!draft || (active?.client === client && active.connectionKey === connectionKey)) return;
    const operation = { client, connectionKey, kind, token: ++operationSequence.current };
    const transportEpoch = connectionEpoch.current;
    pending.current.set(key, operation);
    setOperations((previous) => ({ ...previous, [key]: operation }));
    setDrafts((previous) => ({ ...previous, [key]: { ...draft, error: undefined, notice: undefined } }));
    try {
      const updated = kind === "saving"
        ? await client.saveOrganization({ target, expectedRevision: draft.original.revision, pinned: draft.pinned, archived: draft.archived, workflow: draft.workflow })
        : (await client.getOrganization({ targets: [target] })).entries[0];
      if (connectionEpoch.current !== transportEpoch || pending.current.get(key)?.token !== operation.token) return;
      if (!updated || organizationTargetKey(updated.target) !== key) throw new IpcContractError("Organization operation returned another target");
      writeEpoch.current += 1;
      setDrafts((previous) => ({
        ...previous,
        [key]: kind === "saving"
          ? { ...fromEntry(updated), notice: "Organization saved locally." }
          : { ...(previous[key] ?? draft), original: updated, needsRebase: false, rebased: true, error: undefined, notice: "Latest revision loaded. Your choices are preserved; review them before saving." },
      }));
      setRefreshIndex((value) => value + 1);
      if (kind === "saving") {
        try { onChanged?.(updated); }
        catch { setDrafts((previous) => ({ ...previous, [key]: { ...fromEntry(updated), error: "Organization was saved, but the workspace could not refresh. Refresh the workspace to see the change." } })); }
      }
    } catch (error) {
      if (connectionEpoch.current !== transportEpoch || pending.current.get(key)?.token !== operation.token) return;
      const detail = errorData(error);
      setDrafts((previous) => ({ ...previous, [key]: { ...(previous[key] ?? draft), error: detail.message, needsRebase: detail.code === "organization_conflict" || draft.needsRebase } }));
    } finally {
      if (pending.current.get(key)?.token === operation.token) {
        pending.current.delete(key);
        setOperations((previous) => ({ ...previous, [key]: undefined }));
      }
    }
  }

  function operationFor(target: OrganizationTarget): Operation["kind"] | undefined {
    const operation = operations[organizationTargetKey(target)];
    return operation?.client === client && operation.connectionKey === connectionKey ? operation.kind : undefined;
  }

  return { drafts, loading: hasTargets && !isCurrent, error: isCurrent ? load.error : undefined, refresh: () => setRefreshIndex((value) => value + 1), update, discard, change, operationFor };
}

function fromEntry(entry: OrganizationEntry): OrganizationDraft {
  return { original: entry, pinned: entry.pinned, archived: entry.archived, workflow: entry.workflow };
}

/** Computes unsaved choices without duplicating dirty state. */
export function hasOrganizationChanges(draft: OrganizationDraft): boolean {
  return draft.pinned !== draft.original.pinned || draft.archived !== draft.original.archived || draft.workflow !== draft.original.workflow;
}
