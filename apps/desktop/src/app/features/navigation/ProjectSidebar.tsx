import { Icon } from "../../components/Icon";

/** Renders project and session navigation in their empty states. */
export function ProjectSidebar() {
  return (
    <aside className="sidebar" aria-label="Project and session navigation">
      <div className="sidebar__heading">
        <span>Workspace</span>
        <span className="sidebar__count" aria-label="No projects">0</span>
      </div>
      <nav className="sidebar__navigation" aria-label="Workspace navigation">
        <section className="sidebar-section" aria-labelledby="projects-heading">
          <div className="sidebar-section__header">
            <h2 id="projects-heading">Projects</h2>
          </div>
          <div className="sidebar-section__empty">
            <Icon name="folder" />
            <p>No repositories added.</p>
          </div>
          <button
            className="button button--secondary button--full"
            type="button"
            disabled
            aria-describedby="add-project-requirement"
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
            <span className="sidebar-section__meta">0 active</span>
          </div>
          <div className="sidebar-section__empty">
            <Icon name="session" />
            <p>Sessions appear here after you select a project.</p>
          </div>
        </section>
      </nav>
    </aside>
  );
}
