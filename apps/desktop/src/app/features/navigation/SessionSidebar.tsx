import type { AgentRecord, Session, Worktree } from "../../../ipc/types";
import { Icon } from "../../components/Icon";
import { StatusBadge } from "../../components/StatusBadge";
import { formatActivityTime, toDateTime } from "../../utils";

interface SessionSidebarProps {
  readonly sessions: readonly Session[];
  readonly agents: readonly AgentRecord[];
  readonly worktrees: readonly Worktree[];
  readonly selectedSessionId?: string;
  readonly projectSelected: boolean;
  readonly canCreateSession: boolean;
  readonly canManageWorktrees: boolean;
  readonly onSelectSession: (sessionId: string) => void;
  readonly onNewSession: () => void;
  readonly onRemoveWorktree: (worktreeId: string) => void;
}

/** Renders sessions for the selected project without owning process state. */
export function SessionSidebar({
  sessions,
  agents,
  worktrees,
  selectedSessionId,
  projectSelected,
  canCreateSession,
  canManageWorktrees,
  onSelectSession,
  onNewSession,
  onRemoveWorktree,
}: SessionSidebarProps) {
  const orderedSessions = [...sessions].sort(
    (left, right) => right.updatedAtMs - left.updatedAtMs,
  );
  const activeCount = sessions.filter((session) =>
    ["starting", "running", "idle"].includes(session.status),
  ).length;
  const sessionIds = new Set(sessions.map((session) => session.id));
  const retainedWorktrees = projectSelected
    ? worktrees.filter(
        (worktree) =>
          worktree.sessionId === undefined || !sessionIds.has(worktree.sessionId),
      )
    : [];

  return (
    <aside className="session-pane" aria-label="Sessions">
      <div className="pane-heading">
        <div>
          <span className="pane-heading__label">Sessions</span>
          <span className="pane-heading__meta">{activeCount} active</span>
        </div>
        <button
          className="icon-button"
          type="button"
          aria-label="New Session"
          title={
            canCreateSession
              ? "New Session"
              : projectSelected
                ? "Connect the daemon and use an available repository"
                : "Select a project first"
          }
          disabled={!canCreateSession}
          onClick={onNewSession}
        >
          <Icon name="plus" />
        </button>
      </div>

      <nav className="pane-scroll" aria-label="Project sessions">
        {!projectSelected ? (
          <div className="nav-empty nav-empty--compact">
            <Icon name="session" />
            <p>Select a project to view its sessions.</p>
          </div>
        ) : orderedSessions.length === 0 ? (
          <div className="nav-empty">
            <Icon name="session" />
            <p>No sessions for this project.</p>
            <button className="button button--secondary button--full" type="button" disabled={!canCreateSession} onClick={onNewSession}>
              <Icon name="plus" />
              New Session
            </button>
          </div>
        ) : (
          <ul className="session-list">
            {orderedSessions.map((session, index) => {
              const agent = agents.find(
                (candidate) => candidate.id === session.agentId,
              );
              const worktree = worktrees.find(
                (candidate) => candidate.id === session.worktreeId,
              );
              const activityAt = session.lastActivityAtMs ?? session.updatedAtMs;
              return (
                <li key={session.id}>
                  <button
                    className="session-row"
                    type="button"
                    aria-current={session.id === selectedSessionId ? "page" : undefined}
                    aria-keyshortcuts={index < 9 ? `Control+${index + 1} Meta+${index + 1}` : undefined}
                    onClick={() => onSelectSession(session.id)}
                  >
                    <span className="session-row__heading">
                      <span className="session-row__name">{session.name}</span>
                      <StatusBadge status={session.status} compact />
                    </span>
                    <span className="session-row__agent">
                      {agent?.displayName ?? "Unknown agent"}
                    </span>
                    <span className="session-row__details">
                      <span title={session.branch ?? "No branch"}>
                        <Icon name="branch" />
                        {session.branch ?? "No branch"}
                      </span>
                      <time dateTime={toDateTime(activityAt)} title={new Date(activityAt).toLocaleString()}>
                        {formatActivityTime(activityAt)}
                      </time>
                    </span>
                    {worktree ? (
                      <span className="session-row__worktree">
                        <Icon name="worktree" />
                        {worktree.isDirty ? "Dirty worktree" : "Clean worktree"}
                      </span>
                    ) : null}
                    {session.status === "exited" ? (
                      <span className="session-row__terminal-state">
                        Process ended{session.exitCode !== undefined ? ` · exit ${session.exitCode}` : ""}
                      </span>
                    ) : null}
                    {session.status === "failed" ? (
                      <span className="session-row__terminal-state session-row__terminal-state--error">
                        {session.errorCode === "executable_not_found"
                          ? "Executable not found"
                          : session.exitCode !== undefined
                            ? `Exited with code ${session.exitCode}`
                            : session.errorCode
                              ? `Session failed · ${session.errorCode}`
                              : "Session failed"}
                      </span>
                    ) : null}
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </nav>
      {retainedWorktrees.length > 0 ? (
        <section className="retained-worktrees" aria-labelledby="retained-worktrees-title">
          <h2 id="retained-worktrees-title">Retained worktrees</h2>
          <ul>
            {retainedWorktrees.map((worktree) => (
              <li key={worktree.id}>
                <span>
                  <strong>{worktree.branch}</strong>
                  <small className="mono" title={worktree.path}>{worktree.path}</small>
                </span>
                <button
                  className="button button--danger-secondary"
                  type="button"
                  disabled={!canManageWorktrees}
                  aria-label={`Remove retained worktree ${worktree.branch}`}
                  title={!canManageWorktrees ? "Reconnect the daemon first" : undefined}
                  onClick={() => onRemoveWorktree(worktree.id)}
                >
                  Remove
                </button>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
    </aside>
  );
}
