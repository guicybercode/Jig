import { Icon } from "./Icon";

interface AppHeaderProps {
  readonly newSessionDisabledReason: string | null;
  readonly onNewSession: () => void;
  readonly onOpenCommandPalette: () => void;
}

/** Renders global product identity and session actions. */
export function AppHeader({
  newSessionDisabledReason,
  onNewSession,
  onOpenCommandPalette,
}: AppHeaderProps) {
  return (
    <header className="app-header">
      <div className="app-header__brand">
        <span className="app-header__mark" aria-hidden="true">
          <Icon name="terminal" />
        </span>
        <span className="app-header__name">CLI Master</span>
        <span className="app-header__edition">Desktop</span>
      </div>
      <div className="app-header__actions">
        {newSessionDisabledReason ? (
          <span
            id="new-session-requirement"
            className="app-header__requirement"
          >
            {newSessionDisabledReason}
          </span>
        ) : null}
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
          disabled={newSessionDisabledReason !== null}
          aria-describedby={
            newSessionDisabledReason ? "new-session-requirement" : undefined
          }
          onClick={onNewSession}
        >
          <Icon name="plus" />
          <span>New Session</span>
        </button>
      </div>
    </header>
  );
}
