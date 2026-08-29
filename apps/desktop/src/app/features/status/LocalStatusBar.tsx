import { Icon } from "../../components/Icon";
import { useWorkspace } from "../../workspace/WorkspaceContext";

/** Reports the current local-only application and daemon state. */
export function LocalStatusBar() {
  const workspace = useWorkspace();
  const connected = workspace.connected;

  return (
    <footer className="status-bar">
      <div className="status-bar__item">
        <Icon name="branch" />
        <span>Local-first</span>
      </div>
      <div
        className={
          connected
            ? "status-bar__item"
            : "status-bar__item status-bar__item--unavailable"
        }
        role="status"
        aria-live="polite"
        aria-atomic="true"
      >
        <span className="status-bar__indicator" aria-hidden="true" />
        <span>{connected ? "Daemon connected" : "Daemon unavailable"}</span>
      </div>
    </footer>
  );
}
