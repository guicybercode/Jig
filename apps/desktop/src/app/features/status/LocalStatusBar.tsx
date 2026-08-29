import { Icon } from "../../components/Icon";

/** Reports the current local-only application and daemon state. */
export function LocalStatusBar() {
  return (
    <footer className="status-bar">
      <div className="status-bar__item">
        <Icon name="branch" />
        <span>Local-first</span>
      </div>
      <div
        className="status-bar__item status-bar__item--unavailable"
        role="status"
        aria-live="polite"
        aria-atomic="true"
      >
        <span className="status-bar__indicator" aria-hidden="true" />
        <span>Daemon unavailable</span>
      </div>
    </footer>
  );
}
