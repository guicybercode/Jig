import type { AppPlatform } from "../../ipc/client";
import type { Project } from "../../ipc/types";
import { Icon } from "./Icon";

interface AppHeaderProps {
  readonly project?: Project;
  readonly platform: AppPlatform;
  readonly canCreateSession: boolean;
  readonly navigationOpen: boolean;
  readonly onToggleNavigation: () => void;
  readonly onNewSession: () => void;
  readonly onOpenPalette: () => void;
}

/** Renders product identity, current repository context, and primary actions. */
export function AppHeader({
  project,
  platform,
  canCreateSession,
  navigationOpen,
  onToggleNavigation,
  onNewSession,
  onOpenPalette,
}: AppHeaderProps) {
  const modifier = platform === "macos" ? "⌘" : "Ctrl";
  return (
    <header className="app-header">
      <div className="app-header__leading">
        <button
          className="icon-button app-header__navigation-toggle"
          type="button"
          aria-label={navigationOpen ? "Close navigation" : "Open navigation"}
          aria-expanded={navigationOpen}
          onClick={onToggleNavigation}
        >
          <Icon name="menu" />
        </button>
        <div className="app-header__brand" aria-label="Jig">
          <span className="app-header__mark" aria-hidden="true">
            <Icon name="terminal" />
          </span>
          <span className="app-header__name">Jig</span>
        </div>
        {project ? (
          <div className="app-header__context">
            <span className="app-header__project">{project.name}</span>
            <span className="app-header__branch">
              <Icon name="branch" />
              {project.currentBranch ?? "Branch unavailable"}
            </span>
          </div>
        ) : null}
      </div>
      <div className="app-header__actions">
        <button
          className="command-trigger"
          type="button"
          aria-label={`Open command palette, ${modifier}+K`}
          onClick={onOpenPalette}
        >
          <Icon name="search" />
          <span>Commands</span>
          <kbd>{modifier} K</kbd>
        </button>
        <button
          className="button button--primary"
          type="button"
          disabled={!canCreateSession}
          aria-describedby={!canCreateSession ? "new-session-requirement" : undefined}
          onClick={onNewSession}
        >
          <Icon name="plus" />
          <span>New Session</span>
          <kbd>{modifier} T</kbd>
        </button>
        {!canCreateSession ? (
          <span id="new-session-requirement" className="visually-hidden">
            Add or select an available project first
          </span>
        ) : null}
      </div>
    </header>
  );
}
