import type { Project, Session, Worktree } from "../../../ipc/types";
import { Icon } from "../../components/Icon";

type ConnectionState = "connecting" | "connected" | "disconnected" | "fatal";

interface LocalStatusBarProps {
  readonly connection: ConnectionState;
  readonly project?: Project;
  readonly sessions: readonly Session[];
  readonly selectedWorktree?: Worktree;
  readonly onOpenDiagnostics: () => void;
}

/** Reports local daemon, repository, and active process state outside terminal output. */
export function LocalStatusBar({
  connection,
  project,
  sessions,
  selectedWorktree,
  onOpenDiagnostics,
}: LocalStatusBarProps) {
  const activeCount = sessions.filter((session) =>
    ["starting", "running", "idle"].includes(session.status),
  ).length;
  const connectionLabel =
    connection === "connecting"
      ? "Connecting to daemon"
      : connection === "fatal"
        ? "Daemon protocol error"
        : "Daemon disconnected";

  return (
    <footer className="status-bar">
      <div className="status-bar__group">
        <span className="status-bar__item">
          <Icon name="branch" />
          <span>{project?.currentBranch ?? "No project branch"}</span>
        </span>
        {selectedWorktree ? (
          <span
            className={`status-bar__item ${selectedWorktree.isDirty ? "status-bar__item--warning" : ""}`}
          >
            <Icon name="worktree" />
            <span>{selectedWorktree.isDirty ? "Worktree dirty" : "Worktree clean"}</span>
          </span>
        ) : null}
        <span className="status-bar__item status-bar__sessions">
          {activeCount} active {activeCount === 1 ? "session" : "sessions"}
        </span>
      </div>
      {connection === "connected" ? null : (
        <div
          className={`status-bar__connection status-bar__connection--${connection}`}
          role="status"
          aria-live="polite"
          aria-atomic="true"
        >
          <span className="status-bar__indicator" aria-hidden="true" />
          <span>{connectionLabel}</span>
          <button
            className="status-bar__diagnostics"
            type="button"
            aria-label="Open Diagnostics"
            title="Open Diagnostics"
            onClick={onOpenDiagnostics}
          >
            <Icon name="diagnostics" />
          </button>
        </div>
      )}
    </footer>
  );
}
