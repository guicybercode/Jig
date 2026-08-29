import { useLayoutEffect, useMemo, useRef, useState } from "react";

import type {
  AgentRecord,
  CreateCustomAgentInput,
  CreateSessionInput,
  Project,
  Session,
} from "../../../ipc/types";
import { Icon } from "../../components/Icon";
import { StatusBadge } from "../../components/StatusBadge";
import {
  createCanvasNode,
  createInitialCanvasDocument,
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
import { isLiveStatus } from "../../utils";

interface CanvasWorkspaceProps extends LiveTerminalTransport {
  readonly isConnected: boolean;
  readonly projects: readonly Project[];
  readonly project?: Project;
  readonly agents: readonly AgentRecord[];
  readonly sessions: readonly Session[];
  readonly onAddProject: () => void;
  readonly onNewSession: () => void;
  readonly onSelectSession: (sessionId: string) => void;
  readonly onCreateCustomAgent: (
    input: CreateCustomAgentInput,
  ) => Promise<AgentRecord>;
  readonly onCreateSession: (input: CreateSessionInput) => Promise<Session>;
  readonly onStartSession: (sessionId: string) => Promise<Session>;
}

const ZOOM_STEP = 0.1;

/** Spatial terminal and notes workspace inspired by the supplied references. */
export function CanvasWorkspace({
  isConnected,
  projects,
  project,
  agents,
  sessions,
  onAddProject,
  onNewSession,
  onSelectSession,
  onCreateCustomAgent,
  onCreateSession,
  onStartSession,
  subscribeTerminal,
  writeTerminal,
  resizeTerminal,
}: CanvasWorkspaceProps) {
  const { state, dispatch, persistenceAvailable } = useCanvasState();
  const viewportRef = useRef<HTMLDivElement>(null);
  const viewportInitializedRef = useRef(false);
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
  const selectedNode = state.nodes.find(
    (node) => node.id === state.selectedNodeId,
  );
  const selectedConnections = selectedNode
    ? state.connections.flatMap((connection) => {
        const otherNodeId =
          connection.sourceNodeId === selectedNode.id
            ? connection.targetNodeId
            : connection.targetNodeId === selectedNode.id
              ? connection.sourceNodeId
              : null;
        const otherNode = state.nodes.find((node) => node.id === otherNodeId);
        return otherNode ? [{ connection, otherNode }] : [];
      })
    : [];

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
    dispatch({
      type: "node/add",
      node: createCanvasNode("note", nextNodePosition()),
    });
  }

  function addTerminal(configuration: CanvasTerminalConfiguration) {
    const node = createTerminalCanvasNode(nextNodePosition(), configuration);
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
      });
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

  function focusNode(node: CanvasNode) {
    const viewport = viewportRef.current;
    dispatch({ type: "node/select", nodeId: node.id });
    setLayersOpen(false);
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
      behavior: "smooth",
    });
  }

  function fitCanvasToItems() {
    const viewport = viewportRef.current;
    if (!viewport || state.nodes.length === 0) {
      return;
    }

    const minimumX = Math.min(...state.nodes.map((node) => node.x));
    const minimumY = Math.min(...state.nodes.map((node) => node.y));
    const maximumX = Math.max(
      ...state.nodes.map((node) => node.x + getCanvasNodeSize(node).width),
    );
    const maximumY = Math.max(
      ...state.nodes.map((node) => node.y + getCanvasNodeSize(node).height),
    );
    const contentWidth = maximumX - minimumX;
    const contentHeight = maximumY - minimumY;
    const viewportWidth = viewport.clientWidth || 960;
    const viewportHeight = viewport.clientHeight || 640;
    const nextZoom = Math.min(
      1,
      Math.max(
        0.5,
        Math.min(
          (viewportWidth - 160) / contentWidth,
          (viewportHeight - 160) / contentHeight,
        ),
      ),
    );

    setZoom(nextZoom);
    viewport.scrollTo({
      left: Math.max(
        0,
        (CANVAS_ORIGIN_X + minimumX + contentWidth / 2) * nextZoom -
          viewportWidth / 2,
      ),
      top: Math.max(
        0,
        (CANVAS_ORIGIN_Y + minimumY + contentHeight / 2) * nextZoom -
          viewportHeight / 2,
      ),
      behavior: "smooth",
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
          {state.nodes.filter((node) => node.kind === "terminal").length}{" "}
          terminals
        </span>
      </div>

      <div className="canvas-toolbar" role="toolbar" aria-label="Canvas tools">
        <button
          className="canvas-tool canvas-tool--active"
          type="button"
          aria-label="Select and move canvas items"
          aria-pressed="true"
        >
          <Icon name="pointer" />
        </button>
        <span className="canvas-toolbar__divider" aria-hidden="true" />
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
            state.connectionSourceId
              ? "canvas-tool canvas-tool--connecting"
              : "canvas-tool"
          }
          type="button"
          aria-label={
            state.connectionSourceId
              ? "Cancel connection"
              : "Connect selected item"
          }
          aria-pressed={state.connectionSourceId !== null}
          disabled={!selectedNode}
          onClick={() => {
            if (state.connectionSourceId) {
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
          aria-label="Delete selected item"
          disabled={!selectedNode}
          onClick={() => {
            if (selectedNode) {
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
          onClick={() =>
            dispatch({
              type: "document/hydrate",
              document: createInitialCanvasDocument(),
            })
          }
        >
          <Icon name="refresh" />
        </button>
      </div>

      <div className="canvas-session-actions">
        {project ? (
          <button
            className="canvas-pill-button"
            type="button"
            disabled={!isConnected}
            onClick={onNewSession}
          >
            <Icon name="plus" /> New session
          </button>
        ) : (
          <button
            className="canvas-pill-button"
            type="button"
            disabled={!isConnected}
            onClick={onAddProject}
          >
            <Icon name="plus" /> Add project
          </button>
        )}
      </div>

      {state.connectionSourceId ? (
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
          if (event.defaultPrevented) {
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
            (event.target as HTMLElement).closest(".canvas-node")
          ) {
            return;
          }
          event.preventDefault();
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
            connections={state.connections}
            nodes={state.nodes}
            selectedNodeId={state.selectedNodeId}
            connectionSourceId={state.connectionSourceId}
          />
          {state.nodes.map((node) => {
            const session =
              node.kind === "terminal"
                ? terminalSessions.get(node.sessionId ?? "")
                : undefined;
            return (
              <CanvasNodeCard
                key={node.id}
                node={node}
                session={session}
                selected={state.selectedNodeId === node.id}
                connectionSource={state.connectionSourceId}
                connectionCount={state.connections.filter(
                  (connection) =>
                    connection.sourceNodeId === node.id ||
                    connection.targetNodeId === node.id,
                ).length}
                onSelect={() =>
                  dispatch({ type: "node/select", nodeId: node.id })
                }
                onConnect={() => {
                  if (
                    state.connectionSourceId &&
                    state.connectionSourceId !== node.id
                  ) {
                    dispatch({
                      type: "connection/complete",
                      targetNodeId: node.id,
                    });
                  } else if (state.connectionSourceId === node.id) {
                    dispatch({ type: "connection/cancel" });
                  } else {
                    dispatch({ type: "connection/start", nodeId: node.id });
                  }
                }}
                onCancelConnection={() =>
                  dispatch({ type: "connection/cancel" })
                }
                onDelete={() =>
                  dispatch({ type: "node/delete", nodeId: node.id })
                }
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
                onOpenSession={() => session && onSelectSession(session.id)}
                onStartTerminal={() =>
                  node.kind === "terminal" && launchTerminal(node, session)
                }
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
            <span>{state.nodes.length}</span>
          </header>
          <ul>
            {state.nodes.map((node) => (
              <li key={node.id}>
                <button type="button" onClick={() => focusNode(node)}>
                  <Icon name={node.kind === "terminal" ? "terminal" : "note"} />
                  <span>{node.title}</span>
                  <small>
                    {state.connections.filter(
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

      <div className="canvas-view-controls" aria-label="Canvas view controls">
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

      <div className="canvas-zoom" aria-label="Canvas zoom controls">
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
  readonly session?: Session;
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
  readonly onOpenSession: () => void;
  readonly onStartTerminal: () => void;
  readonly terminalPending: boolean;
  readonly terminalError?: string;
  readonly terminalTransport: LiveTerminalTransport;
}

function CanvasNodeCard({
  node,
  session,
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
  onOpenSession,
  onStartTerminal,
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
      aria-selected={selected}
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
      <header
        className="canvas-node__header"
        aria-label={`Move ${node.title}`}
        onPointerDown={(event) => {
          if ((event.target as HTMLElement).closest("button")) {
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
            aria-label={`Delete ${node.title}`}
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
            onOpenSession={onOpenSession}
            onStart={onStartTerminal}
            pending={terminalPending}
            error={terminalError}
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
                const size = keyboardResize(event.key, node, step);
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
                  width: node.width,
                  height: node.height,
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
  onOpenSession,
  onStart,
  pending,
  error,
  transport,
}: {
  readonly node: TerminalCanvasNode;
  readonly session?: Session;
  readonly onOpenSession: () => void;
  readonly onStart: () => void;
  readonly pending: boolean;
  readonly error?: string;
  readonly transport: LiveTerminalTransport;
}) {
  const live = session ? isLiveStatus(session.status) : false;
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
              (session
                ? "This terminal is stopped. Start it to attach a fresh live PTY."
                : `${node.executable ?? "A login shell"} is ready to start in this project.`)}
          </p>
          <div className="canvas-terminal__body-actions">
            <button
              type="button"
              disabled={pending}
              aria-busy={pending}
              onClick={onStart}
            >
              {pending ? "Starting…" : error ? "Retry terminal" : "Start terminal"}
            </button>
            {session ? (
              <button type="button" onClick={onOpenSession}>
                Session details
              </button>
            ) : null}
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
