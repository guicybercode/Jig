import { useState } from "react";

import { Icon } from "../../components/Icon";
import { formatApiError } from "../../../lib/ipc";
import { useWorkspace } from "../../workspace/WorkspaceContext";

/** Renders project and session navigation. */
export function ProjectSidebar() {
  const workspace = useWorkspace();
  const [path, setPath] = useState("");
  const [formError, setFormError] = useState<string | null>(null);
  const projectSessions = workspace.sessions.filter(
    (session) => session.projectId === workspace.selectedProjectId,
  );

  return (
    <aside className="sidebar" aria-label="Project and session navigation">
      <div className="sidebar__heading">
        <span>Workspace</span>
        <span
          className="sidebar__count"
          aria-label={
            workspace.projects.length === 0
              ? "No projects"
              : `${workspace.projects.length} projects`
          }
        >
          {workspace.projects.length}
        </span>
      </div>
      <nav className="sidebar__navigation" aria-label="Workspace navigation">
        <section className="sidebar-section" aria-labelledby="projects-heading">
          <div className="sidebar-section__header">
            <h2 id="projects-heading">Projects</h2>
          </div>
          {workspace.projects.length === 0 ? (
            <div className="sidebar-section__empty">
              <Icon name="folder" />
              <p>No repositories added.</p>
            </div>
          ) : (
            <ul className="sidebar-list">
              {workspace.projects.map((project) => (
                <li key={project.id}>
                  <button
                    type="button"
                    className={
                      project.id === workspace.selectedProjectId
                        ? "sidebar-list__item sidebar-list__item--active"
                        : "sidebar-list__item"
                    }
                    onClick={() => workspace.selectProject(project.id)}
                  >
                    <strong>{project.name}</strong>
                    <span>{project.currentBranch ?? "unknown branch"}</span>
                    <span className="sidebar-list__path">{project.path}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
          <form
            className="sidebar-form"
            onSubmit={(event) => {
              event.preventDefault();
              setFormError(null);
              void workspace.addProject(path.trim()).catch((error: unknown) => {
                setFormError(formatApiError(error));
              });
            }}
          >
            <label htmlFor="project-path">Repository path</label>
            <input
              id="project-path"
              value={path}
              onChange={(event) => setPath(event.target.value)}
              placeholder="/home/you/src/app"
              autoComplete="off"
              disabled={!workspace.connected}
            />
            <button
              className="button button--secondary button--full"
              type="submit"
              disabled={!workspace.connected || path.trim().length === 0}
              aria-describedby="add-project-requirement"
            >
              <Icon name="plus" />
              <span>Add Project</span>
            </button>
          </form>
          <p id="add-project-requirement" className="sidebar-section__hint">
            {workspace.connected
              ? "The directory stays on disk when you remove it from CLI Master."
              : "Available when the local daemon is connected."}
          </p>
          {formError ? (
            <p className="form-error" role="alert">
              {formError}
            </p>
          ) : null}
        </section>
        <section className="sidebar-section" aria-labelledby="sessions-heading">
          <div className="sidebar-section__header">
            <h2 id="sessions-heading">Sessions</h2>
            <span className="sidebar-section__meta">
              {projectSessions.filter((session) => session.status === "running").length}{" "}
              active
            </span>
          </div>
          {projectSessions.length === 0 ? (
            <div className="sidebar-section__empty">
              <Icon name="session" />
              <p>Sessions appear here after you select a project.</p>
            </div>
          ) : (
            <ul className="sidebar-list">
              {projectSessions.map((session) => (
                <li key={session.id}>
                  <button
                    type="button"
                    className={
                      session.id === workspace.focusedSessionId
                        ? "sidebar-list__item sidebar-list__item--active"
                        : "sidebar-list__item"
                    }
                    onClick={() => {
                      workspace.focusSession(session.id);
                      workspace.toggleVisible(session.id);
                    }}
                  >
                    <strong>{session.name}</strong>
                    <span>
                      {session.status}
                      {session.branch ? ` · ${session.branch}` : ""}
                    </span>
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
