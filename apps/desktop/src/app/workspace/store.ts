import { createInitialWorkspaceState, type WorkspaceState } from "./types";

export type WorkspaceStore = {
  getState(): WorkspaceState;
  subscribe(listener: () => void): () => void;
  update(recipe: (state: WorkspaceState) => WorkspaceState): void;
};

/** Creates a subscription store so UI slices can skip unrelated renders. */
export function createWorkspaceStore(
  initial: WorkspaceState = createInitialWorkspaceState(),
): WorkspaceStore {
  let state = initial;
  const listeners = new Set<() => void>();

  return {
    getState() {
      return state;
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    update(recipe) {
      const next = recipe(state);
      if (next === state) {
        return;
      }
      state = next;
      for (const listener of listeners) {
        listener();
      }
    },
  };
}

/** Shallow comparison for selector results that are small records. */
export function shallowEqual<T>(left: T, right: T): boolean {
  if (Object.is(left, right)) {
    return true;
  }
  if (
    typeof left !== "object" ||
    typeof right !== "object" ||
    left === null ||
    right === null
  ) {
    return false;
  }
  const leftRecord = left as Record<string, unknown>;
  const rightRecord = right as Record<string, unknown>;
  const keys = Object.keys(leftRecord);
  if (keys.length !== Object.keys(rightRecord).length) {
    return false;
  }
  return keys.every((key) => Object.is(leftRecord[key], rightRecord[key]));
}
