import { Icon } from "../../components/Icon";

/** Guides first-time setup without inventing project or session data. */
export function WorkspaceEmptyState() {
  return (
    <main id="workspace" className="workspace" tabIndex={-1}>
      <header className="workspace__header">
        <div>
          <p className="workspace__eyebrow">Workspace</p>
          <h1>No project selected</h1>
        </div>
        <span className="workspace__mode">Local</span>
      </header>
      <section className="empty-state" aria-labelledby="empty-state-title">
        <span className="empty-state__icon" aria-hidden="true">
          <Icon name="repository" />
        </span>
        <div className="empty-state__copy">
          <h2 id="empty-state-title">Add a repository to begin</h2>
          <p>
            Jig keeps projects and sessions on this device. Connect the
            local daemon, then add a repository from the sidebar.
          </p>
        </div>
        <div className="empty-state__steps" aria-label="Getting started">
          <div className="empty-state__step">
            <span className="empty-state__step-number" aria-hidden="true">1</span>
            <span>Connect the local daemon</span>
          </div>
          <div className="empty-state__step">
            <span className="empty-state__step-number" aria-hidden="true">2</span>
            <span>Add a repository</span>
          </div>
          <div className="empty-state__step">
            <span className="empty-state__step-number" aria-hidden="true">3</span>
            <span>Start a session</span>
          </div>
        </div>
      </section>
    </main>
  );
}
