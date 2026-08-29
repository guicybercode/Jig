import { Icon } from "./Icon";
import {
  useDaemonReady,
  useSelectedProject,
  useWorkspaceActions,
} from "../workspace";

interface AppHeaderProps {
  readonly onOpenAgents: () => void;
  readonly agentsActive: boolean;
}

/** Renders global product identity and session actions. */
export function AppHeader({ onOpenAgents, agentsActive }: AppHeaderProps) {
  const ready = useDaemonReady();
  const project = useSelectedProject();
  const actions = useWorkspaceActions();
  const canCreate = ready && project !== null;

  return (
    <header className="app-header">
      <div className="app-header__brand" aria-label="CLI Master">
        <span className="app-header__mark" aria-hidden="true">
          <Icon name="terminal" />
        </span>
        <span className="app-header__name">CLI Master</span>
        <span className="app-header__edition">Desktop</span>
      </div>
      <div className="app-header__actions">
        <button
          className="button button--secondary"
          type="button"
          onClick={onOpenAgents}
          aria-current={agentsActive ? "page" : undefined}
        >
          <Icon name="settings" />
          <span>Agents</span>
        </button>
        {canCreate ? null : (
          <span id="new-session-requirement" className="app-header__requirement">
            Add a project first
          </span>
        )}
        <button
          className="button button--primary"
          type="button"
          disabled={!canCreate}
          aria-describedby={canCreate ? undefined : "new-session-requirement"}
          onClick={() => actions.openDialog("newSession")}
        >
          <Icon name="plus" />
          <span>New Session</span>
        </button>
      </div>
    </header>
  );
}
