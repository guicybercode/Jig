import {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";

import type {
  AgentRecord,
  ApiErrorData,
  CreateCustomAgentInput,
  CreateSessionInput,
  Project,
  Session,
  Worktree,
} from "../../../ipc/types";
import { Icon } from "../../components/Icon";
import { StatusBadge } from "../../components/StatusBadge";
import {
  createCanvasNode,
  createInitialCanvasDocument,
  createSessionTerminalCanvasNode,
  createTerminalCanvasNode,
  getCanvasNodeSize,
  type CanvasTerminalConfiguration,
  type CanvasNode,
  type NoteCanvasNode,
  type TerminalCanvasNode,
} from "./canvas-state";
import { CanvasConnections } from "./CanvasConnections";
import {
  CANVAS_ORIGIN_X,
  CANVAS_ORIGIN_Y,
  INITIAL_VIEW_CENTER,
  toStagePoint,
} from "./canvas-geometry";
import { NewCanvasTerminalDialog } from "./NewCanvasTerminalDialog";
import { useCanvasState } from "./useCanvasState";
import {
  LiveTerminal,
  type LiveTerminalTransport,
} from "../terminal/LiveTerminal";
import { errorData, isLiveStatus } from "../../utils";

export interface CanvasSessionFocusRequest {
  readonly sessionId: string;
  readonly revision: number;
}

interface CanvasWorkspaceProps extends LiveTerminalTransport {
  readonly isCompact?: boolean;
  readonly isConnected: boolean;
  readonly projects: readonly Project[];
  readonly project?: Project;
  readonly agents: readonly AgentRecord[];
  readonly sessions: readonly Session[];
  readonly worktrees: readonly Worktree[];
  readonly selectedSessionId?: string;
  readonly sessionFocusRevision: number;
  readonly onSelectSession: (sessionId: string | null) => void;
  readonly onCreateCustomAgent: (
    input: CreateCustomAgentInput,
  ) => Promise<AgentRecord>;
  readonly onCreateSession: (input: CreateSessionInput) => Promise<Session>;
  readonly onStartSession: (sessionId: string) => Promise<Session>;
  readonly onRestartSession: (sessionId: string) => Promise<Session>;
  readonly onRenameSession: (sessionId: string) => void;
  readonly onStopSession: (sessionId: string) => void;
  readonly onDeleteSession: (sessionId: string) => void;
  readonly onRemoveWorktree: (worktreeId: string) => void;
  readonly onGitStatus: (sessionId: string) => void;
  readonly onOpenPath: (path: string) => Promise<void>;
}

const ZOOM_STEP = 0.1;
const COMPACT_TERMINAL_GUTTER_PX = 48;

/** Spatial terminal and notes workspace inspired by the supplied references. */
export function CanvasWorkspace({
  isCompact = false,
  isConnected,
  projects,
  project,
  agents,
  sessions,
  worktrees,
  selectedSessionId,
  sessionFocusRevision,
  onSelectSession,
  onCreateCustomAgent,
  onCreateSession,
  onStartSession,
  onRestartSession,
  onRenameSession,
  onStopSession,
  onDeleteSession,
  onRemoveWorktree,
  onGitStatus,
  onOpenPath,
  subscribeTerminal,
  writeTerminal,
  resizeTerminal,
}: CanvasWorkspaceProps) {
  const { state, dispatch, persistenceAvailable } = useCanvasState();
  const viewportRef = useRef<HTMLDivElement>(null);
  const viewportWidth = useElementWidth(viewportRef);
  const viewportInitializedRef = useRef(false);
  const nodeElementsRef = useRef(new Map<string, HTMLElement>());
  const handledFocusRequestRef = useRef<CanvasSessionFocusRequest | null>(null);
  const panRef = useRef<{
    readonly pointerId: number;
    readonly clientX: number;
    readonly clientY: number;
    readonly scrollLeft: number;
    readonly scrollTop: number;
  } | null>(null);
  const [layersOpen, setLayersOpen] = useState(false);
  const [terminalDialogOpen, setTerminalDialogOpen] = useState(false);
  const [pendingTerminals, setPendingTerminals] = useState<ReadonlySet<string>>(
    () => new Set(),
  );
  const [terminalErrors, setTerminalErrors] = useState<
    Readonly<Record<string, string>>
  >({});
  const terminalSessions = useMemo(
    () => new Map(sessions.map((session) => [session.id, session])),
    [sessions],
  );
  const projectSessions = useMemo(
    () =>
      project
        ? sessions.filter((session) => session.projectId === project.id)
        : [],
    [project, sessions],
  );
  const hiddenSessionIds = useMemo(
    () => new Set(state.hiddenSessionIds),
    [state.hiddenSessionIds],
  );
  const storedVisibleNodes = useMemo(
    () =>
      state.nodes.filter((node) => {
        if (
          node.kind === "terminal" &&
          node.sessionId &&
          hiddenSessionIds.has(node.sessionId)
        ) {
          return false;
        }
        const session =
          node.kind === "terminal" && node.sessionId
            ? terminalSessions.get(node.sessionId)
            : undefined;
        const nodeProjectId = session?.projectId ?? node.projectId;
        return !nodeProjectId || nodeProjectId === project?.id;
      }),
    [hiddenSessionIds, project?.id, state.nodes, terminalSessions],
  );
  const visibleNodes = useMemo(
    () =>
      effectiveCanvasNodes(
        storedVisibleNodes,
        isCompact,
        viewportWidth,
        state.zoom,
      ),
    [isCompact, state.zoom, storedVisibleNodes, viewportWidth],
  );
  const storedVisibleNodesById = useMemo(
    () => new Map(storedVisibleNodes.map((node) => [node.id, node])),
    [storedVisibleNodes],
  );
  const visibleNodeIds = useMemo(
    () => new Set(visibleNodes.map((node) => node.id)),
    [visibleNodes],
  );
  const visibleConnections = useMemo(
    () =>
      state.connections.filter(
        (connection) =>
          visibleNodeIds.has(connection.sourceNodeId) &&
          visibleNodeIds.has(connection.targetNodeId),
      ),
    [state.connections, visibleNodeIds],
  );
  const visibleConnectionSourceId =
    state.connectionSourceId && visibleNodeIds.has(state.connectionSourceId)
      ? state.connectionSourceId
      : null;
  const selectedNode = visibleNodes.find(
    (node) => node.id === state.selectedNodeId,
  );
  const selectedConnections = selectedNode
    ? visibleConnections.flatMap((connection) => {
        const otherNodeId =
          connection.sourceNodeId === selectedNode.id
            ? connection.targetNodeId
            : connection.targetNodeId === selectedNode.id
              ? connection.sourceNodeId
              : null;
        const otherNode = visibleNodes.find((node) => node.id === otherNodeId);
        return otherNode ? [{ connection, otherNode }] : [];
      })
    : [];
  const sessionCanvasTopologyKey = useMemo(
    () =>
      JSON.stringify({
        attachedSessionIds: state.nodes.flatMap((node) =>
          node.kind === "terminal" && node.sessionId ? [node.sessionId] : [],
        ),
        hiddenSessionIds: state.hiddenSessionIds,
      }),
    [state.hiddenSessionIds, state.nodes],
  );
  const focusNode = useCallback(
    (node: CanvasNode) => {
      const viewport = viewportRef.current;
      onSelectSession(
        node.kind === "terminal" && node.sessionId ? node.sessionId : null,
      );
      dispatch({ type: "node/select", nodeId: node.id });
      setLayersOpen(false);
      nodeElementsRef.current.get(node.id)?.focus({ preventScroll: true });
      if (!viewport) {
        return;
      }

      const size = getCanvasNodeSize(node);
      viewport.scrollTo({
        left: Math.max(
          0,
          (CANVAS_ORIGIN_X + node.x + size.width / 2) * state.zoom -
            viewport.clientWidth / 2,
        ),
        top: Math.max(
          0,
          (CANVAS_ORIGIN_Y + node.y + size.height / 2) * state.zoom -
            viewport.clientHeight / 2,
        ),
        behavior: canvasScrollBehavior(),
      });
    },
    [dispatch, onSelectSession, state.zoom],
  );

  useLayoutEffect(() => {
    dispatch({
      type: "sessions/reconcile",
      knownSessionIds: sessions.map((session) => session.id),
      sessionNodes: projectSessions.map((session, index) =>
        createSessionTerminalCanvasNode(
          reconciledSessionPosition(index),
          session,
        ),
      ),
    });
  }, [dispatch, projectSessions, sessionCanvasTopologyKey, sessions]);

  useLayoutEffect(() => {
    if (
      state.selectedNodeId !== null &&
      !visibleNodeIds.has(state.selectedNodeId)
    ) {
      dispatch({ type: "node/select", nodeId: null });
    }
    if (
      state.connectionSourceId !== null &&
      !visibleNodeIds.has(state.connectionSourceId)
    ) {
      dispatch({ type: "connection/cancel" });
    }
  }, [
    dispatch,
    state.connectionSourceId,
    state.selectedNodeId,
    visibleNodeIds,
  ]);

  useLayoutEffect(() => {
    if (!selectedSessionId) {
      handledFocusRequestRef.current = null;
      return;
    }
    const request: CanvasSessionFocusRequest = {
      sessionId: selectedSessionId,
      revision: sessionFocusRevision,
    };
    const handledRequest = handledFocusRequestRef.current;
    if (
      handledRequest?.sessionId === request.sessionId &&
      handledRequest.revision === request.revision
    ) {
      return;
    }

    const session = terminalSessions.get(request.sessionId);
    if (!session || session.projectId !== project?.id) {
      return;
    }
    const existingNode = visibleNodes.find(
      (node) =>
        node.kind === "terminal" && node.sessionId === request.sessionId,
    );
    if (!existingNode || hiddenSessionIds.has(request.sessionId)) {
      const projectSessionIndex = Math.max(
        0,
        projectSessions.findIndex((candidate) => candidate.id === session.id),
      );
      dispatch({
        type: "session/reveal",
        node: createSessionTerminalCanvasNode(
          reconciledSessionPosition(projectSessionIndex),
          session,
        ),
      });
      return;
    }
    if (!nodeElementsRef.current.has(existingNode.id)) {
      return;
    }

    handledFocusRequestRef.current = request;
    focusNode(existingNode);
  }, [
    dispatch,
    focusNode,
    hiddenSessionIds,
    project?.id,
    projectSessions,
    selectedSessionId,
    sessionFocusRevision,
    terminalSessions,
    visibleNodes,
  ]);

  useLayoutEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport || viewportInitializedRef.current) {
      return;
    }
    viewportInitializedRef.current = true;
    viewport.scrollLeft = Math.max(
      0,
      (CANVAS_ORIGIN_X + INITIAL_VIEW_CENTER.x) * state.zoom -
        viewport.clientWidth / 2,
    );
    viewport.scrollTop = Math.max(
      0,
      (CANVAS_ORIGIN_Y + INITIAL_VIEW_CENTER.y) * state.zoom -
        viewport.clientHeight / 2,
    );
  }, [state.zoom]);

  function nextNodePosition() {
    const viewport = viewportRef.current;
    const offset = (state.nodes.length % 6) * 32;
    return {
      x:
        ((viewport?.scrollLeft ?? 0) + 260) / state.zoom -
        CANVAS_ORIGIN_X +
        offset,
      y:
        ((viewport?.scrollTop ?? 0) + 150) / state.zoom -
        CANVAS_ORIGIN_Y +
        offset,
    };
  }

  function addNote() {
    const node = createCanvasNode("note", nextNodePosition());
    onSelectSession(null);
    dispatch({
      type: "node/add",
      node: project ? { ...node, projectId: project.id } : node,
    });
  }

  function addTerminal(configuration: CanvasTerminalConfiguration) {
    const terminal = createTerminalCanvasNode(
      nextNodePosition(),
      configuration,
    );
    const node = project
      ? { ...terminal, projectId: project.id }
      : terminal;
    onSelectSession(null);
    dispatch({
      type: "node/add",
      node,
    });
    setTerminalDialogOpen(false);
    if (project && isConnected) {
      void launchTerminal(node);
    }
  }

  async function launchTerminal(node: TerminalCanvasNode, session?: Session) {
    if (!project) {
      setTerminalErrors((current) => ({
        ...current,
        [node.id]: "Select a project before starting this terminal.",
      }));
      return;
    }
    setPendingTerminals((current) => new Set(current).add(node.id));
    setTerminalErrors((current) => {
      const next = { ...current };
      delete next[node.id];
      return next;
    });
    try {
      if (session) {
        await onStartSession(session.id);
        return;
      }
      const agent = await resolveTerminalAgent(
        node,
        agents,
        onCreateCustomAgent,
      );
      const created = await onCreateSession({
        projectId: project.id,
        name: node.title,
        agentId: agent.id,
        isolation: "current",
        relativeDirectory: relativeWorkingDirectory(project, node.workingDirectory),
      });
      dispatch({
        type: "terminal/attach",
        nodeId: node.id,
        sessionId: created.id,
        projectId: created.projectId,
      });
      onSelectSession(created.id);
      await onStartSession(created.id);
    } catch (error) {
      setTerminalErrors((current) => ({
        ...current,
        [node.id]: terminalErrorMessage(error),
      }));
    } finally {
      setPendingTerminals((current) => {
        const next = new Set(current);
        next.delete(node.id);
        return next;
      });
    }
  }

  function setZoom(zoom: number) {
    dispatch({ type: "zoom/set", zoom: Number(zoom.toFixed(2)) });
  }

  function fitCanvasToItems() {
    const viewport = viewportRef.current;
    if (!viewport || storedVisibleNodes.length === 0) {
      return;
    }

    const storedBounds = canvasNodeBounds(storedVisibleNodes);
    const measuredViewportWidth = viewport.clientWidth || viewportWidth || 960;
    const viewportHeight = viewport.clientHeight || 640;
    const nextZoom = Number(
      Math.min(
        1,
        Math.max(
          0.5,
          Math.min(
            (measuredViewportWidth - 160) / storedBounds.width,
            (viewportHeight - 160) / storedBounds.height,
          ),
        ),
      ).toFixed(2),
    );
    const fittedBounds = canvasNodeBounds(
      effectiveCanvasNodes(
        storedVisibleNodes,
        isCompact,
        measuredViewportWidth,
        nextZoom,
      ),
    );

    setZoom(nextZoom);
    viewport.scrollTo({
      left: Math.max(
        0,
        (CANVAS_ORIGIN_X + fittedBounds.minimumX + fittedBounds.width / 2) *
          nextZoom -
          measuredViewportWidth / 2,
      ),
      top: Math.max(
        0,
        (CANVAS_ORIGIN_Y + fittedBounds.minimumY + fittedBounds.height / 2) *
          nextZoom -
          viewportHeight / 2,
      ),
      behavior: canvasScrollBehavior(),
    });
  }

  function resetCanvasLayout() {
    onSelectSession(null);
    if (!project) {
      dispatch({
        type: "document/hydrate",
        document: createInitialCanvasDocument(),
      });
      return;
    }

    const projectNodeIds = new Set(
      state.nodes
        .filter((node) => node.projectId === project.id)
        .map((node) => node.id),
    );
    const projectSessionIds = new Set(
      projectSessions.map((session) => session.id),
    );
    dispatch({
      type: "document/hydrate",
      document: {
        version: 1,
        nodes: state.nodes.filter((node) => !projectNodeIds.has(node.id)),
        connections: state.connections.filter(
          (connection) =>
            !projectNodeIds.has(connection.sourceNodeId) &&
            !projectNodeIds.has(connection.targetNodeId),
        ),
        zoom: state.zoom,
        hiddenSessionIds: state.hiddenSessionIds.filter(
          (sessionId) => !projectSessionIds.has(sessionId),
        ),
      },
    });
  }

  return (
    <main
      id="workspace"
      className="canvas-workspace"
      tabIndex={-1}
      aria-labelledby="canvas-workspace-title"
    >
      <div className="canvas-context" aria-live="polite">
        <span className="canvas-context__eyebrow">Workspace</span>
        <h1 id="canvas-workspace-title">
          {project?.name ?? "My Workspace"}
        </h1>
        <span className="canvas-context__meta">
          {projects.length} {projects.length === 1 ? "project" : "projects"}
          <span aria-hidden="true"> · </span>
          {visibleNodes.filter((node) => node.kind === "terminal").length}{" "}
          terminals
        </span>
      </div>

      <div className="canvas-toolbar" role="toolbar" aria-label="Canvas tools">
        <button
          className="canvas-tool"
          type="button"
          aria-label="Add terminal card"
          onClick={() => setTerminalDialogOpen(true)}
        >
          <Icon name="terminal" />
        </button>
        <button
          className="canvas-tool"
          type="button"
          aria-label="Add note"
          onClick={addNote}
        >
          <Icon name="note" />
        </button>
        <button
          className={
            visibleConnectionSourceId
              ? "canvas-tool canvas-tool--connecting"
              : "canvas-tool"
          }
          type="button"
          aria-label={
            visibleConnectionSourceId
              ? "Cancel connection"
              : "Connect selected item"
          }
          aria-pressed={visibleConnectionSourceId !== null}
          disabled={!selectedNode}
          onClick={() => {
            if (visibleConnectionSourceId) {
              dispatch({ type: "connection/cancel" });
            } else if (selectedNode) {
              dispatch({ type: "connection/start", nodeId: selectedNode.id });
            }
          }}
        >
          <Icon name="link" />
        </button>
        <span className="canvas-toolbar__divider" aria-hidden="true" />
        <button
          className="canvas-tool"
          type="button"
          aria-label="Remove selected item from canvas"
          title="Remove the selected card without deleting session metadata"
          disabled={!selectedNode}
          onClick={() => {
            if (selectedNode) {
              onSelectSession(null);
              dispatch({ type: "node/delete", nodeId: selectedNode.id });
            }
          }}
        >
          <Icon name="trash" />
        </button>
        <button
          className="canvas-tool"
          type="button"
          aria-label="Reset canvas layout"
          title={
            project ? "Reset this project's canvas layout" : "Reset canvas layout"
          }
          onClick={resetCanvasLayout}
        >
          <Icon name="refresh" />
        </button>
      </div>

      {visibleConnectionSourceId ? (
        <div className="canvas-connect-notice" role="status">
          <Icon name="link" />
          Choose another terminal or note to finish the connection.
          <button
            type="button"
            onClick={() => dispatch({ type: "connection/cancel" })}
          >
            Cancel
          </button>
        </div>
      ) : null}

      <div
        ref={viewportRef}
        className="canvas-viewport"
        tabIndex={0}
        aria-label="Pannable canvas"
        aria-keyshortcuts="ArrowUp ArrowDown ArrowLeft ArrowRight"
        onKeyDown={(event) => {
          if (event.defaultPrevented || event.currentTarget !== event.target) {
            return;
          }
          const step = event.altKey ? 16 : 80;
          const movement = keyboardMovement(event.key, step);
          if (movement) {
            event.preventDefault();
            event.currentTarget.scrollLeft += movement.x;
            event.currentTarget.scrollTop += movement.y;
          }
        }}
        onPointerDown={(event) => {
          if (
            event.button !== 0 ||
            (event.target instanceof Element &&
              event.target.closest(".canvas-node"))
          ) {
            return;
          }
          event.preventDefault();
          onSelectSession(null);
          dispatch({ type: "node/select", nodeId: null });
          panRef.current = {
            pointerId: event.pointerId,
            clientX: event.clientX,
            clientY: event.clientY,
            scrollLeft: event.currentTarget.scrollLeft,
            scrollTop: event.currentTarget.scrollTop,
          };
          event.currentTarget.setPointerCapture?.(event.pointerId);
        }}
        onPointerMove={(event) => {
          const pan = panRef.current;
          if (!pan || pan.pointerId !== event.pointerId) {
            return;
          }
          event.currentTarget.scrollLeft =
            pan.scrollLeft - (event.clientX - pan.clientX);
          event.currentTarget.scrollTop =
            pan.scrollTop - (event.clientY - pan.clientY);
        }}
        onPointerUp={(event) => {
          if (panRef.current?.pointerId === event.pointerId) {
            panRef.current = null;
            event.currentTarget.releasePointerCapture?.(event.pointerId);
          }
        }}
        onPointerCancel={() => {
          panRef.current = null;
        }}
        onWheel={(event) => {
          if (event.shiftKey && event.deltaX === 0) {
            event.preventDefault();
            event.currentTarget.scrollLeft += event.deltaY;
          }
        }}
      >
        <div
          className="canvas-stage"
          style={{ transform: `scale(${state.zoom})` }}
        >
          <CanvasConnections
            connections={visibleConnections}
            nodes={visibleNodes}
            selectedNodeId={state.selectedNodeId}
            connectionSourceId={visibleConnectionSourceId}
          />
          {visibleNodes.map((node) => {
            const storedNode = storedVisibleNodesById.get(node.id);
            const session =
              node.kind === "terminal"
                ? terminalSessions.get(node.sessionId ?? "")
                : undefined;
            const worktree = session
              ? worktrees.find((candidate) =>
                  session.worktreeId
                    ? candidate.id === session.worktreeId
                    : candidate.sessionId === session.id,
                )
              : undefined;
            return (
              <CanvasNodeCard
                key={node.id}
                node={node}
                storedTerminalSize={
                  storedNode?.kind === "terminal"
                    ? { width: storedNode.width, height: storedNode.height }
                    : undefined
                }
                session={session}
                worktree={worktree}
                isConnected={isConnected}
                selected={state.selectedNodeId === node.id}
                connectionSource={visibleConnectionSourceId}
                connectionCount={visibleConnections.filter(
                  (connection) =>
                    connection.sourceNodeId === node.id ||
                    connection.targetNodeId === node.id,
                ).length}
                onSelect={() => {
                  dispatch({ type: "node/select", nodeId: node.id });
                  onSelectSession(session?.id ?? null);
                }}
                onConnect={() => {
                  if (
                    visibleConnectionSourceId &&
                    visibleConnectionSourceId !== node.id
                  ) {
                    dispatch({
                      type: "connection/complete",
                      targetNodeId: node.id,
                    });
                  } else if (visibleConnectionSourceId === node.id) {
                    dispatch({ type: "connection/cancel" });
                  } else {
                    dispatch({ type: "connection/start", nodeId: node.id });
                  }
                }}
                onCancelConnection={() =>
                  dispatch({ type: "connection/cancel" })
                }
                onDelete={() => {
                  onSelectSession(null);
                  dispatch({ type: "node/delete", nodeId: node.id });
                }}
                zoom={state.zoom}
                onMove={(position) =>
                  dispatch({ type: "node/move", nodeId: node.id, position })
                }
                onResize={(size) =>
                  dispatch({ type: "terminal/resize", nodeId: node.id, size })
                }
                onNoteChange={(text) =>
                  dispatch({ type: "note/update", nodeId: node.id, text })
                }
                onStartTerminal={() =>
                  node.kind === "terminal" && launchTerminal(node, session)
                }
                onStartSession={onStartSession}
                onRestartSession={onRestartSession}
                onRenameSession={onRenameSession}
                onStopSession={onStopSession}
                onDeleteSession={onDeleteSession}
                onRemoveWorktree={onRemoveWorktree}
                onGitStatus={onGitStatus}
                onOpenPath={onOpenPath}
                elementRef={(element) => {
                  if (element) {
                    nodeElementsRef.current.set(node.id, element);
                  } else {
                    nodeElementsRef.current.delete(node.id);
                  }
                }}
                terminalPending={pendingTerminals.has(node.id)}
                terminalError={terminalErrors[node.id]}
                terminalTransport={{
                  subscribeTerminal,
                  writeTerminal,
                  resizeTerminal,
                }}
              />
            );
          })}
        </div>
      </div>

      {layersOpen ? (
        <section
          id="canvas-layers-panel"
          className="canvas-layers-panel"
          aria-labelledby="canvas-layers-title"
        >
          <header>
            <div>
              <span>Workspace</span>
              <h2 id="canvas-layers-title">Canvas items</h2>
            </div>
            <span>{visibleNodes.length}</span>
          </header>
          <ul>
            {visibleNodes.map((node) => (
              <li key={node.id}>
                <button type="button" onClick={() => focusNode(node)}>
                  <Icon name={node.kind === "terminal" ? "terminal" : "note"} />
                  <span>{node.title}</span>
                  <small>
                    {visibleConnections.filter(
                      (connection) =>
                        connection.sourceNodeId === node.id ||
                        connection.targetNodeId === node.id,
                    ).length} connections
                  </small>
                </button>
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      {selectedNode && selectedConnections.length > 0 && !layersOpen ? (
        <section
          className="canvas-connections-panel"
          aria-label={`Connections for ${selectedNode.title}`}
        >
          <header>
            <div>
              <span>Selected item</span>
              <h2>{selectedNode.title}</h2>
            </div>
            <span>{selectedConnections.length}</span>
          </header>
          <ul>
            {selectedConnections.map(({ connection, otherNode }) => (
              <li key={connection.id}>
                <Icon name="link" />
                <span>{otherNode.title}</span>
                <button
                  type="button"
                  aria-label={`Remove connection to ${otherNode.title}`}
                  onClick={() =>
                    dispatch({
                      type: "connection/delete",
                      connectionId: connection.id,
                    })
                  }
                >
                  <Icon name="close" />
                </button>
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      <div
        className="canvas-view-controls"
        role="group"
        aria-label="Canvas view controls"
      >
        <button
          type="button"
          aria-label="Show canvas items"
          aria-expanded={layersOpen}
          aria-controls="canvas-layers-panel"
          onClick={() => setLayersOpen((open) => !open)}
        >
          <Icon name="layers" />
        </button>
        <button
          type="button"
          aria-label="Fit canvas to items"
          onClick={fitCanvasToItems}
        >
          <Icon name="map" />
        </button>
      </div>

      <div
        className="canvas-zoom"
        role="group"
        aria-label="Canvas zoom controls"
      >
        <button
          type="button"
          aria-label="Zoom out"
          disabled={state.zoom <= 0.5}
          onClick={() => setZoom(state.zoom - ZOOM_STEP)}
        >
          <Icon name="zoom-out" />
        </button>
        <output aria-live="polite">{Math.round(state.zoom * 100)}%</output>
        <button
          type="button"
          aria-label="Zoom in"
          disabled={state.zoom >= 1.5}
          onClick={() => setZoom(state.zoom + ZOOM_STEP)}
        >
          <Icon name="zoom-in" />
        </button>
      </div>

      <p className="canvas-save-status" role="status">
        {persistenceAvailable
          ? "Canvas saved locally"
          : "Canvas persistence unavailable"}
      </p>

      {terminalDialogOpen ? (
        <NewCanvasTerminalDialog
          defaultWorkingDirectory={project?.path ?? "~"}
          onClose={() => setTerminalDialogOpen(false)}
          onCreate={addTerminal}
        />
      ) : null}
    </main>
  );
}

interface CanvasNodeCardProps {
  readonly node: CanvasNode;
  readonly storedTerminalSize?: {
    readonly width: number;
    readonly height: number;
  };
  readonly session?: Session;
  readonly worktree?: Worktree;
  readonly isConnected: boolean;
  readonly selected: boolean;
  readonly connectionSource: string | null;
  readonly connectionCount: number;
  readonly zoom: number;
  readonly onSelect: () => void;
  readonly onConnect: () => void;
  readonly onCancelConnection: () => void;
  readonly onDelete: () => void;
  readonly onMove: (position: { readonly x: number; readonly y: number }) => void;
  readonly onResize: (size: {
    readonly width: number;
    readonly height: number;
  }) => void;
  readonly onNoteChange: (text: string) => void;
  readonly onStartTerminal: () => void;
  readonly onStartSession: (sessionId: string) => Promise<Session>;
  readonly onRestartSession: (sessionId: string) => Promise<Session>;
  readonly onRenameSession: (sessionId: string) => void;
  readonly onStopSession: (sessionId: string) => void;
  readonly onDeleteSession: (sessionId: string) => void;
  readonly onRemoveWorktree: (worktreeId: string) => void;
  readonly onGitStatus: (sessionId: string) => void;
  readonly onOpenPath: (path: string) => Promise<void>;
  readonly elementRef: (element: HTMLElement | null) => void;
  readonly terminalPending: boolean;
  readonly terminalError?: string;
  readonly terminalTransport: LiveTerminalTransport;
}

function CanvasNodeCard({
  node,
  storedTerminalSize,
  session,
  worktree,
  isConnected,
  selected,
  connectionSource,
  connectionCount,
  zoom,
  onSelect,
  onConnect,
  onCancelConnection,
  onDelete,
  onMove,
  onResize,
  onNoteChange,
  onStartTerminal,
  onStartSession,
  onRestartSession,
  onRenameSession,
  onStopSession,
  onDeleteSession,
  onRemoveWorktree,
  onGitStatus,
  onOpenPath,
  elementRef,
  terminalPending,
  terminalError,
  terminalTransport,
}: CanvasNodeCardProps) {
  const dragRef = useRef<{
    readonly pointerId: number;
    readonly clientX: number;
    readonly clientY: number;
    readonly nodeX: number;
    readonly nodeY: number;
  } | null>(null);
  const resizeRef = useRef<{
    readonly pointerId: number;
    readonly clientX: number;
    readonly clientY: number;
    readonly width: number;
    readonly height: number;
  } | null>(null);
  const isConnectionTarget =
    connectionSource !== null && connectionSource !== node.id;
  const classes = [
    "canvas-node",
    `canvas-node--${node.kind}`,
    selected ? "canvas-node--selected" : "",
    isConnectionTarget ? "canvas-node--connection-target" : "",
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <article
      ref={elementRef}
      className={classes}
      style={{
        transform: `translate(${toStagePoint(node).x}px, ${toStagePoint(node).y}px)`,
        ...(node.kind === "terminal"
          ? { width: `${node.width}px`, height: `${node.height}px` }
          : {}),
      }}
      tabIndex={0}
      aria-label={`${node.title}, ${node.kind} canvas item`}
      data-canvas-node-id={node.id}
      data-canvas-session-id={session?.id}
      data-selected={selected ? "true" : undefined}
      aria-describedby={selected ? `${node.id}-selection-status` : undefined}
      aria-keyshortcuts="ArrowUp ArrowDown ArrowLeft ArrowRight"
      onFocus={onSelect}
      onKeyDown={(event) => {
        if (event.currentTarget !== event.target) {
          return;
        }
        const step = event.altKey ? 1 : 8;
        const movement = keyboardMovement(event.key, step);
        if (movement) {
          event.preventDefault();
          onMove({ x: node.x + movement.x, y: node.y + movement.y });
        } else if (event.key === "Escape" && connectionSource) {
          event.preventDefault();
          onCancelConnection();
        }
      }}
      onPointerDown={(event) => {
        event.stopPropagation();
        onSelect();
      }}
    >
      {selected ? (
        <span id={`${node.id}-selection-status`} className="visually-hidden">
          Selected canvas item
        </span>
      ) : null}
      <header
        className="canvas-node__header"
        aria-label={`Move ${node.title}`}
        onPointerDown={(event) => {
          if (
            event.target instanceof Element &&
            event.target.closest("button, summary")
          ) {
            return;
          }
          event.preventDefault();
          event.stopPropagation();
          onSelect();
          dragRef.current = {
            pointerId: event.pointerId,
            clientX: event.clientX,
            clientY: event.clientY,
            nodeX: node.x,
            nodeY: node.y,
          };
          event.currentTarget.setPointerCapture?.(event.pointerId);
        }}
        onPointerMove={(event) => {
          const drag = dragRef.current;
          if (!drag || drag.pointerId !== event.pointerId) {
            return;
          }
          onMove({
            x: drag.nodeX + (event.clientX - drag.clientX) / zoom,
            y: drag.nodeY + (event.clientY - drag.clientY) / zoom,
          });
        }}
        onPointerUp={(event) => {
          if (dragRef.current?.pointerId === event.pointerId) {
            dragRef.current = null;
            event.currentTarget.releasePointerCapture?.(event.pointerId);
          }
        }}
        onPointerCancel={() => {
          dragRef.current = null;
        }}
      >
        <div className="canvas-node__identity">
          {node.kind === "terminal" ? (
            <span className="canvas-window-controls" aria-hidden="true">
              <i />
              <i />
              <i />
            </span>
          ) : (
            <span className="canvas-node__kind-icon" aria-hidden="true">
              <Icon name="note" />
            </span>
          )}
          <strong>{node.title}</strong>
          {connectionCount > 0 ? (
            <span className="canvas-node__connection-count">
              <Icon name="link" /> {connectionCount}
            </span>
          ) : null}
        </div>
        <div className="canvas-node__actions">
          {selected ? (
            <span className="canvas-node__selected-label">Selected</span>
          ) : null}
          {session ? (
            <CanvasSessionActions
              session={session}
              worktree={worktree}
              isConnected={isConnected}
              onStartSession={onStartSession}
              onRestartSession={onRestartSession}
              onRenameSession={onRenameSession}
              onStopSession={onStopSession}
              onDeleteSession={onDeleteSession}
              onRemoveWorktree={onRemoveWorktree}
              onGitStatus={onGitStatus}
              onOpenPath={onOpenPath}
            />
          ) : null}
          <button
            type="button"
            aria-label={
              isConnectionTarget
                ? `Connect to ${node.title}`
                : connectionSource === node.id
                  ? `Cancel connection from ${node.title}`
                  : `Start connection from ${node.title}`
            }
            aria-pressed={connectionSource === node.id}
            onClick={(event) => {
              event.stopPropagation();
              onConnect();
            }}
          >
            <Icon name="link" />
          </button>
          <button
            type="button"
            aria-label={`Remove ${node.title} from canvas`}
            title="Remove this card from the canvas without deleting its session"
            onClick={(event) => {
              event.stopPropagation();
              onDelete();
            }}
          >
            <Icon name="trash" />
          </button>
        </div>
      </header>
      {node.kind === "terminal" ? (
        <>
          <TerminalNodeBody
            node={node}
            session={session}
            onStart={onStartTerminal}
            pending={terminalPending}
            error={terminalError}
            startDisabledReason={terminalStartDisabledReason(
              session,
              worktree,
              isConnected,
            )}
            transport={terminalTransport}
          />
          {selected ? (
            <button
              className="canvas-node__resize-handle"
              type="button"
              aria-label={`Resize ${node.title}`}
              aria-keyshortcuts="ArrowUp ArrowDown ArrowLeft ArrowRight"
              title="Drag to resize. Arrow keys resize; hold Alt for 1 px."
              onKeyDown={(event) => {
                const step = event.altKey ? 1 : 16;
                const size = keyboardResize(
                  event.key,
                  {
                    ...node,
                    width: storedTerminalSize?.width ?? node.width,
                    height: storedTerminalSize?.height ?? node.height,
                  },
                  step,
                );
                if (size) {
                  event.preventDefault();
                  onResize(size);
                }
              }}
              onPointerDown={(event) => {
                event.preventDefault();
                event.stopPropagation();
                resizeRef.current = {
                  pointerId: event.pointerId,
                  clientX: event.clientX,
                  clientY: event.clientY,
                  width: storedTerminalSize?.width ?? node.width,
                  height: storedTerminalSize?.height ?? node.height,
                };
                event.currentTarget.setPointerCapture?.(event.pointerId);
              }}
              onPointerMove={(event) => {
                const resize = resizeRef.current;
                if (!resize || resize.pointerId !== event.pointerId) {
                  return;
                }
                onResize({
                  width:
                    resize.width + (event.clientX - resize.clientX) / zoom,
                  height:
                    resize.height + (event.clientY - resize.clientY) / zoom,
                });
              }}
              onPointerUp={(event) => {
                if (resizeRef.current?.pointerId === event.pointerId) {
                  resizeRef.current = null;
                  event.currentTarget.releasePointerCapture?.(event.pointerId);
                }
              }}
              onPointerCancel={() => {
                resizeRef.current = null;
              }}
            >
              <span aria-hidden="true" />
            </button>
          ) : null}
        </>
      ) : (
        <NoteNodeBody node={node} onChange={onNoteChange} />
      )}
    </article>
  );
}

function useElementWidth(elementRef: {
  readonly current: HTMLElement | null;
}): number {
  const subscribe = useCallback(
    (onStoreChange: () => void) => {
      const element = elementRef.current;
      if (!element || typeof window === "undefined") {
        return () => undefined;
      }
      const observer =
        typeof ResizeObserver === "function"
          ? new ResizeObserver(() => onStoreChange())
          : undefined;
      observer?.observe(element);
      window.addEventListener("resize", onStoreChange);
      return () => {
        observer?.disconnect();
        window.removeEventListener("resize", onStoreChange);
      };
    },
    [elementRef],
  );
  const getSnapshot = useCallback(
    () => elementRef.current?.clientWidth ?? 0,
    [elementRef],
  );
  return useSyncExternalStore(subscribe, getSnapshot, () => 0);
}

function effectiveCanvasNodes(
  nodes: readonly CanvasNode[],
  isCompact: boolean,
  viewportWidth: number,
  zoom: number,
): readonly CanvasNode[] {
  if (!isCompact || viewportWidth <= 0) {
    return nodes;
  }
  const maximumTerminalWidth = Math.max(
    1,
    (viewportWidth - COMPACT_TERMINAL_GUTTER_PX) / zoom,
  );
  return nodes.map((node) =>
    node.kind === "terminal" && node.width > maximumTerminalWidth
      ? { ...node, width: maximumTerminalWidth }
      : node,
  );
}

function canvasNodeBounds(nodes: readonly CanvasNode[]) {
  const minimumX = Math.min(...nodes.map((node) => node.x));
  const minimumY = Math.min(...nodes.map((node) => node.y));
  const maximumX = Math.max(
    ...nodes.map((node) => node.x + getCanvasNodeSize(node).width),
  );
  const maximumY = Math.max(
    ...nodes.map((node) => node.y + getCanvasNodeSize(node).height),
  );
  return {
    minimumX,
    minimumY,
    width: maximumX - minimumX,
    height: maximumY - minimumY,
  };
}

interface CanvasSessionActionsProps {
  readonly session: Session;
  readonly worktree?: Worktree;
  readonly isConnected: boolean;
  readonly onStartSession: (sessionId: string) => Promise<Session>;
  readonly onRestartSession: (sessionId: string) => Promise<Session>;
  readonly onRenameSession: (sessionId: string) => void;
  readonly onStopSession: (sessionId: string) => void;
  readonly onDeleteSession: (sessionId: string) => void;
  readonly onRemoveWorktree: (worktreeId: string) => void;
  readonly onGitStatus: (sessionId: string) => void;
  readonly onOpenPath: (path: string) => Promise<void>;
}

function CanvasSessionActions({
  session,
  worktree,
  isConnected,
  onStartSession,
  onRestartSession,
  onRenameSession,
  onStopSession,
  onDeleteSession,
  onRemoveWorktree,
  onGitStatus,
  onOpenPath,
}: CanvasSessionActionsProps) {
  const actionsContainerRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const [actionsOpen, setActionsOpen] = useState(false);
  const [pendingAction, setPendingAction] = useState<string>();
  const [actionError, setActionError] = useState<ApiErrorData>();
  const live = isLiveStatus(session.status);
  const managedWorktreeUnavailable = Boolean(session.worktreeId && !worktree);
  const availablePath = worktree?.path ?? session.worktreePath ?? session.cwd;
  const path = managedWorktreeUnavailable
    ? undefined
    : availablePath || undefined;
  const unavailableWorktreeReason = managedWorktreeUnavailable
    ? "The managed worktree is no longer available."
    : undefined;
  const disconnectedReason = !isConnected
    ? "Connect the local daemon first."
    : undefined;

  useEffect(() => {
    if (!actionsOpen) {
      return;
    }

    function closeForOutsideInteraction(event: Event) {
      const target = event.target;
      if (
        target instanceof Node &&
        !actionsContainerRef.current?.contains(target)
      ) {
        setActionsOpen(false);
      }
    }

    document.addEventListener("pointerdown", closeForOutsideInteraction, true);
    document.addEventListener("focusin", closeForOutsideInteraction);
    return () => {
      document.removeEventListener(
        "pointerdown",
        closeForOutsideInteraction,
        true,
      );
      document.removeEventListener("focusin", closeForOutsideInteraction);
    };
  }, [actionsOpen]);

  function closeActionDisclosure() {
    triggerRef.current?.focus();
    setActionsOpen(false);
  }

  async function runDirectAction(
    name: string,
    action: () => Promise<unknown>,
  ) {
    if (pendingAction) {
      return;
    }
    setPendingAction(name);
    setActionError(undefined);
    try {
      await action();
      closeActionDisclosure();
    } catch (error) {
      setActionError(errorData(error));
    } finally {
      setPendingAction(undefined);
    }
  }

  function runOverlayAction(action: () => void) {
    if (pendingAction) {
      return;
    }
    setActionError(undefined);
    try {
      action();
      closeActionDisclosure();
    } catch (error) {
      setActionError(errorData(error));
    }
  }

  return (
    <div
      ref={actionsContainerRef}
      className="canvas-node__session-actions"
      onPointerDown={(event) => event.stopPropagation()}
      onKeyDown={(event) => {
        if (actionsOpen && event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          closeActionDisclosure();
        }
      }}
    >
      <button
        ref={triggerRef}
        className="canvas-node__session-actions-trigger"
        type="button"
        aria-label={`Session actions for ${session.name}`}
        aria-expanded={actionsOpen}
        aria-controls={`canvas-session-actions-${session.id}`}
        title="Session actions"
        onClick={() => setActionsOpen((open) => !open)}
      >
        <Icon name="more" />
      </button>
      {actionsOpen ? (
        <div
          id={`canvas-session-actions-${session.id}`}
          className="canvas-node__session-actions-panel"
          role="group"
          aria-label={`Actions for ${session.name}`}
          aria-busy={pendingAction !== undefined}
        >
          <strong>{session.name}</strong>
          <StatusBadge status={session.status} compact />
          <SessionActionButton
            label="Start session"
            icon="play"
            pending={pendingAction === "start"}
            disabledReason={
              pendingAction
                ? "Another session action is in progress."
                : live
                  ? "This session is already running."
                  : disconnectedReason ?? unavailableWorktreeReason
            }
            onClick={() =>
              void runDirectAction("start", () => onStartSession(session.id))
            }
          />
          <SessionActionButton
            label="Restart session"
            icon="refresh"
            pending={pendingAction === "restart"}
            disabledReason={
              pendingAction
                ? "Another session action is in progress."
                : disconnectedReason ?? unavailableWorktreeReason
            }
            onClick={() =>
              void runDirectAction("restart", () =>
                onRestartSession(session.id),
              )
            }
          />
          <SessionActionButton
            label="Rename session"
            icon="pencil"
            disabledReason={
              pendingAction
                ? "Another session action is in progress."
                : disconnectedReason
            }
            onClick={() => runOverlayAction(() => onRenameSession(session.id))}
          />
          <SessionActionButton
            label="Stop process"
            icon="stop"
            disabledReason={
              pendingAction
                ? "Another session action is in progress."
                : disconnectedReason ??
                  (!live ? "This session has no live process." : undefined)
            }
            onClick={() => runOverlayAction(() => onStopSession(session.id))}
          />
          <SessionActionButton
            label="Git status"
            icon="branch"
            disabledReason={
              pendingAction
                ? "Another session action is in progress."
                : disconnectedReason ?? unavailableWorktreeReason
            }
            onClick={() => runOverlayAction(() => onGitStatus(session.id))}
          />
          <SessionActionButton
            label="Open working directory"
            icon="folder"
            pending={pendingAction === "open-path"}
            disabledReason={
              pendingAction
                ? "Another session action is in progress."
                : path
                  ? undefined
                  : managedWorktreeUnavailable
                    ? "The managed worktree path is no longer available."
                    : "This session has no working directory."
            }
            onClick={() => {
              if (path) {
                void runDirectAction("open-path", () => onOpenPath(path));
              }
            }}
          />
          <SessionActionButton
            label="Delete session metadata"
            icon="trash"
            disabledReason={
              pendingAction
                ? "Another session action is in progress."
                : disconnectedReason ??
                  (live
                    ? "Stop the process before deleting the session."
                    : undefined)
            }
            onClick={() =>
              runOverlayAction(() => onDeleteSession(session.id))
            }
          />
          <SessionActionButton
            label="Remove worktree"
            icon="worktree"
            disabledReason={
              pendingAction
                ? "Another session action is in progress."
                : disconnectedReason ??
                  (live
                    ? "Stop the process before removing its worktree."
                    : worktree
                      ? undefined
                      : "This session has no available managed worktree.")
            }
            onClick={() => {
              if (worktree) {
                runOverlayAction(() => onRemoveWorktree(worktree.id));
              }
            }}
          />
          {actionError ? (
            <div className="canvas-node__session-action-error" role="alert">
              <strong>{actionError.message}</strong>
              {actionError.action ? <span>{actionError.action}</span> : null}
              <button
                type="button"
                aria-label="Dismiss session action error"
                onClick={() => setActionError(undefined)}
              >
                <Icon name="close" />
              </button>
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

function SessionActionButton({
  label,
  icon,
  pending = false,
  disabledReason,
  onClick,
}: {
  readonly label: string;
  readonly icon: Parameters<typeof Icon>[0]["name"];
  readonly pending?: boolean;
  readonly disabledReason?: string;
  readonly onClick: () => void;
}) {
  const disabledReasonId = useId();
  const disabled = disabledReason !== undefined;
  return (
    <>
      <button
        type="button"
        aria-disabled={disabled ? true : undefined}
        title={disabledReason}
        aria-busy={pending}
        aria-describedby={disabledReason ? disabledReasonId : undefined}
        onClick={() => {
          if (!disabled) {
            onClick();
          }
        }}
      >
        <Icon name={icon} />
        <span>{pending ? `${label}…` : label}</span>
      </button>
      {disabledReason ? (
        <span id={disabledReasonId} className="visually-hidden">
          {disabledReason}
        </span>
      ) : null}
    </>
  );
}

function keyboardMovement(
  key: string,
  step: number,
): { readonly x: number; readonly y: number } | null {
  switch (key) {
    case "ArrowUp":
      return { x: 0, y: -step };
    case "ArrowDown":
      return { x: 0, y: step };
    case "ArrowLeft":
      return { x: -step, y: 0 };
    case "ArrowRight":
      return { x: step, y: 0 };
    default:
      return null;
  }
}

function reconciledSessionPosition(index: number) {
  return {
    x: 170 + (index % 3) * 464,
    y: 720 + Math.floor(index / 3) * 288,
  };
}

function canvasScrollBehavior(): ScrollBehavior {
  return globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches
    ? "auto"
    : "smooth";
}

function terminalStartDisabledReason(
  session: Session | undefined,
  worktree: Worktree | undefined,
  isConnected: boolean,
): string | undefined {
  if (!isConnected) {
    return "Connect the local daemon first.";
  }
  if (session?.worktreeId && !worktree) {
    return "The managed worktree is no longer available.";
  }
  return undefined;
}

function keyboardResize(
  key: string,
  node: TerminalCanvasNode,
  step: number,
): { readonly width: number; readonly height: number } | null {
  switch (key) {
    case "ArrowUp":
      return { width: node.width, height: node.height - step };
    case "ArrowDown":
      return { width: node.width, height: node.height + step };
    case "ArrowLeft":
      return { width: node.width - step, height: node.height };
    case "ArrowRight":
      return { width: node.width + step, height: node.height };
    default:
      return null;
  }
}

function TerminalNodeBody({
  node,
  session,
  onStart,
  pending,
  error,
  startDisabledReason,
  transport,
}: {
  readonly node: TerminalCanvasNode;
  readonly session?: Session;
  readonly onStart: () => void;
  readonly pending: boolean;
  readonly error?: string;
  readonly startDisabledReason?: string;
  readonly transport: LiveTerminalTransport;
}) {
  const live = session ? isLiveStatus(session.status) : false;
  const startDisabled = pending || startDisabledReason !== undefined;
  return (
    <div
      className="canvas-terminal"
      role="region"
      aria-label={`Terminal surface for ${node.title}`}
      data-terminal-root="true"
    >
      <div className="canvas-terminal__status">
        {session ? (
          <>
            <StatusBadge status={session.status} compact />
            <span className="mono">{session.branch ?? session.cwd}</span>
          </>
        ) : (
          <span className="canvas-terminal__draft">
            <Icon name="terminal" /> {node.executable ?? "Shell"} draft
          </span>
        )}
      </div>
      {session && live ? (
        <div className="canvas-terminal__body canvas-terminal__body--live">
          <LiveTerminal session={session} {...transport} />
        </div>
      ) : (
        <div className="canvas-terminal__body">
          <Icon name="terminal" />
          <p>
            {error ??
              startDisabledReason ??
              (session
                ? "This terminal is stopped. Start it to attach a fresh live PTY."
                : `${node.executable ?? "A login shell"} is ready to start in this project.`)}
          </p>
          <div className="canvas-terminal__body-actions">
            <button
              type="button"
              aria-disabled={startDisabled ? true : undefined}
              title={startDisabledReason}
              aria-busy={pending}
              onClick={() => {
                if (!startDisabled) {
                  onStart();
                }
              }}
            >
              {pending ? "Starting…" : error ? "Retry terminal" : "Start terminal"}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

async function resolveTerminalAgent(
  node: TerminalCanvasNode,
  agents: readonly AgentRecord[],
  createCustomAgent: (
    input: CreateCustomAgentInput,
  ) => Promise<AgentRecord>,
): Promise<AgentRecord> {
  if (node.preset === "custom") {
    if (!node.executable) {
      throw new Error("Choose an executable before starting this terminal.");
    }
    return createCustomAgent({
      displayName: node.title,
      command: { executable: node.executable, args: [], env: {} },
    });
  }
  const executable = node.preset === "shell" ? undefined : node.executable;
  const match = agents.find((agent) => {
    if (!agent.enabled) {
      return false;
    }
    if (node.preset === "shell") {
      return agent.displayName.toLowerCase() === "shell";
    }
    const commandName = agent.command.executable.split(/[\\/]/).pop();
    return commandName?.toLowerCase() === executable?.toLowerCase();
  });
  if (!match) {
    throw new Error(
      `${node.preset === "shell" ? "Shell" : node.title} is not available in Jig yet.`,
    );
  }
  return match;
}

function relativeWorkingDirectory(
  project: Project,
  selectedDirectory?: string,
): string | undefined {
  if (!selectedDirectory || selectedDirectory === "~") {
    return undefined;
  }
  const root = (project.repositoryRoot ?? project.path).replace(/\/$/, "");
  const selected = selectedDirectory.replace(/\/$/, "");
  if (selected === root || selected === project.path.replace(/\/$/, "")) {
    return undefined;
  }
  if (!selected.startsWith(`${root}/`)) {
    throw new Error("Choose a working directory inside the selected project.");
  }
  return selected.slice(root.length + 1);
}

function terminalErrorMessage(error: unknown): string {
  if (error instanceof Error && error.message) {
    return error.message;
  }
  return "Jig could not start this terminal. Try again.";
}

function NoteNodeBody({
  node,
  onChange,
}: {
  readonly node: NoteCanvasNode;
  readonly onChange: (text: string) => void;
}) {
  return (
    <label className="canvas-note">
      <span className="visually-hidden">{node.title}</span>
      <textarea
        value={node.text}
        maxLength={50_000}
        aria-label={`${node.title} content`}
        placeholder="Write a note…"
        onChange={(event) => onChange(event.currentTarget.value)}
        onPointerDown={(event) => event.stopPropagation()}
      />
    </label>
  );
}
