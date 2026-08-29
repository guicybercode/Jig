import { useContext, useRef, useSyncExternalStore } from "react";

import type {
  AgentRecord,
  Project,
  Session,
  Worktree,
} from "../../ipc";
import { WorkspaceRuntimeContext, type WorkspaceRuntime } from "./context";
import { connectionStatusLabel, isDaemonReady } from "./connection";
import {
  detectionForAgent,
  sessionsForProject,
  worktreesForProject,
} from "./queries";
import { shallowEqual } from "./store";
import {
  EMPTY_AGENTS,
  EMPTY_PROJECTS,
  EMPTY_SESSIONS,
  type ConnectionState,
  type DialogState,
  type GitViewState,
  type Notification,
  type PendingState,
  type SelectionState,
  type WorkspaceActions,
  type WorkspaceState,
} from "./types";

/** Reads the stable workspace runtime. Must be used under WorkspaceProvider. */
export function useWorkspaceRuntime(): WorkspaceRuntime {
  const runtime = useContext(WorkspaceRuntimeContext);
  if (!runtime) {
    throw new Error("Workspace hooks require WorkspaceProvider.");
  }
  return runtime;
}

/** Selects a slice of workspace state with cached identity. */
export function useWorkspaceSelector<T>(
  selector: (state: WorkspaceState) => T,
  isEqual: (left: T, right: T) => boolean = Object.is,
): T {
  const { store } = useWorkspaceRuntime();
  const selection = useRef(selector(store.getState()));
  return useSyncExternalStore(
    store.subscribe,
    () => {
      const next = selector(store.getState());
      if (isEqual(selection.current, next)) {
        return selection.current;
      }
      selection.current = next;
      return next;
    },
    () => selector(store.getState()),
  );
}

export function useWorkspaceActions(): WorkspaceActions {
  return useWorkspaceRuntime().actions;
}

export function useConnection(): ConnectionState {
  return useWorkspaceSelector((state) => state.connection);
}

export function useConnectionLabel(): string {
  return useWorkspaceSelector((state) => connectionStatusLabel(state.connection));
}

export function useDaemonReady(): boolean {
  return useWorkspaceSelector((state) => isDaemonReady(state.connection.phase));
}

export function useProjects(): readonly Project[] {
  return useWorkspaceSelector(
    (state) => state.snapshot?.projects ?? EMPTY_PROJECTS,
  );
}

export function useAgents(): readonly AgentRecord[] {
  return useWorkspaceSelector(
    (state) => state.snapshot?.agents ?? EMPTY_AGENTS,
  );
}

export function useSelection(): SelectionState {
  return useWorkspaceSelector((state) => state.selection, shallowEqual);
}

export function useSelectedProject(): Project | null {
  return useWorkspaceSelector((state) => {
    const projectId = state.selection.projectId;
    if (!projectId || !state.snapshot) {
      return null;
    }
    return (
      state.snapshot.projects.find((project) => project.id === projectId) ??
      null
    );
  });
}

export function useSelectedSessions(): readonly Session[] {
  return useWorkspaceSelector(
    (state) => sessionsForProject(state.snapshot, state.selection.projectId),
    listEqual,
  );
}

export function useSelectedWorktrees(): readonly Worktree[] {
  return useWorkspaceSelector(
    (state) => worktreesForProject(state.snapshot, state.selection.projectId),
    listEqual,
  );
}

export function useDialogs(): DialogState {
  return useWorkspaceSelector((state) => state.dialogs, shallowEqual);
}

export function useNotifications(): readonly Notification[] {
  return useWorkspaceSelector((state) => state.notifications);
}

export function usePending(): PendingState {
  return useWorkspaceSelector((state) => state.pending, shallowEqual);
}

export function useGit(): GitViewState {
  return useWorkspaceSelector((state) => state.git, shallowEqual);
}

export function useAgentAvailability(): ReadonlyMap<string, boolean> {
  return useWorkspaceSelector((state) => {
    const map = new Map<string, boolean>();
    for (const agent of state.snapshot?.agents ?? []) {
      const detected = detectionForAgent(state.detections, agent.id);
      if (detected !== undefined) {
        map.set(agent.id, detected);
      }
    }
    return map;
  }, mapEqual);
}

function listEqual<T>(left: readonly T[], right: readonly T[]): boolean {
  return (
    left.length === right.length &&
    left.every((item, index) => item === right[index])
  );
}

function mapEqual(
  left: ReadonlyMap<string, boolean>,
  right: ReadonlyMap<string, boolean>,
): boolean {
  if (left.size !== right.size) {
    return false;
  }
  for (const [key, value] of left) {
    if (right.get(key) !== value) {
      return false;
    }
  }
  return true;
}

export { EMPTY_SESSIONS, shallowEqual };
