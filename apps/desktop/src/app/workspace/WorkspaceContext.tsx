import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";

import {
  createTauriClient,
  formatApiError,
  type AgentInfo,
  type GitDiff,
  type GitStatus,
  type IpcClient,
  type Project,
  type Session,
  type StateSnapshot,
  type Worktree,
} from "../../lib/ipc";

export interface WorkspaceState {
  readonly connected: boolean;
  readonly loading: boolean;
  readonly error: string | null;
  readonly projects: Project[];
  readonly agents: AgentInfo[];
  readonly sessions: Session[];
  readonly worktrees: Worktree[];
  readonly selectedProjectId: string | null;
  readonly focusedSessionId: string | null;
  readonly visibleSessionIds: string[];
  readonly gitStatus: GitStatus | null;
  readonly gitDiff: GitDiff | null;
  readonly appVersion: string;
}

export interface WorkspaceApi extends WorkspaceState {
  refresh(): Promise<void>;
  addProject(path: string, name?: string): Promise<void>;
  removeProject(projectId: string): Promise<void>;
  selectProject(projectId: string | null): void;
  createCustomAgent(
    displayName: string,
    executable: string,
    args: string[],
  ): Promise<void>;
  createSession(input: {
    agentId: string;
    name?: string;
    createWorktree: boolean;
  }): Promise<void>;
  stopSession(sessionId: string): Promise<void>;
  deleteSession(sessionId: string): Promise<void>;
  focusSession(sessionId: string): void;
  toggleVisible(sessionId: string): void;
  writeSession(sessionId: string, bytes: Uint8Array): Promise<void>;
  resizeSession(sessionId: string, cols: number, rows: number): Promise<void>;
  subscribeReplay(
    sessionId: string,
  ): Promise<{ lastSequence: number; replayBase64: string }>;
  inspectGit(worktreeId?: string): Promise<void>;
  prepareRemoveWorktree(worktreeId: string): Promise<{
    isDirty: boolean;
    inUse: boolean;
    confirmationToken: string;
  }>;
  removeWorktree(
    worktreeId: string,
    confirmationToken: string,
    allowDirty: boolean,
  ): Promise<void>;
}

const WorkspaceContext = createContext<WorkspaceApi | null>(null);

interface ProviderProps {
  readonly children: ReactNode;
  readonly client?: IpcClient;
}

const emptyState: WorkspaceState = {
  connected: false,
  loading: true,
  error: null,
  projects: [],
  agents: [],
  sessions: [],
  worktrees: [],
  selectedProjectId: null,
  focusedSessionId: null,
  visibleSessionIds: [],
  gitStatus: null,
  gitDiff: null,
  appVersion: "0.1.0-beta.1",
};

/** Owns workspace metadata. PTY bytes stay outside this React state. */
export function WorkspaceProvider({ children, client }: ProviderProps) {
  const [ipc, setIpc] = useState<IpcClient | null>(client ?? null);
  const [state, setState] = useState<WorkspaceState>(emptyState);

  useEffect(() => {
    if (client) {
      setIpc(client);
      return;
    }
    let cancelled = false;
    void createTauriClient().then((resolved) => {
      if (!cancelled) {
        setIpc(resolved);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [client]);

  const refresh = useCallback(async () => {
    if (!ipc) {
      return;
    }
    setState((current) => ({ ...current, loading: true }));
    try {
      const snapshot = (await ipc.request(
        "state.snapshot",
        {},
      )) as StateSnapshot;
      setState((current) => {
        const selected =
          current.selectedProjectId &&
          snapshot.projects.some((project) => project.id === current.selectedProjectId)
            ? current.selectedProjectId
            : (snapshot.projects[0]?.id ?? null);
        const projectSessions = snapshot.sessions.filter(
          (session) => session.projectId === selected,
        );
        const visible = current.visibleSessionIds.filter((id) =>
          projectSessions.some((session) => session.id === id),
        );
        return {
          ...current,
          connected: true,
          loading: false,
          error: null,
          projects: snapshot.projects,
          agents: snapshot.agents,
          sessions: snapshot.sessions,
          worktrees: snapshot.worktrees,
          selectedProjectId: selected,
          visibleSessionIds: visible.slice(0, 4),
          focusedSessionId:
            current.focusedSessionId &&
            projectSessions.some((session) => session.id === current.focusedSessionId)
              ? current.focusedSessionId
              : (visible[0] ?? projectSessions[0]?.id ?? null),
          appVersion: snapshot.daemon.appVersion,
        };
      });
    } catch (error) {
      setState((current) => ({
        ...current,
        connected: false,
        loading: false,
        error: formatApiError(error),
      }));
    }
  }, [ipc]);

  useEffect(() => {
    if (ipc) {
      void refresh();
    }
  }, [ipc, refresh]);

  const api = useMemo<WorkspaceApi>(
    () => ({
      ...state,
      refresh,
      async addProject(path, name) {
        if (!ipc) {
          return;
        }
        await ipc.request("project.add", { path, name });
        await refresh();
      },
      async removeProject(projectId) {
        if (!ipc) {
          return;
        }
        await ipc.request("project.remove", { projectId });
        await refresh();
      },
      selectProject(projectId) {
        setState((current) => ({
          ...current,
          selectedProjectId: projectId,
          visibleSessionIds: current.sessions
            .filter((session) => session.projectId === projectId)
            .slice(0, 4)
            .map((session) => session.id),
        }));
      },
      async createCustomAgent(displayName, executable, args) {
        if (!ipc) {
          return;
        }
        await ipc.request("agent.custom.create", {
          displayName,
          executable,
          args,
        });
        await refresh();
      },
      async createSession(input) {
        if (!ipc || !state.selectedProjectId) {
          return;
        }
        const created = (await ipc.request("session.create", {
          projectId: state.selectedProjectId,
          agentId: input.agentId,
          name: input.name,
          createWorktree: input.createWorktree,
        })) as Session;
        await refresh();
        setState((current) => ({
          ...current,
          focusedSessionId: created.id,
          visibleSessionIds: uniqueIds([created.id, ...current.visibleSessionIds]).slice(
            0,
            4,
          ),
        }));
      },
      async stopSession(sessionId) {
        if (!ipc) {
          return;
        }
        await ipc.request("session.stop", { sessionId });
        await refresh();
      },
      async deleteSession(sessionId) {
        if (!ipc) {
          return;
        }
        await ipc.request("session.delete", { sessionId });
        await refresh();
      },
      focusSession(sessionId) {
        setState((current) => ({ ...current, focusedSessionId: sessionId }));
      },
      toggleVisible(sessionId) {
        setState((current) => {
          const exists = current.visibleSessionIds.includes(sessionId);
          const visible = exists
            ? current.visibleSessionIds.filter((id) => id !== sessionId)
            : uniqueIds([...current.visibleSessionIds, sessionId]).slice(0, 4);
          return {
            ...current,
            visibleSessionIds: visible,
            focusedSessionId: sessionId,
          };
        });
      },
      async writeSession(sessionId, bytes) {
        if (!ipc) {
          return;
        }
        await ipc.request("session.write", {
          sessionId,
          bytesBase64: bytesToBase64(bytes),
        });
      },
      async resizeSession(sessionId, cols, rows) {
        if (!ipc) {
          return;
        }
        await ipc.request("session.resize", { sessionId, cols, rows });
      },
      async subscribeReplay(sessionId) {
        if (!ipc) {
          return { lastSequence: 0, replayBase64: "" };
        }
        return (await ipc.request("session.subscribe", { sessionId })) as {
          lastSequence: number;
          replayBase64: string;
        };
      },
      async inspectGit(worktreeId) {
        if (!ipc || !state.selectedProjectId) {
          return;
        }
        const gitStatus = (await ipc.request("git.status", {
          projectId: state.selectedProjectId,
          worktreeId,
        })) as GitStatus;
        const gitDiff = (await ipc.request("git.diff", {
          projectId: state.selectedProjectId,
          worktreeId,
        })) as GitDiff;
        setState((current) => ({ ...current, gitStatus, gitDiff }));
      },
      async prepareRemoveWorktree(worktreeId) {
        if (!ipc) {
          throw new Error("Daemon unavailable");
        }
        return (await ipc.request("worktree.prepare_remove", { worktreeId })) as {
          isDirty: boolean;
          inUse: boolean;
          confirmationToken: string;
        };
      },
      async removeWorktree(worktreeId, confirmationToken, allowDirty) {
        if (!ipc) {
          return;
        }
        await ipc.request("worktree.remove", {
          worktreeId,
          confirmationToken,
          allowDirty,
        });
        await refresh();
      },
    }),
    [ipc, refresh, state],
  );

  return (
    <WorkspaceContext.Provider value={api}>{children}</WorkspaceContext.Provider>
  );
}

/** Reads the workspace API. Must be used under {@link WorkspaceProvider}. */
export function useWorkspace(): WorkspaceApi {
  const value = useContext(WorkspaceContext);
  if (!value) {
    throw new Error("useWorkspace requires WorkspaceProvider");
  }
  return value;
}

function uniqueIds(ids: string[]): string[] {
  return [...new Set(ids)];
}

function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  bytes.forEach((byte) => {
    binary += String.fromCharCode(byte);
  });
  return btoa(binary);
}
