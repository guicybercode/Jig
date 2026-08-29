import { useState } from "react";

import type { AgentRecord, ApiErrorData, Project, Session, Worktree } from "../../../ipc/types";
import { Icon } from "../../components/Icon";
import { StatusBadge } from "../../components/StatusBadge";
import { copyText, errorData, formatActivityTime, isLiveStatus, toDateTime } from "../../utils";

interface SessionWorkspaceProps {
  readonly session: Session;
  readonly project: Project;
  readonly agent?: AgentRecord;
  readonly worktree?: Worktree;
  readonly isConnected: boolean;
  readonly onStart: (sessionId: string) => Promise<Session>;
  readonly onRestart: (sessionId: string) => Promise<Session>;
  readonly onRename: (sessionId: string) => void;
  readonly onStop: (sessionId: string) => void;
  readonly onDelete: (sessionId: string) => void;
  readonly onRemoveWorktree: (sessionId: string) => void;
  readonly onGitStatus: (sessionId: string) => void;
  readonly onOpenPath: (path: string) => Promise<void>;
}

/** Renders session metadata/actions while leaving the terminal host seam untouched. */
export function SessionWorkspace({
  session,
  project,
  agent,
  worktree,
  isConnected,
  onStart,
  onRestart,
  onRename,
  onStop,
  onDelete,
  onRemoveWorktree,
  onGitStatus,
  onOpenPath,
}: SessionWorkspaceProps) {
  const [pendingAction, setPendingAction] = useState<string>();
  const [notice, setNotice] = useState<string>();
  const [actionError, setActionError] = useState<ApiErrorData>();
  const live = isLiveStatus(session.status);
  const worktreeUnavailable = Boolean(session.worktreeId && !worktree);
  const path = worktreeUnavailable
    ? undefined
    : (worktree?.path ?? session.worktreePath ?? session.cwd);

  async function runAction(
    name: string,
    action: () => Promise<unknown>,
    successMessage?: string,
  ) {
    if (pendingAction) {
      return;
    }
    setPendingAction(name);
    setActionError(undefined);
    setNotice(undefined);
    try {
      await action();
      setNotice(successMessage);
    } catch (error) {
      setActionError(errorData(error));
    } finally {
      setPendingAction(undefined);
    }
  }

  async function handleCopyPath() {
    if (!path) return;
    await runAction("copy", () => copyText(path), "Path copied to clipboard.");
  }

  return (
    <main id="workspace" className="workspace" tabIndex={-1}>
      <header className="workspace-header">
        <div className="workspace-header__identity">
          <p className="workspace-header__eyebrow">{project.name} / Session</p>
          <div className="workspace-header__title-row">
            <h1>{session.name}</h1>
            <StatusBadge status={session.status} />
          </div>
        </div>
        <div className="workspace-header__actions" aria-label="Session actions">
          {!live ? (
            <button
              className="button button--secondary"
              type="button"
              disabled={!isConnected || worktreeUnavailable || pendingAction !== undefined}
              title={worktreeUnavailable ? "The managed worktree is no longer available" : undefined}
              onClick={() => void runAction("start", () => onStart(session.id))}
            >
              <Icon name="play" />
              Start
            </button>
          ) : null}
          <button
            className="button button--secondary"
            type="button"
            disabled={!isConnected || worktreeUnavailable || pendingAction !== undefined}
            title={worktreeUnavailable ? "The managed worktree is no longer available" : undefined}
            onClick={() => void runAction("restart", () => onRestart(session.id))}
          >
            <Icon name="refresh" />
            Restart
          </button>
          <button
            className="button button--secondary"
            type="button"
            disabled={!isConnected || !live || pendingAction !== undefined}
            onClick={() => onStop(session.id)}
          >
            <Icon name="stop" />
            Stop Process
          </button>
          <button
            className="icon-button"
            type="button"
            aria-label={`Rename ${session.name}`}
            title="Rename session"
            disabled={!isConnected}
            onClick={() => onRename(session.id)}
          >
            <Icon name="pencil" />
          </button>
        </div>
      </header>

      {actionError ? (
        <div className="notice notice--error" role="alert">
          <div>
            <strong>{actionError.message}</strong>
            {actionError.action ? <p>{actionError.action}</p> : null}
          </div>
          <button className="icon-button" type="button" aria-label="Dismiss error" onClick={() => setActionError(undefined)}>
            <Icon name="close" />
          </button>
        </div>
      ) : null}
      {notice ? <div className="notice notice--success" role="status">{notice}</div> : null}

      <div className="session-workspace">
        <section className="session-overview" aria-labelledby="session-overview-title">
          <div className="section-heading">
            <h2 id="session-overview-title">Session details</h2>
            <button className="button button--quiet" type="button" disabled={!isConnected || worktreeUnavailable} title={worktreeUnavailable ? "The managed worktree is no longer available" : undefined} onClick={() => onGitStatus(session.id)}>
              <Icon name="branch" />
              Git Status
            </button>
          </div>
          <dl className="metadata-grid">
            <div>
              <dt>Agent</dt>
              <dd>{agent?.displayName ?? "Unknown agent"}</dd>
            </div>
            <div>
              <dt>Last activity</dt>
              <dd>
                <time
                  dateTime={toDateTime(session.lastActivityAtMs ?? session.updatedAtMs)}
                  title={new Date(session.lastActivityAtMs ?? session.updatedAtMs).toLocaleString()}
                >
                  {formatActivityTime(session.lastActivityAtMs ?? session.updatedAtMs)}
                </time>
              </dd>
            </div>
            <div>
              <dt>Branch</dt>
              <dd className="mono">{session.branch ?? project.currentBranch ?? "Unavailable"}</dd>
            </div>
            <div>
              <dt>Process</dt>
              <dd className="mono">
                {session.pid
                  ? `PID ${session.pid}`
                  : session.status === "exited" || session.status === "failed"
                    ? `Ended${session.exitCode !== undefined ? ` · exit ${session.exitCode}` : ""}`
                    : "No attached process"}
              </dd>
            </div>
            <div className="metadata-grid__wide">
              <dt>Working directory</dt>
              <dd className="path-value" title={path}>{path ?? "Unavailable — managed worktree was removed"}</dd>
            </div>
            <div className="metadata-grid__wide">
              <dt>Worktree</dt>
              <dd>
                {worktree ? (
                  <span className={worktree.isDirty ? "text-warning" : undefined}>
                    {worktree.isDirty ? "Dirty — uncommitted changes present" : "Clean"}
                  </span>
                ) : session.worktreeId ? (
                  "Managed worktree removed or unavailable"
                ) : (
                  "Current working tree"
                )}
              </dd>
            </div>
          </dl>
          <div className="session-path-actions">
            <button className="button button--secondary" type="button" disabled={!path || pendingAction !== undefined} onClick={() => void handleCopyPath()}>
              <Icon name="copy" />
              Copy Path
            </button>
            <button className="button button--secondary" type="button" disabled={!path || pendingAction !== undefined} onClick={() => path && void runAction("open", () => onOpenPath(path))}>
              <Icon name="folder" />
              Open in System
            </button>
          </div>
        </section>

        <section
          id="terminal-workspace"
          className="terminal-host"
          data-terminal-root="true"
          tabIndex={0}
          aria-label={`Terminal host for ${session.name}`}
        >
          <div className="terminal-host__header">
            <span><Icon name="terminal" /> Terminal</span>
            <span className="terminal-host__status">Session {session.status}</span>
          </div>
          <div className="terminal-host__placeholder">
            <Icon name="terminal" />
            <p>Terminal rendering attaches here without flowing output through React state.</p>
          </div>
        </section>

        <section className="danger-zone" aria-labelledby="session-data-title">
          <div>
            <h2 id="session-data-title">Session data</h2>
            <p>These operations affect different resources and are never combined implicitly.</p>
          </div>
          <div className="danger-zone__actions">
            <button
              className="button button--danger-secondary"
              type="button"
              disabled={!isConnected || live}
              title={!isConnected ? "Reconnect the daemon first" : live ? "Stop the process before deleting session metadata" : undefined}
              onClick={() => onDelete(session.id)}
            >
              <Icon name="trash" />
              Delete Session
            </button>
            <button
              className="button button--danger-secondary"
              type="button"
              disabled={!isConnected || !worktree || live}
              title={!isConnected ? "Reconnect the daemon first" : !worktree ? "This session has no available managed worktree" : live ? "Stop the process before removing its worktree" : undefined}
              onClick={() => onRemoveWorktree(session.id)}
            >
              <Icon name="worktree" />
              Remove Worktree
            </button>
          </div>
        </section>
      </div>
    </main>
  );
}
