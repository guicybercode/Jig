import type { ReactNode } from "react";

import { Icon } from "../../components/Icon";
import type { ProjectView, SessionView } from "../../../ipc/types";

interface ProjectSidebarProps {
  daemonConnected: boolean;
  projects: ProjectView[];
  sessions: SessionView[];
  selectedProjectId: string | null;
  onSelectProject: (projectId: string) => void;
  onAddProject: () => void;
  onSelectSession: (sessionId: string) => void;
}

/** Renders project and session navigation for empty and populated workspaces. */
export function ProjectSidebar({
  daemonConnected,
  projects,
  sessions,
  selectedProjectId,
  onSelectProject,
  onAddProject,
  onSelectSession,
}: ProjectSidebarProps) {
  const visibleSessions = selectedProjectId
    ? sessions.filter((session) => session.projectId === selectedProjectId)
    : [];
  const activeCount = visibleSessions.filter((session) =>
    ["starting", "running", "idle"].includes(session.status),
  ).length;

  return (
    <aside className="sidebar" aria-label="Project and session navigation">
      <div className="sidebar__heading">
        <span>Workspace</span>
        <span
          className="sidebar__count"
          aria-label={
            projects.length === 0 ? "No projects" : `${projects.length} projects`
          }
        >
          {projects.length}
        </span>
      </div>
      <nav className="sidebar__navigation" aria-label="Workspace navigation">
        <section className="sidebar-section" aria-labelledby="projects-heading">
          <div className="sidebar-section__header">
            <h2 id="projects-heading">Projects</h2>
          </div>
          {projects.length === 0 ? (
            <div className="sidebar-section__empty">
              <Icon name="folder" />
              <p>No repositories added.</p>
            </div>
          ) : (
            <ul className="sidebar-list">
              {projects.map((project) => (
                <li key={project.id}>
                  <button
                    type="button"
                    className={
                      project.id === selectedProjectId
                        ? "sidebar-list__item sidebar-list__item--active"
                        : "sidebar-list__item"
                    }
                    aria-current={
                      project.id === selectedProjectId ? "page" : undefined
                    }
                    onClick={() => onSelectProject(project.id)}
                  >
                    <Icon name="repository" />
                    <span>{project.name}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
          <button
            className="button button--secondary button--full"
            type="button"
            disabled={!daemonConnected}
            aria-describedby="add-project-requirement"
            onClick={onAddProject}
          >
            <Icon name="plus" />
            <span>Add Project</span>
          </button>
          <p id="add-project-requirement" className="sidebar-section__hint">
            Available when the local daemon is connected.
          </p>
        </section>
        <section className="sidebar-section" aria-labelledby="sessions-heading">
          <div className="sidebar-section__header">
            <h2 id="sessions-heading">Sessions</h2>
            <span className="sidebar-section__meta">{activeCount} active</span>
          </div>
          {visibleSessions.length === 0 ? (
            <div className="sidebar-section__empty">
              <Icon name="session" />
              <p>Sessions appear here after you select a project.</p>
            </div>
          ) : (
            <ul className="sidebar-list">
              {visibleSessions.map((session) => (
                <li key={session.id}>
                  <button
                    type="button"
                    className="sidebar-list__item"
                    onClick={() => onSelectSession(session.id)}
                  >
                    <Icon name="session" />
                    <span>{session.name}</span>
                    <SessionStatusBadge status={session.status} />
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>
      </nav>
    </aside>
  );
}

function SessionStatusBadge({
  status,
}: {
  status: SessionView["status"];
}): ReactNode {
  const labels: Record<SessionView["status"], string> = {
    starting: "Starting",
    running: "Running",
    idle: "Idle",
    exited: "Exited",
    failed: "Failed",
    unknown: "Unknown",
  };
  return (
    <span className={`status-badge status-badge--${status}`}>
      {labels[status]}
    </span>
  );
}
