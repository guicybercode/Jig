import { useMemo, useState } from "react";

import type { Project, Session } from "../../../ipc/types";
import { Icon } from "../../components/Icon";

interface CanvasSidebarProps {
  readonly projects: readonly Project[];
  readonly sessions: readonly Session[];
  readonly selectedProjectId?: string;
  readonly canManageProjects: boolean;
  readonly onSelectProject: (projectId: string) => void;
  readonly onAddProject: () => void;
  readonly onOpenSettings: () => void;
  readonly onOpenDiagnostics: () => void;
}

/** Compact workspace navigation used only by the spatial canvas. */
export function CanvasSidebar({
  projects,
  sessions,
  selectedProjectId,
  canManageProjects,
  onSelectProject,
  onAddProject,
  onOpenSettings,
  onOpenDiagnostics,
}: CanvasSidebarProps) {
  const [query, setQuery] = useState("");
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

  return (
    <aside className="canvas-sidebar" aria-label="Canvas workspaces">
      <div className="canvas-sidebar__topbar">
        <button
          type="button"
          aria-label="Add workspace project"
          disabled={!canManageProjects}
          onClick={onAddProject}
        >
          <Icon name="plus" />
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
          <div className="canvas-sidebar__workspace is-selected">
            <Icon name="monitor" />
            <span>My Workspace</span>
            <small aria-label={`${sessions.length} sessions`}>
              <Icon name="terminal" /> {sessions.length}
            </small>
          </div>
        ) : visibleProjects.length > 0 ? (
          <ul>
            {visibleProjects.map((project) => {
              const sessionCount = sessions.filter(
                (session) => session.projectId === project.id,
              ).length;
              return (
                <li key={project.id}>
                  <button
                    type="button"
                    aria-current={
                      project.id === selectedProjectId ? "page" : undefined
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
        <button type="button" onClick={onOpenSettings}>
          <Icon name="settings" />
          <span>Settings</span>
        </button>
        <button
          type="button"
          aria-label="Open diagnostics"
          title="Diagnostics"
          onClick={onOpenDiagnostics}
        >
          <Icon name="diagnostics" />
        </button>
      </footer>
    </aside>
  );
}
