import { createContext } from "react";

import type { WorkspaceActions } from "./types";
import type { WorkspaceStore } from "./store";
import type { TerminalRegistry } from "./terminal-registry";

/** Stable runtime handles. Components subscribe to slices instead of this object. */
export type WorkspaceRuntime = {
  readonly store: WorkspaceStore;
  readonly actions: WorkspaceActions;
  readonly terminals: TerminalRegistry;
};

export const WorkspaceRuntimeContext = createContext<WorkspaceRuntime | null>(
  null,
);
