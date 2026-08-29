import { useMemo, useRef, useState } from "react";

import type { Project, Session } from "../../../ipc/types";
import { Icon } from "../../components/Icon";
import { StatusBadge } from "../../components/StatusBadge";
import {
  createCanvasNode,
  createInitialCanvasDocument,
  type CanvasNode,
  type NoteCanvasNode,
  type TerminalCanvasNode,
} from "./canvas-state";
import { useCanvasState } from "./useCanvasState";

interface CanvasWorkspaceProps {
  readonly isConnected: boolean;
  readonly projects: readonly Project[];
  readonly project?: Project;
  readonly sessions: readonly Session[];
  readonly onAddProject: () => void;
  readonly onNewSession: () => void;
  readonly onSelectSession: (sessionId: string) => void;
}

const ZOOM_STEP = 0.1;
const NODE_SIZE = {
  terminal: { width: 432, height: 256 },
  note: { width: 288, height: 288 },
} as const;

/** Spatial terminal and notes workspace inspired by the supplied references. */
export function CanvasWorkspace({
  isConnected,
  projects,
  project,
  sessions,
  onAddProject,
  onNewSession,
  onSelectSession,
}: CanvasWorkspaceProps) {
  const { state, dispatch, persistenceAvailable } = useCanvasState();
  const viewportRef = useRef<HTMLDivElement>(null);
  const [layersOpen, setLayersOpen] = useState(false);
  const terminalSessions = useMemo(
    () => new Map(sessions.map((session) => [session.id, session])),
    [sessions],
  );
  const selectedNode = state.nodes.find(
    (node) => node.id === state.selectedNodeId,
  );

  function addNode(kind: "terminal" | "note") {
    const viewport = viewportRef.current;
    const offset = (state.nodes.length % 6) * 32;
    const position = {
      x: ((viewport?.scrollLeft ?? 0) + 260) / state.zoom + offset,
      y: ((viewport?.scrollTop ?? 0) + 150) / state.zoom + offset,
    };
    dispatch({ type: "node/add", node: createCanvasNode(kind, position) });
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

    const size = NODE_SIZE[node.kind];
    viewport.scrollTo({
      left: Math.max(
        0,
        (node.x + size.width / 2) * state.zoom - viewport.clientWidth / 2,
      ),
      top: Math.max(
        0,
        (node.y + size.height / 2) * state.zoom - viewport.clientHeight / 2,
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
      ...state.nodes.map((node) => node.x + NODE_SIZE[node.kind].width),
    );
    const maximumY = Math.max(
      ...state.nodes.map((node) => node.y + NODE_SIZE[node.kind].height),
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
        (minimumX + contentWidth / 2) * nextZoom - viewportWidth / 2,
      ),
      top: Math.max(
        0,
        (minimumY + contentHeight / 2) * nextZoom - viewportHeight / 2,
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
          onClick={() => addNode("terminal")}
        >
          <Icon name="terminal" />
        </button>
        <button
          className="canvas-tool"
          type="button"
          aria-label="Add note"
          onClick={() => addNode("note")}
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
        onPointerDown={(event) => {
          if (event.currentTarget === event.target) {
            dispatch({ type: "node/select", nodeId: null });
          }
        }}
      >
        <div
          className="canvas-stage"
          style={{ transform: `scale(${state.zoom})` }}
        >
          {state.nodes.map((node, index) => {
            const fallbackSession =
              node.kind === "terminal" ? sessions[index] : undefined;
            const session =
              node.kind === "terminal"
                ? terminalSessions.get(node.sessionId ?? "") ?? fallbackSession
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
                onNoteChange={(text) =>
                  dispatch({ type: "note/update", nodeId: node.id, text })
                }
                onOpenSession={() => session && onSelectSession(session.id)}
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
  readonly onNoteChange: (text: string) => void;
  readonly onOpenSession: () => void;
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
  onNoteChange,
  onOpenSession,
}: CanvasNodeCardProps) {
  const dragRef = useRef<{
    readonly pointerId: number;
    readonly clientX: number;
    readonly clientY: number;
    readonly nodeX: number;
    readonly nodeY: number;
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
      style={{ transform: `translate(${node.x}px, ${node.y}px)` }}
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
        <TerminalNodeBody
          node={node}
          session={session}
          onOpenSession={onOpenSession}
        />
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

function TerminalNodeBody({
  node,
  session,
  onOpenSession,
}: {
  readonly node: TerminalCanvasNode;
  readonly session?: Session;
  readonly onOpenSession: () => void;
}) {
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
            <Icon name="terminal" /> Terminal draft
          </span>
        )}
      </div>
      <div className="canvas-terminal__body">
        <Icon name="terminal" />
        <p>
          {session
            ? "The PTY surface will attach here without routing output through React."
            : "Create a project session to attach a live PTY to this terminal card."}
        </p>
        {session ? (
          <button type="button" onClick={onOpenSession}>
            Open session details
          </button>
        ) : null}
      </div>
    </div>
  );
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
