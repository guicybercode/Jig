import { useEffect, useMemo, useState } from "react";

import type { Project, Session } from "../../../ipc/types";
import { Icon } from "../../components/Icon";
import {
  CANVAS_DOCUMENT_UPDATED_EVENT,
  CANVAS_STORAGE_KEY,
  parseCanvasDocument,
  type CanvasDocument,
} from "../canvas/canvas-state";

interface CanvasSidebarProps {
  readonly projects: readonly Project[];
  readonly sessions: readonly Session[];
  readonly selectedProjectId?: string;
  readonly activeView: "canvas" | "settings" | "diagnostics";
  readonly canManageProjects: boolean;
  readonly onSelectProject: (projectId: string) => void;
  readonly onOpenCanvas: () => void;
  readonly onHide: () => void;
  readonly onAddProject: () => void;
  readonly onOpenSettings: () => void;
  readonly onOpenDiagnostics: () => void;
}

/** Compact navigation shared by the canvas and its utility views. */
export function CanvasSidebar({
  projects,
  sessions,
  selectedProjectId,
  activeView,
  canManageProjects,
  onSelectProject,
  onOpenCanvas,
  onHide,
  onAddProject,
  onOpenSettings,
  onOpenDiagnostics,
}: CanvasSidebarProps) {
  const [query, setQuery] = useState("");
  const [canvasTerminalCount, setCanvasTerminalCount] = useState(
    readCanvasTerminalCount,
  );
  const visibleProjects = useMemo(() => {
    const normalizedQuery = query.trim().toLocaleLowerCase();
    return [...projects]
      .sort((left, right) => right.lastOpenedAtMs - left.lastOpenedAtMs)
      .filter(
        (project) =>
          !normalizedQuery ||
          project.name.toLocaleLowerCase().includes(normalizedQuery),
      );
  }, [projects, query]);

  useEffect(() => {
    function handleCanvasDocument(event: Event) {
      const document = (event as CustomEvent<CanvasDocument>).detail;
      setCanvasTerminalCount(
        document.nodes.filter((node) => node.kind === "terminal").length,
      );
    }

    window.addEventListener(CANVAS_DOCUMENT_UPDATED_EVENT, handleCanvasDocument);
    return () =>
      window.removeEventListener(
        CANVAS_DOCUMENT_UPDATED_EVENT,
        handleCanvasDocument,
      );
  }, []);

  return (
    <aside className="canvas-sidebar" aria-label="Canvas workspaces">
      <div className="canvas-sidebar__topbar">
        <button
          type="button"
          aria-label="Add workspace project"
          title={
            canManageProjects
              ? "Add project"
              : "Reconnect the daemon to add a project."
          }
          disabled={!canManageProjects}
          onClick={onAddProject}
        >
          <Icon name="plus" />
        </button>
        <button
          type="button"
          aria-label="Hide workspace sidebar"
          title="Hide sidebar"
          onClick={onHide}
        >
          <Icon name="sidebar" />
        </button>
      </div>

      <label className="canvas-sidebar__filter">
        <Icon name="search" />
        <span className="visually-hidden">Filter workspaces</span>
        <input
          type="search"
          value={query}
          placeholder="Filter"
          onChange={(event) => setQuery(event.currentTarget.value)}
        />
      </label>

      <nav aria-label="Workspaces">
        <h2>Workspaces</h2>
        {projects.length === 0 ? (
          <button
            className="canvas-sidebar__workspace"
            type="button"
            aria-current={activeView === "canvas" ? "page" : undefined}
            onClick={onOpenCanvas}
          >
            <Icon name="monitor" />
            <span>My Workspace</span>
            <small aria-label={`${canvasTerminalCount} canvas terminals`}>
              <Icon name="terminal" /> {canvasTerminalCount}
            </small>
          </button>
        ) : visibleProjects.length > 0 ? (
          <ul>
            {visibleProjects.map((project) => {
              const sessionCount =
                project.id === selectedProjectId
                  ? canvasTerminalCount
                  : sessions.filter((session) => session.projectId === project.id)
                      .length;
              return (
                <li key={project.id}>
                  <button
                    type="button"
                    aria-current={
                      activeView === "canvas" && project.id === selectedProjectId
                        ? "page"
                        : undefined
                    }
                    onClick={() => onSelectProject(project.id)}
                  >
                    <Icon name="monitor" />
                    <span>{project.name}</span>
                    <small aria-label={`${sessionCount} sessions`}>
                      <Icon name="terminal" /> {sessionCount}
                    </small>
                  </button>
                </li>
              );
            })}
          </ul>
        ) : (
          <p className="canvas-sidebar__empty">No matching workspaces.</p>
        )}
      </nav>

      <footer>
        <button
          type="button"
          aria-current={activeView === "settings" ? "page" : undefined}
          onClick={onOpenSettings}
        >
          <Icon name="settings" />
          <span>Settings</span>
        </button>
        <button
          type="button"
          aria-label="Open diagnostics"
          title="Diagnostics"
          aria-current={activeView === "diagnostics" ? "page" : undefined}
          onClick={onOpenDiagnostics}
        >
          <Icon name="diagnostics" />
        </button>
      </footer>
    </aside>
  );
}

function readCanvasTerminalCount(): number {
  try {
    return parseCanvasDocument(localStorage.getItem(CANVAS_STORAGE_KEY)).nodes.filter(
      (node) => node.kind === "terminal",
    ).length;
  } catch {
    return 0;
  }
}
