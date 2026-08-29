import { Icon } from "./Icon";
import {
  useDaemonReady,
  useSelectedProject,
  useWorkspaceActions,
} from "../workspace";

interface AppHeaderProps {
  readonly commandPaletteOpen: boolean;
  readonly onOpenCommandPalette: () => void;
}

/** Renders global product identity and session actions. */
export function AppHeader({
  commandPaletteOpen,
  onOpenCommandPalette,
}: AppHeaderProps) {
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
          aria-haspopup="dialog"
          aria-expanded={commandPaletteOpen}
          aria-keyshortcuts="Control+K Meta+K"
          onClick={onOpenCommandPalette}
        >
          <span>Commands</span>
          <kbd className="app-header__shortcut" aria-hidden="true">
            Ctrl/⌘ K
          </kbd>
        </button>
        <span id="new-session-requirement" className="app-header__requirement">
          Add a project first
        </span>
        <button
          className="button button--primary"
          type="button"
          disabled={!canCreate}
          aria-describedby="new-session-requirement"
          onClick={() => actions.openDialog("newSession")}
        >
          <Icon name="plus" />
          <span>New Session</span>
        </button>
      </div>
    </header>
  );
}
