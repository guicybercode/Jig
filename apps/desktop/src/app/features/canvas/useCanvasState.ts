import { useEffect, useReducer } from "react";

import {
  CANVAS_STORAGE_KEY,
  CANVAS_DOCUMENT_UPDATED_EVENT,
  canvasReducer,
  createInitialCanvasState,
  parseCanvasDocument,
  serializeCanvasDocument,
  toCanvasDocument,
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

  useEffect(() => {
    safelyWrite(storage, CANVAS_STORAGE_KEY, serializeCanvasDocument(state));
    globalThis.dispatchEvent?.(
      new CustomEvent(CANVAS_DOCUMENT_UPDATED_EVENT, {
        detail: toCanvasDocument(state),
      }),
    );
  }, [state, storage]);

  return {
    state,
    dispatch,
    persistenceAvailable: storage !== null,
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
): void {
  try {
    storage?.setItem(key, value);
  } catch {
    // Private browsing or a full quota must not make the canvas unusable.
  }
}
