import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useReducer,
  useRef,
} from "react";
import type { ReactNode } from "react";

import {
  eventDecoders,
  tauriIpcClient,
  toIpcError,
} from "../../ipc/client";
import type { AppPlatform, IpcClient } from "../../ipc/client";
import {
  IpcContractError,
  requireNumber,
  requireRecord,
  requireString,
} from "../../ipc/schema";
import type {
  AddProjectInput,
  AgentDetection,
  AgentRecord,
  ApiErrorData,
  BootstrapResult,
  CreateCustomAgentInput,
  CreateSessionInput,
  DiagnosticsSnapshot,
  EventEnvelope,
  GitStatus,
  GitTarget,
  HelloResponse,
  Project,
  RemoveWorktreeInput,
  RenameProjectInput,
  RenameSessionInput,
  Session,
  SessionIdInput,
  SessionStatus,
  StateSnapshot,
  Worktree,
  WorktreeRemovalPreparation,
} from "../../ipc/types";

/** Main application destinations controlled independently from dialog state. */
export type WorkspaceView =
  | "session"
  | "grid"
  | "settings"
  | "diagnostics";

/** Every application-level overlay has one explicit, typed target. */
export type WorkspaceOverlay =
  | { readonly kind: "add-project" }
  | { readonly kind: "new-session"; readonly projectId: string }
  | { readonly kind: "rename-project"; readonly projectId: string }
  | { readonly kind: "remove-project"; readonly projectId: string }
  | { readonly kind: "rename-session"; readonly sessionId: string }
  | { readonly kind: "stop-session"; readonly sessionId: string }
  | { readonly kind: "delete-session"; readonly sessionId: string }
  | { readonly kind: "remove-worktree"; readonly worktreeId: string }
  | { readonly kind: "git-status"; readonly sessionId: string }
  | { readonly kind: "command-palette" };

/** Connection state shown by the shell and status bar. */
export type DaemonConnection =
  | {
      readonly status: "connecting";
      readonly error?: never;
      readonly hello?: never;
    }
  | {
      readonly status: "connected";
      readonly hello: HelloResponse;
      readonly error?: never;
    }
  | {
      readonly status: "disconnected" | "fatal";
      readonly error: ApiErrorData;
      readonly hello?: HelloResponse;
    };

/** Mutations and native reads available to workspace UI features. */
export interface WorkspaceOperations {
  addProject(input: AddProjectInput): Promise<Project>;
  renameProject(input: RenameProjectInput): Promise<Project>;
  removeProject(projectId: string): Promise<void>;
  createCustomAgent(input: CreateCustomAgentInput): Promise<AgentRecord>;
  createSession(input: CreateSessionInput): Promise<Session>;
  startSession(input: SessionIdInput): Promise<Session>;
  stopSession(input: SessionIdInput): Promise<Session>;
  restartSession(input: SessionIdInput): Promise<Session>;
  renameSession(input: RenameSessionInput): Promise<Session>;
  deleteSession(input: SessionIdInput): Promise<void>;
  getGitStatus(target: GitTarget): Promise<GitStatus>;
  prepareWorktreeRemoval(
    worktreeId: string,
  ): Promise<WorktreeRemovalPreparation>;
  removeWorktree(input: RemoveWorktreeInput): Promise<void>;
  getDiagnostics(): Promise<DiagnosticsSnapshot>;
  openPath(path: string): Promise<void>;
}

/** Stable state and actions consumed by project/session UI features. */
export interface WorkspaceContextValue extends WorkspaceOperations {
  readonly platform: AppPlatform;
  readonly connection: DaemonConnection;
  readonly isConnected: boolean;
  readonly isLoading: boolean;
  readonly hello: HelloResponse | null;
  readonly snapshot: StateSnapshot | null;
  readonly projects: readonly Project[];
  readonly agents: readonly AgentRecord[];
  readonly agentDetections: readonly AgentDetection[];
  readonly sessions: readonly Session[];
  readonly worktrees: readonly Worktree[];
  readonly selectedProjectId: string | null;
  readonly selectedSessionId: string | null;
  readonly selectedProject: Project | null;
  readonly selectedSession: Session | null;
  readonly selectedWorktree: Worktree | null;
  readonly projectSessions: readonly Session[];
  readonly view: WorkspaceView;
  readonly overlay: WorkspaceOverlay | null;
  readonly operationError: ApiErrorData | null;
  retry(): void;
  selectProject(projectId: string | null): void;
  selectSession(sessionId: string | null): void;
  setView(view: WorkspaceView): void;
  openOverlay(overlay: WorkspaceOverlay): void;
  closeOverlay(): void;
  clearOperationError(): void;
}

export interface WorkspaceProviderProps {
  readonly children: ReactNode;
  /** Production uses the shared Tauri client; tests inject a strict fake. */
  readonly client?: IpcClient;
}

interface WorkspaceState {
  readonly connection: DaemonConnection;
  readonly snapshot: StateSnapshot | null;
  readonly agentDetections: readonly AgentDetection[];
  readonly selectedProjectId: string | null;
  readonly selectedSessionId: string | null;
  readonly navigationRevision: number;
  readonly view: WorkspaceView;
  readonly overlay: WorkspaceOverlay | null;
  readonly operationError: ApiErrorData | null;
  readonly retrySequence: number;
}

type WorkspaceAction =
  | { readonly type: "connection/connecting" }
  | {
      readonly type: "connection/ready";
      readonly bootstrap: BootstrapResult;
    }
  | {
      readonly type: "connection/failed";
      readonly error: ApiErrorData;
      readonly isFatal: boolean;
    }
  | {
      readonly type: "connection/disconnected";
      readonly error: ApiErrorData;
    }
  | { readonly type: "connection/retry" }
  | {
      readonly type: "metadata/project-upserted";
      readonly project: Project;
      readonly selectIfRevision?: number;
    }
  | {
      readonly type: "metadata/project-removed";
      readonly projectId: string;
    }
  | {
      readonly type: "metadata/agent-upserted";
      readonly agent: AgentRecord;
    }
  | { readonly type: "metadata/agent-removed"; readonly agentId: string }
  | {
      readonly type: "metadata/session-upserted";
      readonly session: Session;
      readonly selectIfRevision?: number;
    }
  | {
      readonly type: "metadata/session-removed";
      readonly sessionId: string;
    }
  | {
      readonly type: "metadata/session-lifecycle-patched";
      readonly sessionId: string;
      readonly status: SessionStatus;
      readonly updatedAtMs: number;
      readonly reasonCode?: string;
      readonly setsExitCode: boolean;
      readonly exitCode?: number;
    }
  | {
      readonly type: "metadata/worktree-upserted";
      readonly worktree: Worktree;
    }
  | {
      readonly type: "metadata/worktree-removed";
      readonly worktreeId: string;
    }
  | {
      readonly type: "selection/project";
      readonly projectId: string | null;
    }
  | {
      readonly type: "selection/session";
      readonly sessionId: string | null;
    }
  | { readonly type: "view/set"; readonly view: WorkspaceView }
  | {
      readonly type: "overlay/open";
      readonly overlay: WorkspaceOverlay;
    }
  | { readonly type: "overlay/close" }
  | { readonly type: "operation/started" }
  | {
      readonly type: "operation/failed";
      readonly error: ApiErrorData;
    }
  | { readonly type: "operation/error-cleared" };

const EMPTY_PROJECTS: readonly Project[] = [];
const EMPTY_AGENTS: readonly AgentRecord[] = [];
const EMPTY_AGENT_DETECTIONS: readonly AgentDetection[] = [];
const EMPTY_SESSIONS: readonly Session[] = [];
const EMPTY_WORKTREES: readonly Worktree[] = [];

const INITIAL_WORKSPACE_STATE: WorkspaceState = {
  connection: { status: "connecting" },
  snapshot: null,
  agentDetections: EMPTY_AGENT_DETECTIONS,
  selectedProjectId: null,
  selectedSessionId: null,
  navigationRevision: 0,
  view: "session",
  overlay: null,
  operationError: null,
  retrySequence: 0,
};

const WorkspaceContext = createContext<WorkspaceContextValue | undefined>(
  undefined,
);

/** Owns the single desktop bootstrap, event stream, and metadata snapshot. */
export function WorkspaceProvider({
  children,
  client = tauriIpcClient,
}: WorkspaceProviderProps) {
  const [state, dispatch] = useReducer(
    workspaceReducer,
    INITIAL_WORKSPACE_STATE,
  );
  const requestGenerationRef = useRef(0);
  const navigationRevisionRef = useRef(state.navigationRevision);
  navigationRevisionRef.current = state.navigationRevision;

  useEffect(() => {
    requestGenerationRef.current += 1;
    let isActive = true;
    let shouldKeepSubscription = true;
    let isBootstrapped = false;
    let isEventRoutingDisabled = false;
    let hasEventFailure = false;
    let lastEventSequence: number | null = null;
    let unsubscribe: (() => void) | undefined;
    const pendingEvents: EventEnvelope[] = [];

    dispatch({ type: "connection/connecting" });

    const stopSubscription = () => {
      shouldKeepSubscription = false;
      unsubscribe?.();
      unsubscribe = undefined;
    };

    const routeEvent = (event: EventEnvelope) => {
      if (isEventRoutingDisabled) {
        return;
      }
      try {
        const action = actionForEvent(event);
        if (action !== null) {
          dispatch(action);
        }
        if (event.event === "daemon.shutting_down") {
          isEventRoutingDisabled = true;
          stopSubscription();
          requestGenerationRef.current += 1;
        }
      } catch (error) {
        isEventRoutingDisabled = true;
        hasEventFailure = true;
        stopSubscription();
        requestGenerationRef.current += 1;
        dispatch({
          type: "connection/failed",
          error: toErrorData(error),
          isFatal: true,
        });
      }
    };

    const handleEvent = (event: EventEnvelope) => {
      if (!isActive || isEventRoutingDisabled) {
        return;
      }
      if (!Number.isSafeInteger(event.sequence) || event.sequence < 0) {
        isEventRoutingDisabled = true;
        hasEventFailure = true;
        stopSubscription();
        requestGenerationRef.current += 1;
        dispatch({
          type: "connection/failed",
          error: {
            code: "invalid_event_sequence",
            message: "The local daemon sent an invalid event sequence.",
            action:
              "Retry the connection and verify that desktop and daemon versions match.",
            details: { receivedSequence: event.sequence },
          },
          isFatal: true,
        });
        return;
      }
      if (lastEventSequence !== null && event.sequence <= lastEventSequence) {
        return;
      }
      lastEventSequence = event.sequence;

      // Terminal controllers consume these separately; bytes never enter React.
      if (
        event.event === "session.output" ||
        event.event === "session.output_gap" ||
        event.event === "session.replay_complete"
      ) {
        return;
      }
      if (!isBootstrapped) {
        pendingEvents.push(event);
        return;
      }
      routeEvent(event);
    };

    const handleEventError = (error: unknown) => {
      if (!isActive || isEventRoutingDisabled) return;
      isEventRoutingDisabled = true;
      hasEventFailure = true;
      stopSubscription();
      requestGenerationRef.current += 1;
      dispatch({
        type: "connection/failed",
        error: toErrorData(error),
        isFatal: true,
      });
    };

    const subscriptionPromise = Promise.resolve().then(async () => {
      if (!isActive) {
        return false;
      }
      const nextUnsubscribe = await client.subscribe(handleEvent, handleEventError);
      if (!isActive || !shouldKeepSubscription) {
        nextUnsubscribe();
        return false;
      }
      unsubscribe = nextUnsubscribe;
      return true;
    });

    void subscriptionPromise
      .then(async (didSubscribe) => {
        if (!didSubscribe || !isActive || hasEventFailure) {
          return undefined;
        }
        return client.initialize();
      })
      .then((bootstrap) => {
        if (bootstrap === undefined || !isActive || hasEventFailure) {
          return;
        }
        dispatch({ type: "connection/ready", bootstrap });
        isBootstrapped = true;
        for (const event of pendingEvents) {
          routeEvent(event);
          if (isEventRoutingDisabled) {
            break;
          }
        }
        pendingEvents.length = 0;
      })
      .catch((error: unknown) => {
        stopSubscription();
        if (!isActive || hasEventFailure) {
          return;
        }
        requestGenerationRef.current += 1;
        const normalized = toErrorData(error);
        dispatch({
          type: "connection/failed",
          error: normalized,
          isFatal: isFatalConnectionError(normalized),
        });
      });

    return () => {
      isActive = false;
      pendingEvents.length = 0;
      stopSubscription();
    };
  }, [client, state.retrySequence]);

  const execute = useCallback(
    async <T,>(
      request: () => Promise<T>,
      applyResult?: (result: T) => void,
      affectsDaemonConnection = true,
    ): Promise<T> => {
      const requestGeneration = requestGenerationRef.current;
      dispatch({ type: "operation/started" });
      try {
        const result = await request();
        if (requestGeneration === requestGenerationRef.current) {
          applyResult?.(result);
        }
        return result;
      } catch (error) {
        const normalized = toIpcError(error);
        if (requestGeneration === requestGenerationRef.current) {
          dispatch({
            type: "operation/failed",
            error: toErrorData(normalized),
          });
          if (
            affectsDaemonConnection &&
            (isFatalConnectionError(normalized) ||
              isDisconnectedError(normalized))
          ) {
            requestGenerationRef.current += 1;
            dispatch({
              type: "connection/failed",
              error: toErrorData(normalized),
              isFatal: isFatalConnectionError(normalized),
            });
          }
        }
        throw normalized;
      }
    },
    [],
  );

  const retry = useCallback(() => {
    // Invalidate mutation results from the prior daemon connection immediately.
    requestGenerationRef.current += 1;
    dispatch({ type: "connection/retry" });
  }, []);

  const selectProject = useCallback((projectId: string | null) => {
    dispatch({ type: "selection/project", projectId });
  }, []);

  const selectSession = useCallback((sessionId: string | null) => {
    dispatch({ type: "selection/session", sessionId });
  }, []);

  const setView = useCallback((view: WorkspaceView) => {
    dispatch({ type: "view/set", view });
  }, []);

  const openOverlay = useCallback((overlay: WorkspaceOverlay) => {
    dispatch({ type: "overlay/open", overlay });
  }, []);

  const closeOverlay = useCallback(() => {
    dispatch({ type: "overlay/close" });
  }, []);

  const clearOperationError = useCallback(() => {
    dispatch({ type: "operation/error-cleared" });
  }, []);

  const addProject = useCallback(
    (input: AddProjectInput) => {
      const selectIfRevision = navigationRevisionRef.current;
      return execute(
        () => client.addProject(input),
        (project) => dispatch({
          type: "metadata/project-upserted",
          project,
          selectIfRevision,
        }),
      );
    },
    [client, execute],
  );

  const renameProject = useCallback(
    (input: RenameProjectInput) =>
      execute(
        () => client.renameProject(input),
        (project) => dispatch({
          type: "metadata/project-upserted",
          project,
        }),
      ),
    [client, execute],
  );

  const removeProject = useCallback(
    (projectId: string) =>
      execute(
        () => client.removeProject(projectId),
        () => dispatch({
          type: "metadata/project-removed",
          projectId,
        }),
      ),
    [client, execute],
  );

  const createCustomAgent = useCallback(
    (input: CreateCustomAgentInput) =>
      execute(
        () => client.createCustomAgent(input),
        (agent) => dispatch({ type: "metadata/agent-upserted", agent }),
      ),
    [client, execute],
  );

  const createSession = useCallback(
    (input: CreateSessionInput) => {
      const selectIfRevision = navigationRevisionRef.current;
      return execute(
        () => client.createSession(input),
        (session) => dispatch({
          type: "metadata/session-upserted",
          session,
          selectIfRevision,
        }),
      );
    },
    [client, execute],
  );

  const startSession = useCallback(
    (input: SessionIdInput) =>
      execute(
        () => client.startSession(input),
        (session) => dispatch({
          type: "metadata/session-upserted",
          session,
        }),
      ),
    [client, execute],
  );

  const stopSession = useCallback(
    (input: SessionIdInput) =>
      execute(
        () => client.stopSession(input),
        (session) => dispatch({
          type: "metadata/session-upserted",
          session,
        }),
      ),
    [client, execute],
  );

  const restartSession = useCallback(
    (input: SessionIdInput) =>
      execute(
        () => client.restartSession(input),
        (session) => dispatch({
          type: "metadata/session-upserted",
          session,
        }),
      ),
    [client, execute],
  );

  const renameSession = useCallback(
    (input: RenameSessionInput) =>
      execute(
        () => client.renameSession(input),
        (session) => dispatch({
          type: "metadata/session-upserted",
          session,
        }),
      ),
    [client, execute],
  );

  const deleteSession = useCallback(
    (input: SessionIdInput) =>
      execute(
        () => client.deleteSession(input),
        () => dispatch({
          type: "metadata/session-removed",
          sessionId: input.sessionId,
        }),
      ),
    [client, execute],
  );

  const getGitStatus = useCallback(
    (target: GitTarget) => execute(() => client.getGitStatus(target)),
    [client, execute],
  );

  const prepareWorktreeRemoval = useCallback(
    (worktreeId: string) =>
      execute(() => client.prepareWorktreeRemoval(worktreeId)),
    [client, execute],
  );

  const removeWorktree = useCallback(
    (input: RemoveWorktreeInput) =>
      execute(
        () => client.removeWorktree(input),
        () => dispatch({
          type: "metadata/worktree-removed",
          worktreeId: input.worktreeId,
        }),
      ),
    [client, execute],
  );

  const getDiagnostics = useCallback(
    () => execute(() => client.getDiagnostics()),
    [client, execute],
  );

  const openPath = useCallback(
    (path: string) => execute(() => client.openPath(path), undefined, false),
    [client, execute],
  );

  const projects = state.snapshot?.projects ?? EMPTY_PROJECTS;
  const agents = state.snapshot?.agents ?? EMPTY_AGENTS;
  const agentDetections = state.agentDetections;
  const sessions = state.snapshot?.sessions ?? EMPTY_SESSIONS;
  const worktrees = state.snapshot?.worktrees ?? EMPTY_WORKTREES;
  const selectedProject =
    projects.find((project) => project.id === state.selectedProjectId) ?? null;
  const selectedSession =
    sessions.find((session) => session.id === state.selectedSessionId) ?? null;
  const selectedWorktree =
    worktrees.find(
      (worktree) => worktree.id === selectedSession?.worktreeId,
    ) ?? null;
  const projectSessions =
    state.selectedProjectId === null
      ? EMPTY_SESSIONS
      : sessions.filter(
          (session) => session.projectId === state.selectedProjectId,
        );
  const hello = getConnectionHello(state.connection);

  const value: WorkspaceContextValue = {
    platform: client.platform,
    connection: state.connection,
    isConnected: state.connection.status === "connected",
    isLoading: state.connection.status === "connecting",
    hello,
    snapshot: state.snapshot,
    projects,
    agents,
    agentDetections,
    sessions,
    worktrees,
    selectedProjectId: state.selectedProjectId,
    selectedSessionId: state.selectedSessionId,
    selectedProject,
    selectedSession,
    selectedWorktree,
    projectSessions,
    view: state.view,
    overlay: state.overlay,
    operationError: state.operationError,
    retry,
    selectProject,
    selectSession,
    setView,
    openOverlay,
    closeOverlay,
    clearOperationError,
    addProject,
    renameProject,
    removeProject,
    createCustomAgent,
    createSession,
    startSession,
    stopSession,
    restartSession,
    renameSession,
    deleteSession,
    getGitStatus,
    prepareWorktreeRemoval,
    removeWorktree,
    getDiagnostics,
    openPath,
  };

  return (
    <WorkspaceContext.Provider value={value}>
      {children}
    </WorkspaceContext.Provider>
  );
}

/** Reads the centralized workspace controller. */
export function useWorkspace(): WorkspaceContextValue {
  const value = useContext(WorkspaceContext);
  if (value === undefined) {
    throw new Error("useWorkspace must be used within a WorkspaceProvider.");
  }
  return value;
}

function workspaceReducer(
  state: WorkspaceState,
  action: WorkspaceAction,
): WorkspaceState {
  switch (action.type) {
    case "connection/connecting":
      return {
        ...state,
        connection: { status: "connecting" },
        operationError: null,
      };
    case "connection/ready":
      return hydrateState(state, action.bootstrap);
    case "connection/failed":
      return {
        ...state,
        connection: {
          status: action.isFatal ? "fatal" : "disconnected",
          error: action.error,
        },
      };
    case "connection/disconnected":
      return {
        ...state,
        connection: {
          status: "disconnected",
          hello: getConnectionHello(state.connection) ?? undefined,
          error: action.error,
        },
      };
    case "connection/retry":
      return {
        ...state,
        connection: { status: "connecting" },
        operationError: null,
        retrySequence: state.retrySequence + 1,
      };
    case "metadata/project-upserted":
      return upsertProject(state, action);
    case "metadata/project-removed":
      return removeProjectFromState(state, action.projectId);
    case "metadata/agent-upserted":
      return {
        ...updateSnapshot(state, (snapshot) => ({
          ...snapshot,
          agents: upsertAgentRecord(snapshot.agents, action.agent),
        })),
        agentDetections: state.agentDetections.filter(
          (detection) => detection.agentId !== action.agent.id,
        ),
      };
    case "metadata/agent-removed":
      return {
        ...updateSnapshot(state, (snapshot) => ({
          ...snapshot,
          agents: snapshot.agents.filter(
            (agent) => agent.id !== action.agentId,
          ),
        })),
        agentDetections: state.agentDetections.filter(
          (detection) => detection.agentId !== action.agentId,
        ),
      };
    case "metadata/session-upserted":
      return upsertSession(state, action);
    case "metadata/session-removed":
      return removeSessionFromState(state, action.sessionId);
    case "metadata/session-lifecycle-patched":
      return patchSessionLifecycle(state, action);
    case "metadata/worktree-upserted":
      return updateSnapshot(state, (snapshot) => ({
        ...snapshot,
        worktrees: upsertById(snapshot.worktrees, action.worktree),
      }));
    case "metadata/worktree-removed":
      return removeWorktreeFromState(state, action.worktreeId);
    case "selection/project":
      return selectProjectInState(state, action.projectId);
    case "selection/session":
      return selectSessionInState(state, action.sessionId);
    case "view/set":
      if (action.view === state.view) {
        return state;
      }
      return {
        ...state,
        view: action.view,
        navigationRevision: state.navigationRevision + 1,
      };
    case "overlay/open":
      return { ...state, overlay: action.overlay };
    case "overlay/close":
      return state.overlay === null ? state : { ...state, overlay: null };
    case "operation/started":
    case "operation/error-cleared":
      return state.operationError === null
        ? state
        : { ...state, operationError: null };
    case "operation/failed":
      return { ...state, operationError: action.error };
  }
}

function hydrateState(
  state: WorkspaceState,
  bootstrap: BootstrapResult,
): WorkspaceState {
  const selectedProjectId = resolveProjectId(
    bootstrap.snapshot.projects,
    state.selectedProjectId,
  );
  const selectedSessionId = resolveSessionId(
    bootstrap.snapshot.sessions,
    selectedProjectId,
    state.selectedSessionId,
  );
  return {
    ...state,
    connection: { status: "connected", hello: bootstrap.hello },
    snapshot: bootstrap.snapshot,
    agentDetections: bootstrap.agentDetections,
    selectedProjectId,
    selectedSessionId,
    overlay: reconcileOverlay(state.overlay, bootstrap.snapshot),
    operationError: null,
  };
}

function upsertProject(
  state: WorkspaceState,
  action: Extract<WorkspaceAction, { readonly type: "metadata/project-upserted" }>,
): WorkspaceState {
  if (state.snapshot === null) {
    return state;
  }
  const shouldSelect =
    action.selectIfRevision !== undefined &&
    action.selectIfRevision === state.navigationRevision;
  return {
    ...state,
    snapshot: {
      ...state.snapshot,
      projects: upsertById(state.snapshot.projects, action.project),
    },
    selectedProjectId: shouldSelect
      ? action.project.id
      : state.selectedProjectId,
    selectedSessionId: shouldSelect ? null : state.selectedSessionId,
    navigationRevision: shouldSelect
      ? state.navigationRevision + 1
      : state.navigationRevision,
    view: shouldSelect ? "session" : state.view,
  };
}

function upsertSession(
  state: WorkspaceState,
  action: Extract<WorkspaceAction, { readonly type: "metadata/session-upserted" }>,
): WorkspaceState {
  if (state.snapshot === null) {
    return state;
  }
  const shouldSelect =
    action.selectIfRevision !== undefined &&
    action.selectIfRevision === state.navigationRevision &&
    state.selectedProjectId === action.session.projectId;
  return {
    ...state,
    snapshot: {
      ...state.snapshot,
      sessions: upsertById(state.snapshot.sessions, action.session),
    },
    selectedSessionId: shouldSelect
      ? action.session.id
      : state.selectedSessionId,
    navigationRevision: shouldSelect
      ? state.navigationRevision + 1
      : state.navigationRevision,
    view: shouldSelect ? "session" : state.view,
  };
}

function removeProjectFromState(
  state: WorkspaceState,
  projectId: string,
): WorkspaceState {
  if (state.snapshot === null) {
    return state;
  }
  const projects = withoutId(state.snapshot.projects, projectId);
  const sessions = state.snapshot.sessions.filter(
    (session) => session.projectId !== projectId,
  );
  const worktrees = state.snapshot.worktrees.filter(
    (worktree) => worktree.projectId !== projectId,
  );
  const didRemoveSelection = state.selectedProjectId === projectId;
  const selectedProjectId = didRemoveSelection
    ? resolveProjectId(projects, null)
    : state.selectedProjectId;
  const selectedSessionId = didRemoveSelection
    ? null
    : resolveSessionId(sessions, selectedProjectId, state.selectedSessionId);
  return {
    ...state,
    snapshot: { ...state.snapshot, projects, sessions, worktrees },
    selectedProjectId,
    selectedSessionId,
    navigationRevision: didRemoveSelection
      ? state.navigationRevision + 1
      : state.navigationRevision,
    overlay: referencesProject(state.overlay, projectId)
      ? null
      : state.overlay,
  };
}

function removeSessionFromState(
  state: WorkspaceState,
  sessionId: string,
): WorkspaceState {
  if (state.snapshot === null) {
    return state;
  }
  const didRemoveSelection = state.selectedSessionId === sessionId;
  return {
    ...state,
    snapshot: {
      ...state.snapshot,
      sessions: withoutId(state.snapshot.sessions, sessionId),
      worktrees: state.snapshot.worktrees.map((worktree) =>
        worktree.sessionId === sessionId
          ? { ...worktree, sessionId: undefined }
          : worktree,
      ),
    },
    selectedSessionId: didRemoveSelection ? null : state.selectedSessionId,
    navigationRevision: didRemoveSelection
      ? state.navigationRevision + 1
      : state.navigationRevision,
    overlay: referencesSession(state.overlay, sessionId) ? null : state.overlay,
  };
}

function patchSessionLifecycle(
  state: WorkspaceState,
  action: Extract<
    WorkspaceAction,
    { readonly type: "metadata/session-lifecycle-patched" }
  >,
): WorkspaceState {
  if (state.snapshot === null) {
    return state;
  }
  let didPatch = false;
  const sessions = state.snapshot.sessions.map((session) => {
    if (session.id !== action.sessionId) {
      return session;
    }
    didPatch = true;
    return {
      ...session,
      status: action.status,
      updatedAtMs: action.updatedAtMs,
      exitCode: action.setsExitCode ? action.exitCode : session.exitCode,
      errorCode:
        action.status === "failed"
          ? action.reasonCode ?? session.errorCode
          : undefined,
    };
  });
  return didPatch
    ? { ...state, snapshot: { ...state.snapshot, sessions } }
    : state;
}

function removeWorktreeFromState(
  state: WorkspaceState,
  worktreeId: string,
): WorkspaceState {
  if (state.snapshot === null) {
    return state;
  }
  return {
    ...state,
    snapshot: {
      ...state.snapshot,
      worktrees: withoutId(state.snapshot.worktrees, worktreeId),
    },
    overlay:
      state.overlay?.kind === "remove-worktree" &&
      state.overlay.worktreeId === worktreeId
        ? null
        : state.overlay,
  };
}

function selectProjectInState(
  state: WorkspaceState,
  projectId: string | null,
): WorkspaceState {
  if (
    projectId !== null &&
    !state.snapshot?.projects.some((project) => project.id === projectId)
  ) {
    return state;
  }
  const selectedSessionId =
    projectId !== null &&
    state.snapshot?.sessions.some(
      (session) =>
        session.id === state.selectedSessionId &&
        session.projectId === projectId,
    )
      ? state.selectedSessionId
      : null;
  return {
    ...state,
    selectedProjectId: projectId,
    selectedSessionId,
    navigationRevision: state.navigationRevision + 1,
    view: "session",
  };
}

function selectSessionInState(
  state: WorkspaceState,
  sessionId: string | null,
): WorkspaceState {
  if (sessionId === null) {
    return {
      ...state,
      selectedSessionId: null,
      navigationRevision: state.navigationRevision + 1,
      view: "session",
    };
  }
  const session = state.snapshot?.sessions.find(
    (candidate) => candidate.id === sessionId,
  );
  if (session === undefined) {
    return state;
  }
  return {
    ...state,
    selectedProjectId: session.projectId,
    selectedSessionId: session.id,
    navigationRevision: state.navigationRevision + 1,
    view: "session",
  };
}

function updateSnapshot(
  state: WorkspaceState,
  update: (snapshot: StateSnapshot) => StateSnapshot,
): WorkspaceState {
  if (state.snapshot === null) {
    return state;
  }
  return { ...state, snapshot: update(state.snapshot) };
}

function upsertById<T extends { readonly id: string }>(
  items: readonly T[],
  item: T,
): readonly T[] {
  let didReplace = false;
  const nextItems = items.map((existing) => {
    if (existing.id !== item.id) {
      return existing;
    }
    didReplace = true;
    return item;
  });
  return didReplace ? nextItems : [...items, item];
}

function upsertAgentRecord(
  agents: readonly AgentRecord[],
  agent: AgentRecord,
): readonly AgentRecord[] {
  let didReplace = false;
  const nextAgents = agents.map((existing) => {
    if (existing.id !== agent.id) {
      return existing;
    }
    didReplace = true;
    return agent;
  });
  return didReplace ? nextAgents : [...agents, agent];
}

function withoutId<T extends { readonly id: string }>(
  items: readonly T[],
  id: string,
): readonly T[] {
  return items.filter((item) => item.id !== id);
}

function resolveProjectId(
  projects: readonly Project[],
  preferredId: string | null,
): string | null {
  if (
    preferredId !== null &&
    projects.some((project) => project.id === preferredId)
  ) {
    return preferredId;
  }
  let mostRecent: Project | undefined;
  for (const project of projects) {
    if (
      mostRecent === undefined ||
      project.lastOpenedAtMs > mostRecent.lastOpenedAtMs
    ) {
      mostRecent = project;
    }
  }
  return mostRecent?.id ?? null;
}

function resolveSessionId(
  sessions: readonly Session[],
  selectedProjectId: string | null,
  preferredId: string | null,
): string | null {
  if (selectedProjectId === null || preferredId === null) {
    return null;
  }
  return sessions.some(
    (session) =>
      session.id === preferredId && session.projectId === selectedProjectId,
  )
    ? preferredId
    : null;
}

function actionForEvent(event: EventEnvelope): WorkspaceAction | null {
  switch (event.event) {
    case "project.updated":
      return {
        type: "metadata/project-upserted",
        project: eventDecoders.project(
          eventPayloadField(event.payload, "project", "project updated event"),
        ),
      };
    case "project.removed":
      return {
        type: "metadata/project-removed",
        projectId: eventPayloadId(
          event.payload,
          "projectId",
          "project removed event",
        ),
      };
    case "agent.updated":
      return {
        type: "metadata/agent-upserted",
        agent: eventDecoders.agent(
          eventPayloadField(event.payload, "agent", "agent updated event"),
        ),
      };
    case "agent.removed":
      return {
        type: "metadata/agent-removed",
        agentId: eventPayloadId(
          event.payload,
          "agentId",
          "agent removed event",
        ),
      };
    case "session.created":
    case "session.updated":
      return {
        type: "metadata/session-upserted",
        session: eventDecoders.session(
          eventPayloadField(event.payload, "session", "session changed event"),
        ),
      };
    case "session.deleted":
      return {
        type: "metadata/session-removed",
        sessionId: deletedSessionId(event.payload),
      };
    case "session.status_changed":
      return statusChangedAction(event.payload);
    case "session.exited":
      return sessionExitedAction(event.payload);
    case "worktree.updated":
      return {
        type: "metadata/worktree-upserted",
        worktree: eventDecoders.worktree(
          eventPayloadField(
            event.payload,
            "worktree",
            "worktree updated event",
          ),
        ),
      };
    case "worktree.removed":
      return {
        type: "metadata/worktree-removed",
        worktreeId: eventPayloadId(
          event.payload,
          "worktreeId",
          "worktree removed event",
        ),
      };
    case "daemon.shutting_down":
      return {
        type: "connection/disconnected",
        error: daemonShutdownError(event.payload),
      };
    default:
      // Git status and forward-compatible unknown events do not mutate metadata.
      return null;
  }
}

function eventPayloadField(
  payload: unknown,
  field: string,
  label: string,
): unknown {
  return requireRecord(payload, label)[field];
}

function eventPayloadId(
  payload: unknown,
  field: string,
  label: string,
): string {
  return requireString(requireRecord(payload, label)[field], `${label}.${field}`);
}

function deletedSessionId(payload: unknown): string {
  return eventPayloadId(payload, "sessionId", "deleted session event");
}

function statusChangedAction(
  payload: unknown,
): Extract<
  WorkspaceAction,
  { readonly type: "metadata/session-lifecycle-patched" }
> {
  const record = requireRecord(payload, "session status event");
  return {
    type: "metadata/session-lifecycle-patched",
    sessionId: requireString(
      record.sessionId,
      "session status event.sessionId",
    ),
    status: requireSessionStatus(
      record.status,
      "session status event status",
    ),
    updatedAtMs: requireNumber(
      record.changedAtMs,
      "session status event.changedAtMs",
    ),
    reasonCode:
      record.reasonCode === undefined || record.reasonCode === null
        ? undefined
        : requireString(record.reasonCode, "session status event.reasonCode"),
    setsExitCode: false,
  };
}

function sessionExitedAction(
  payload: unknown,
): Extract<
  WorkspaceAction,
  { readonly type: "metadata/session-lifecycle-patched" }
> {
  const record = requireRecord(payload, "session exited event");
  const exitCode =
    record.exitCode === undefined || record.exitCode === null
      ? undefined
      : requireNumber(record.exitCode, "session exited event.exitCode");
  if (exitCode !== undefined && !Number.isInteger(exitCode)) {
    throw new IpcContractError("session exited event.exitCode must be an integer");
  }
  return {
    type: "metadata/session-lifecycle-patched",
    sessionId: requireString(
      record.sessionId,
      "session exited event.sessionId",
    ),
    status: requireSessionStatus(
      record.status,
      "session exited event status",
    ),
    updatedAtMs: requireNumber(
      record.exitedAtMs,
      "session exited event.exitedAtMs",
    ),
    setsExitCode: true,
    exitCode,
  };
}

function requireSessionStatus(
  value: unknown,
  label: string,
): SessionStatus {
  const status = requireString(value, label);
  switch (status) {
    case "starting":
    case "running":
    case "idle":
    case "exited":
    case "failed":
    case "unknown":
      return status;
    default:
      return "unknown";
  }
}

function daemonShutdownError(payload: unknown): ApiErrorData {
  const record = requireRecord(payload, "daemon shutting down event");
  const reasonCode = requireString(
    record.reasonCode,
    "daemon shutting down event.reasonCode",
  );
  const activeSessionCount = requireNumber(
    record.activeSessionCount,
    "daemon shutting down event.activeSessionCount",
  );
  return {
    code: "daemon_shutting_down",
    message: "The local daemon is shutting down.",
    action: "Retry after the local daemon has restarted.",
    details: { reasonCode, activeSessionCount },
  };
}

function toErrorData(error: unknown): ApiErrorData {
  const normalized = toIpcError(error);
  return {
    code: normalized.code,
    message: normalized.message,
    action: normalized.action,
    details: normalized.details,
  };
}

function isFatalConnectionError(error: ApiErrorData): boolean {
  return (
    error.code === "unsupported_protocol_version" ||
    error.code === "unsupported_schema_version" ||
    error.code === "incompatible_schema_version" ||
    error.code === "invalid_ipc_payload"
  );
}

function isDisconnectedError(error: ApiErrorData): boolean {
  return (
    error.code === "daemon_unavailable" ||
    error.code === "daemon_disconnected" ||
    error.code === "connection_closed" ||
    error.code === "daemon_shutting_down"
  );
}

function getConnectionHello(
  connection: DaemonConnection,
): HelloResponse | null {
  return "hello" in connection && connection.hello !== undefined
    ? connection.hello
    : null;
}

function reconcileOverlay(
  overlay: WorkspaceOverlay | null,
  snapshot: StateSnapshot,
): WorkspaceOverlay | null {
  if (overlay === null) {
    return null;
  }
  switch (overlay.kind) {
    case "new-session":
    case "rename-project":
    case "remove-project":
      return snapshot.projects.some(
        (project) => project.id === overlay.projectId,
      )
        ? overlay
        : null;
    case "rename-session":
    case "stop-session":
    case "delete-session":
    case "git-status":
      return snapshot.sessions.some(
        (session) => session.id === overlay.sessionId,
      )
        ? overlay
        : null;
    case "remove-worktree":
      return snapshot.worktrees.some(
        (worktree) => worktree.id === overlay.worktreeId,
      )
        ? overlay
        : null;
    case "add-project":
    case "command-palette":
      return overlay;
  }
}

function referencesProject(
  overlay: WorkspaceOverlay | null,
  projectId: string,
): boolean {
  return (
    overlay?.kind === "new-session" ||
    overlay?.kind === "rename-project" ||
    overlay?.kind === "remove-project"
  )
    ? overlay.projectId === projectId
    : false;
}

function referencesSession(
  overlay: WorkspaceOverlay | null,
  sessionId: string,
): boolean {
  return (
    overlay?.kind === "rename-session" ||
    overlay?.kind === "stop-session" ||
    overlay?.kind === "delete-session" ||
    overlay?.kind === "git-status"
  )
    ? overlay.sessionId === sessionId
    : false;
}
