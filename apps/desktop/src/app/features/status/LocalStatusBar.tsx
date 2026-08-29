import { Icon } from "../../components/Icon";
import { useConnection, useConnectionLabel } from "../../workspace";

/** Reports the current local-only application and daemon state. */
export function LocalStatusBar() {
  const connection = useConnection();
  const label = useConnectionLabel();
  const ready = connection.phase === "ready";

  return (
    <footer className="status-bar">
      <div className="status-bar__item">
        <Icon name="branch" />
        <span>Local-first</span>
      </div>
      <div
        className={
          ready
            ? "status-bar__item"
            : "status-bar__item status-bar__item--unavailable"
        }
        role="status"
        aria-live="polite"
        aria-atomic="true"
      >
        <span className="status-bar__indicator" aria-hidden="true" />
        <span>{label}</span>
      </div>
    </footer>
  );
}
