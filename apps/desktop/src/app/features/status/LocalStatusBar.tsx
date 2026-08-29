import { Icon } from "../../components/Icon";

interface LocalStatusBarProps {
  readonly onOpenDiagnostics: () => void;
}

/** Reports the current local-only application and daemon state. */
export function LocalStatusBar({ onOpenDiagnostics }: LocalStatusBarProps) {
  return (
    <footer className="status-bar">
      <div className="status-bar__item">
        <Icon name="branch" />
        <span>Local-first</span>
      </div>
      <div className="status-bar__actions">
        <button
          className="button button--secondary status-bar__diagnostics"
          type="button"
          onClick={onOpenDiagnostics}
        >
          Diagnostics
        </button>
        <div
          className="status-bar__item status-bar__item--unavailable"
          role="status"
          aria-live="polite"
          aria-atomic="true"
        >
          <span className="status-bar__indicator" aria-hidden="true" />
          <span>Daemon unavailable</span>
        </div>
      </div>
    </footer>
  );
}
