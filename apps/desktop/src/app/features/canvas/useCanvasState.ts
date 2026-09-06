import { useEffect, useMemo, useReducer, useState } from "react";

import {
  CANVAS_STORAGE_KEY,
  CANVAS_DOCUMENT_UPDATED_EVENT,
  CANVAS_DOCUMENT_VERSION,
  canvasReducer,
  createInitialCanvasState,
  parseCanvasDocument,
} from "./canvas-state";
import type { CanvasAction, CanvasState } from "./canvas-state";

export interface CanvasStateController {
  readonly state: CanvasState;
  readonly dispatch: React.Dispatch<CanvasAction>;
  readonly persistenceAvailable: boolean;
}

/** Owns the canvas graph and persists only its durable document fields. */
export function useCanvasState(): CanvasStateController {
  const storage = getStorage();
  const [state, dispatch] = useReducer(
    canvasReducer,
    storage,
    (availableStorage) =>
      createInitialCanvasState(
        parseCanvasDocument(
          safelyRead(availableStorage, CANVAS_STORAGE_KEY),
        ),
      ),
  );
  const [persistenceAvailable, setPersistenceAvailable] = useState(storage !== null);
  const { nodes, connections, zoom, hiddenSessionIds } = state;
  const document = useMemo(() => ({
    version: CANVAS_DOCUMENT_VERSION,
    nodes,
    connections,
    zoom,
    hiddenSessionIds,
  }), [nodes, connections, zoom, hiddenSessionIds]);

  useEffect(() => {
    setPersistenceAvailable(safelyWrite(storage, CANVAS_STORAGE_KEY, JSON.stringify(document)));
    globalThis.dispatchEvent?.(
      new CustomEvent(CANVAS_DOCUMENT_UPDATED_EVENT, {
        detail: document,
      }),
    );
  }, [document, storage]);

  return {
    state,
    dispatch,
    persistenceAvailable,
  };
}

function getStorage(): Storage | null {
  try {
    return globalThis.localStorage ?? null;
  } catch {
    return null;
  }
}

function safelyRead(storage: Storage | null, key: string): string | null {
  try {
    return storage?.getItem(key) ?? null;
  } catch {
    return null;
  }
}

function safelyWrite(
  storage: Storage | null,
  key: string,
  value: string,
): boolean {
  try {
    if (!storage) return false;
    storage.setItem(key, value);
    return true;
  } catch {
    // Private browsing or a full quota must not make the canvas unusable.
    return false;
  }
}
