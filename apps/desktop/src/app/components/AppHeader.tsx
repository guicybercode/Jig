import { Icon } from "./Icon";

interface AppHeaderProps {
  canCreateSession: boolean;
  onNewSession: () => void;
  onOpenCommandPalette: () => void;
}

/** Renders global product identity and session actions. */
export function AppHeader({
  canCreateSession,
  onNewSession,
  onOpenCommandPalette,
}: AppHeaderProps) {
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
        <span id="new-session-requirement" className="app-header__requirement">
          Add a project first
        </span>
        <button
          className="button button--secondary"
          type="button"
          onClick={onOpenCommandPalette}
        >
          Command palette
        </button>
        <button
          className="button button--primary"
          type="button"
          disabled={!canCreateSession}
          aria-describedby="new-session-requirement"
          onClick={onNewSession}
        >
          <Icon name="plus" />
          <span>New Session</span>
        </button>
      </div>
    </header>
  );
}
