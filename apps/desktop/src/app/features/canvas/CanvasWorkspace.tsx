import { useMemo, useRef } from "react";

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
        <span
          className={
            isConnected
              ? "canvas-service-state canvas-service-state--connected"
              : "canvas-service-state"
          }
        >
          <span aria-hidden="true" />
          {isConnected ? "Daemon connected" : "Daemon offline"}
        </span>
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
                onDelete={() =>
                  dispatch({ type: "node/delete", nodeId: node.id })
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
  readonly onSelect: () => void;
  readonly onConnect: () => void;
  readonly onDelete: () => void;
  readonly onNoteChange: (text: string) => void;
  readonly onOpenSession: () => void;
}

function CanvasNodeCard({
  node,
  session,
  selected,
  connectionSource,
  connectionCount,
  onSelect,
  onConnect,
  onDelete,
  onNoteChange,
  onOpenSession,
}: CanvasNodeCardProps) {
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
      aria-selected={selected}
      onFocus={onSelect}
      onPointerDown={(event) => {
        event.stopPropagation();
        onSelect();
      }}
    >
      <header className="canvas-node__header">
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
