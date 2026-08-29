import { Icon } from "./Icon";
import { useWorkspace } from "../workspace/WorkspaceContext";

/** Renders global product identity and session actions. */
export function AppHeader() {
  const workspace = useWorkspace();
  const canCreate = workspace.connected && workspace.selectedProjectId !== null;

  return (
    <header className="app-header">
      <div className="app-header__brand" aria-label="CLI Master">
        <span className="app-header__mark" aria-hidden="true">
          <Icon name="terminal" />
        </span>
        <span className="app-header__name">CLI Master</span>
        <span className="app-header__edition">Beta {workspace.appVersion}</span>
      </div>
      <div className="app-header__actions">
        <span id="new-session-requirement" className="app-header__requirement">
          Add a project first
        </span>
        <button
          className="button button--primary"
          type="button"
          disabled={!canCreate}
          aria-describedby="new-session-requirement"
          onClick={() => {
            document.getElementById("new-session-form")?.scrollIntoView();
            document.getElementById("session-name")?.focus();
          }}
        >
          <Icon name="plus" />
          <span>New Session</span>
        </button>
      </div>
    </header>
  );
}
