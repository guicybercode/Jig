import {
  PROTOCOL_V1,
  formatApiError,
  incompatibleProtocolError,
  toApiError,
  type DaemonEvent,
  type IpcClient,
  type ProjectId,
  type SessionId,
} from "../../ipc";
import { classifyConnectFailure } from "./connection";
import { setDialogOpen } from "./dialogs";
import {
  createOptimisticProject,
  emptySnapshot,
  replaceProject,
  withoutProject,
  withProject,
} from "./mutations";
import { appendNotification, createNotification, removeNotification } from "./notifications";
import { applyDaemonEvent, mergeOptimisticProjects } from "./queries";
import {
  focusSession,
  reconcileSelection,
  selectProject,
  toggleVisibleSession,
} from "./selection";
import type { WorkspaceStore } from "./store";
import type { TerminalRegistry } from "./terminal-registry";
import {
  EMPTY_PROJECTS,
  INITIAL_GIT,
  type WorkspaceActions,
  type WorkspaceState,
} from "./types";

export type WorkspaceController = WorkspaceActions & {
  start(): () => void;
};

type ControllerOptions = {
  readonly client: IpcClient;
  readonly store: WorkspaceStore;
  readonly terminals: TerminalRegistry;
};

function setPending(
  state: WorkspaceState,
  patch: Partial<WorkspaceState["pending"]>,
): WorkspaceState {
  return {
    ...state,
    pending: { ...state.pending, ...patch },
  };
}

/** Owns snapshot generations, mutations, and event routing for one client. */
export function createWorkspaceController({
  client,
  store,
  terminals,
}: ControllerOptions): WorkspaceController {
  let connectGeneration = 0;
  let snapshotGeneration = 0;
  let gitGeneration = 0;
  let notificationSeq = 0;

  function notify(kind: "info" | "warning" | "error", message: string): void {
    notificationSeq += 1;
    const notification = createNotification(
      `notice-${notificationSeq}`,
      kind,
      message,
    );
    store.update((state) => ({
      ...state,
      notifications: appendNotification(state.notifications, notification),
    }));
  }

  function applySnapshot(
    snapshot: WorkspaceState["snapshot"],
    generation: number,
    extra?: Partial<WorkspaceState>,
  ): void {
    if (generation !== snapshotGeneration) {
      return;
    }
    store.update((state) => {
      if (generation !== snapshotGeneration) {
        return state;
      }
      if (!snapshot) {
        return {
          ...state,
          snapshot: null,
          snapshotGeneration: generation,
          ...extra,
        };
      }
      const merged = mergeOptimisticProjects(
        snapshot,
        extra && "optimisticProjects" in extra
          ? (extra.optimisticProjects ?? EMPTY_PROJECTS)
          : state.optimisticProjects,
      );
      return {
        ...state,
        snapshot: merged,
        snapshotGeneration: generation,
        selection: reconcileSelection(state.selection, merged),
        ...extra,
      };
    });
  }

  async function refreshSnapshot(connectId: number): Promise<void> {
    const generation = (snapshotGeneration += 1);
    const snapshot = await client.request("state.snapshot", {});
    if (connectId !== connectGeneration || generation !== snapshotGeneration) {
      return;
    }
    applySnapshot(snapshot, generation);
  }

  async function connect(): Promise<void> {
    const connectId = (connectGeneration += 1);
    snapshotGeneration += 1;
    gitGeneration += 1;
    store.update((state) => ({
      ...state,
      connection: {
        ...state.connection,
        phase: state.connection.phase === "ready" ? "ready" : "loading",
        message: null,
      },
    }));
    try {
      const hello = await client.request("system.hello", {});
      if (connectId !== connectGeneration) {
        return;
      }
      if (hello.protocolVersion !== PROTOCOL_V1) {
        store.update((state) => ({
          ...state,
          connection: {
            phase: "incompatible",
            message: formatApiError(incompatibleProtocolError),
            protocolVersion: hello.protocolVersion,
            daemonVersion: hello.daemonVersion,
            instanceId: hello.instanceId,
          },
          snapshot: null,
        }));
        return;
      }
      await refreshSnapshot(connectId);
      if (connectId !== connectGeneration) {
        return;
      }
      store.update((state) => ({
        ...state,
        connection: {
          phase: "ready",
          message: null,
          protocolVersion: hello.protocolVersion,
          daemonVersion: hello.daemonVersion,
          instanceId: hello.instanceId,
        },
      }));
      try {
        const detected = await client.request("agent.detect", {});
        if (connectId !== connectGeneration) {
          return;
        }
        store.update((state) => ({
          ...state,
          detections: detected.detections,
        }));
      } catch (error) {
        if (connectId !== connectGeneration) {
          return;
        }
        notify("warning", formatApiError(error));
      }
    } catch (error) {
      if (connectId !== connectGeneration) {
        return;
      }
      const apiError = toApiError(error);
      store.update((state) => ({
        ...state,
        connection: {
          phase: classifyConnectFailure(apiError),
          message: formatApiError(apiError),
          protocolVersion: null,
          daemonVersion: null,
          instanceId: null,
        },
      }));
    }
  }

  function handleEvent(event: DaemonEvent): void {
    if (
      event.event === "session.output" ||
      event.event === "session.output_gap" ||
      event.event === "session.replay_complete"
    ) {
      if (event.event === "session.output") {
        terminals.writeOutput(event.payload);
      } else if (event.event === "session.output_gap") {
        terminals.markOutputGap(event.payload);
      } else {
        terminals.markReplayComplete(
          event.payload.sessionId,
          event.payload.outputSequence,
        );
      }
      return;
    }

    if (event.event === "daemon.shutting_down") {
      store.update((state) => ({
        ...state,
        connection: {
          ...state.connection,
          phase: "loading",
          message: "Reconnecting…",
        },
      }));
      void connect();
      return;
    }

    store.update((state) => {
      if (!state.snapshot) {
        return state;
      }
      const applied = applyDaemonEvent(state.snapshot, event, state.git);
      if (applied.kind === "snapshot") {
        const snapshot = mergeOptimisticProjects(
          applied.snapshot,
          state.optimisticProjects,
        );
        return {
          ...state,
          snapshot,
          selection: reconcileSelection(state.selection, snapshot),
        };
      }
      if (applied.kind === "git") {
        return { ...state, git: applied.git };
      }
      return state;
    });
  }

  const actions: WorkspaceController = {
    start() {
      const unsubscribe = client.subscribe(handleEvent);
      void connect();
      return () => {
        connectGeneration += 1;
        snapshotGeneration += 1;
        gitGeneration += 1;
        unsubscribe();
      };
    },
    async refresh() {
      const connectId = connectGeneration;
      try {
        await refreshSnapshot(connectId);
      } catch (error) {
        if (connectId !== connectGeneration) {
          return;
        }
        notify("error", formatApiError(error));
      }
    },
    reconnect() {
      return connect();
    },
    async addProject(path, name) {
      const optimistic = createOptimisticProject(path, name);
      const previous = store.getState();
      store.update((state) => {
        const base = state.snapshot ?? emptySnapshot();
        const snapshot = withProject(base, optimistic);
        return {
          ...state,
          snapshot,
          optimisticProjects: [...state.optimisticProjects, optimistic],
          selection: selectProject(snapshot, optimistic.id),
          pending: { ...state.pending, creatingProject: true },
        };
      });
      try {
        const created = await client.request("project.add", { path, name });
        store.update((state) => {
          const base = state.snapshot ?? emptySnapshot();
          const snapshot = replaceProject(base, optimistic.id, created);
          const optimisticProjects = state.optimisticProjects.filter(
            (project) => project.id !== optimistic.id,
          );
          const selected =
            state.selection.projectId === optimistic.id
              ? selectProject(snapshot, created.id)
              : reconcileSelection(state.selection, snapshot);
          return {
            ...state,
            snapshot,
            optimisticProjects,
            selection: selected,
            pending: { ...state.pending, creatingProject: false },
          };
        });
      } catch (error) {
        store.update(() => ({
          ...previous,
          pending: { ...previous.pending, creatingProject: false },
          optimisticProjects: previous.optimisticProjects,
          notifications: appendNotification(
            previous.notifications,
            createNotification(
              `notice-${(notificationSeq += 1)}`,
              "error",
              formatApiError(error),
            ),
          ),
        }));
        throw error;
      }
    },
    async removeProject(projectId: ProjectId) {
      await client.request("project.remove", { projectId });
      store.update((state) => {
        if (!state.snapshot) {
          return state;
        }
        const snapshot = withoutProject(state.snapshot, projectId);
        return {
          ...state,
          snapshot,
          selection: reconcileSelection(state.selection, snapshot),
        };
      });
    },
    selectProject(projectId) {
      store.update((state) => ({
        ...state,
        selection: selectProject(state.snapshot, projectId),
        git: INITIAL_GIT,
      }));
    },
    async createCustomAgent(input) {
      store.update((state) => setPending(state, { creatingAgent: true }));
      try {
        const agent = await client.request("agent.custom.create", {
          displayName: input.displayName,
          command: {
            executable: input.executable,
            args: [...input.args],
            env: {},
          },
        });
        store.update((state) => {
          if (!state.snapshot) {
            return setPending(state, { creatingAgent: false });
          }
          return {
            ...state,
            pending: { ...state.pending, creatingAgent: false },
            dialogs: setDialogOpen(state.dialogs, "customAgent", false),
            snapshot: {
              ...state.snapshot,
              agents: [...state.snapshot.agents, agent],
            },
          };
        });
      } catch (error) {
        store.update((state) => setPending(state, { creatingAgent: false }));
        throw error;
      }
    },
    async createSession(input) {
      const projectId = store.getState().selection.projectId;
      if (!projectId) {
        throw new Error("Select a project before creating a session.");
      }
      store.update((state) => setPending(state, { creatingSession: true }));
      try {
        const session = await client.request("session.create", {
          projectId,
          name: input.name,
          agentId: input.agentId,
          isolation: input.isolation,
        });
        store.update((state) => {
          if (!state.snapshot) {
            return setPending(state, { creatingSession: false });
          }
          const snapshot = {
            ...state.snapshot,
            sessions: [...state.snapshot.sessions, session],
          };
          return {
            ...state,
            snapshot,
            pending: { ...state.pending, creatingSession: false },
            dialogs: setDialogOpen(state.dialogs, "newSession", false),
            selection: focusSession(state.selection, session.id),
          };
        });
      } catch (error) {
        store.update((state) => setPending(state, { creatingSession: false }));
        throw error;
      }
    },
    async stopSession(sessionId: SessionId) {
      const session = await client.request("session.stop", { sessionId });
      store.update((state) => {
        if (!state.snapshot) {
          return state;
        }
        return {
          ...state,
          snapshot: {
            ...state.snapshot,
            sessions: state.snapshot.sessions.map((item) =>
              item.id === session.id ? session : item,
            ),
          },
        };
      });
    },
    async deleteSession(sessionId: SessionId) {
      await client.request("session.delete", { sessionId });
      store.update((state) => {
        if (!state.snapshot) {
          return state;
        }
        const snapshot = {
          ...state.snapshot,
          sessions: state.snapshot.sessions.filter(
            (session) => session.id !== sessionId,
          ),
        };
        return {
          ...state,
          snapshot,
          selection: reconcileSelection(state.selection, snapshot),
        };
      });
    },
    focusSession(sessionId) {
      store.update((state) => ({
        ...state,
        selection: focusSession(state.selection, sessionId),
      }));
    },
    toggleVisible(sessionId) {
      store.update((state) => ({
        ...state,
        selection: toggleVisibleSession(state.selection, sessionId),
      }));
    },
    async writeSession(sessionId, base64) {
      await client.request("session.write", { sessionId, base64 });
    },
    async resizeSession(sessionId, columns, rows) {
      await client.request("session.resize", { sessionId, columns, rows });
    },
    async subscribeSession(sessionId, cursor) {
      await client.request("session.subscribe", { sessionId, cursor });
    },
    async inspectGit(target) {
      const generation = (gitGeneration += 1);
      store.update((state) => ({
        ...state,
        git: { ...state.git, loading: true, error: null },
      }));
      try {
        const status = await client.request("git.status", { target });
        if (generation !== gitGeneration) {
          return;
        }
        const diff = await client.request("git.diff", { target });
        if (generation !== gitGeneration) {
          return;
        }
        store.update((state) => ({
          ...state,
          git: { status, diff, error: null, loading: false },
        }));
      } catch (error) {
        if (generation !== gitGeneration) {
          return;
        }
        store.update((state) => ({
          ...state,
          git: {
            ...state.git,
            loading: false,
            error: formatApiError(error),
          },
        }));
      }
    },
    prepareRemoveWorktree(worktreeId) {
      return client.request("worktree.prepare_remove", { worktreeId });
    },
    async removeWorktree(worktreeId, confirmationToken) {
      await client.request("worktree.remove", {
        worktreeId,
        confirmationToken,
      });
      store.update((state) => {
        if (!state.snapshot) {
          return state;
        }
        const snapshot = {
          ...state.snapshot,
          worktrees: state.snapshot.worktrees.filter(
            (worktree) => worktree.id !== worktreeId,
          ),
        };
        return {
          ...state,
          snapshot,
          selection: reconcileSelection(state.selection, snapshot),
        };
      });
    },
    openDialog(name) {
      store.update((state) => ({
        ...state,
        dialogs: setDialogOpen(state.dialogs, name, true),
      }));
    },
    closeDialog(name) {
      store.update((state) => ({
        ...state,
        dialogs: setDialogOpen(state.dialogs, name, false),
      }));
    },
    dismissNotification(id) {
      store.update((state) => ({
        ...state,
        notifications: removeNotification(state.notifications, id),
      }));
    },
  };

  return actions;
}
