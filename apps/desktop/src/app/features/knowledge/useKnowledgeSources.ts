import { useEffect, useRef, useState } from "react";

import type { IpcClient } from "../../../ipc/client";
import type { KnowledgeDiscoverResponse, KnowledgeReadResponse, KnowledgeSourceEntry } from "../../../ipc/domain";
import type { ApiErrorData } from "../../../ipc/types";
import { IpcContractError } from "../../../ipc/schema";
import { errorData } from "../../utils";
import type { KnowledgeProject } from "./knowledge-types";

/** Read-only IPC boundary; a changed connection key invalidates ephemeral capabilities. */
export interface KnowledgeSourceInspectorProps {
  readonly client: Pick<IpcClient, "discoverKnowledge" | "readKnowledge">;
  readonly currentProject?: KnowledgeProject | null;
  /** Pass the daemon instance ID or reconnect generation when reusing a client object. */
  readonly connectionKey?: string | number;
}

interface ScanState {
  readonly client: KnowledgeSourceInspectorProps["client"];
  readonly projectId: string | null;
  readonly connectionKey: KnowledgeSourceInspectorProps["connectionKey"];
  readonly refreshIndex: number;
  readonly requestId: number;
  readonly result?: KnowledgeDiscoverResponse;
  readonly error?: ApiErrorData;
}

interface ReadState {
  readonly entryId: string;
  readonly loading: boolean;
  readonly result?: KnowledgeReadResponse;
  readonly error?: ApiErrorData;
}

/** Keeps scans and explicit previews bound to the current project, selection and connection. */
export function useKnowledgeSources({ client, currentProject, connectionKey }: KnowledgeSourceInspectorProps) {
  const projectId = currentProject?.id ?? null;
  const scanEpoch = useRef(0);
  const readEpoch = useRef(0);
  const [scan, setScan] = useState<ScanState>();
  const [refreshIndex, setRefreshIndex] = useState(0);
  const [selection, setSelection] = useState<{ readonly requestId: number; readonly entryId: string }>();
  const [read, setRead] = useState<ReadState>();
  const isCurrent = scan?.client === client && scan.projectId === projectId && scan.connectionKey === connectionKey && scan.refreshIndex === refreshIndex;
  const result = isCurrent ? scan.result : undefined;
  const selected = selection?.requestId === scan?.requestId
    ? result?.entries.find((entry) => entry.entryId === selection?.entryId)
    : undefined;

  useEffect(() => {
    const epoch = ++scanEpoch.current;
    readEpoch.current += 1;
    async function discover() {
      try {
        const discovered = await client.discoverKnowledge({ projectId });
        if (epoch === scanEpoch.current) {
          setScan({ client, projectId, connectionKey, refreshIndex, requestId: epoch, result: discovered });
        }
      } catch (error) {
        if (epoch === scanEpoch.current) {
          setScan({ client, projectId, connectionKey, refreshIndex, requestId: epoch, error: errorData(error) });
        }
      }
    }
    void discover();
    return () => {
      scanEpoch.current += 1;
      readEpoch.current += 1;
    };
  }, [client, projectId, connectionKey, refreshIndex]);

  function rescan() {
    scanEpoch.current += 1;
    readEpoch.current += 1;
    setSelection(undefined);
    setRead(undefined);
    setRefreshIndex((value) => value + 1);
  }

  async function selectSource(entry: KnowledgeSourceEntry) {
    if (!result || !scan) return;
    const epoch = ++readEpoch.current;
    const sourceScanEpoch = scanEpoch.current;
    setSelection({ requestId: scan.requestId, entryId: entry.entryId });
    setRead(undefined);
    if (entry.availability !== "available") return;
    setRead({ entryId: entry.entryId, loading: true });
    try {
      const preview = await client.readKnowledge({ scanId: result.scanId, entryId: entry.entryId });
      if (!sameSource(entry, preview.entry)) {
        throw new IpcContractError("Discovery read changed the selected source provenance");
      }
      if (epoch === readEpoch.current && sourceScanEpoch === scanEpoch.current) {
        setRead({ entryId: entry.entryId, result: preview, loading: false });
      }
    } catch (error) {
      if (epoch === readEpoch.current && sourceScanEpoch === scanEpoch.current) {
        setRead({ entryId: entry.entryId, error: errorData(error), loading: false });
      }
    }
  }

  return {
    result,
    loading: !isCurrent,
    scanError: isCurrent ? scan.error : undefined,
    selected,
    read: selected && read?.entryId === selected.entryId ? read : undefined,
    rescan,
    selectSource,
  };
}

/** A preview must retain all provenance from its selected inventory descriptor. */
function sameSource(selected: KnowledgeSourceEntry, returned: KnowledgeSourceEntry): boolean {
  const fields: readonly (keyof KnowledgeSourceEntry)[] = [
    "entryId", "kind", "provider", "scope", "sourcePath", "name",
    "scopeDirectory", "precedenceHint", "viaSymlink", "availability",
  ];
  return fields.every((field) => selected[field] === returned[field]);
}
