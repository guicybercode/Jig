import { useState } from "react";

import type { Project, Worktree } from "../../../ipc";
import { formatApiError, parseWorktreeId } from "../../../ipc";
import {
  useAgentAvailability,
  useAgents,
  useGit,
  useSelectedSessions,
  useSelectedWorktrees,
  useWorkspaceActions,
} from "../../workspace";
import { SessionCreateForm } from "./SessionCreateForm";
import { TerminalGrid } from "./TerminalGrid";

interface WorkspaceMainProps {
  readonly project: Project;
}

/** Project workspace: sessions, Git, custom agents, and xterm surfaces. */
export function WorkspaceMain({ project }: WorkspaceMainProps) {
  const actions = useWorkspaceActions();
  const sessions = useSelectedSessions();
  const worktrees = useSelectedWorktrees();
  const agents = useAgents();
  const availability = useAgentAvailability();
  const [error, setError] = useState<string | null>(null);

  return (
    <main id="workspace" className="workspace workspace--populated" tabIndex={-1}>
      <header className="workspace__header">
        <div>
          <p className="workspace__eyebrow">Workspace</p>
          <h1>{project.name}</h1>
          <p className="workspace__meta">
            {project.path}
            {project.currentBranch ? ` · ${project.currentBranch}` : ""}
          </p>
        </div>
        <span className="workspace__mode">Local</span>
      </header>
      {error ? (
        <p className="form-error" role="alert">
          {error}
        </p>
      ) : null}
      <div className="workspace__panels">
        <section className="panel" aria-labelledby="new-session-heading">
          <h2 id="new-session-heading">New session</h2>
          <SessionCreateForm formId="new-session-form" />
        </section>
        <section className="panel" aria-labelledby="custom-agent-heading">
          <h2 id="custom-agent-heading">Custom agent</h2>
          <p className="workspace__meta">
            Custom agents use a local executable. Environment variables stay
            empty from this form.
          </p>
          <button
            className="button button--secondary"
            type="button"
            onClick={() => actions.openDialog("customAgent")}
          >
            Register custom agent
          </button>
          <ul className="agent-list">
            {agents.map((agent) => (
              <li key={agent.id}>
                {agent.displayName} · {agent.command.executable}
                {availability.get(agent.id) === true
                  ? " · detected"
                  : availability.get(agent.id) === false
                    ? " · missing"
                    : ""}
              </li>
            ))}
          </ul>
        </section>
        <GitPanel
          project={project}
          worktrees={worktrees}
          onError={setError}
        />
      </div>
      <TerminalGrid sessions={sessions} />
      {sessions.length > 0 ? (
        <div className="session-toolbar">
          {sessions.map((session) => (
            <div key={session.id} className="session-toolbar__item">
              <button
                type="button"
                className="button button--secondary"
                onClick={() =>
                  void actions
                    .stopSession(session.id)
                    .catch((caught: unknown) =>
                      setError(formatApiError(caught)),
                    )
                }
              >
                Stop {session.name}
              </button>
              <button
                type="button"
                className="button button--secondary"
                onClick={() =>
                  void actions
                    .deleteSession(session.id)
                    .catch((caught: unknown) =>
                      setError(formatApiError(caught)),
                    )
                }
              >
                Delete metadata
              </button>
            </div>
          ))}
        </div>
      ) : null}
    </main>
  );
}

function GitPanel({
  project,
  worktrees,
  onError,
}: {
  readonly project: Project;
  readonly worktrees: readonly Worktree[];
  readonly onError: (message: string) => void;
}) {
  const actions = useWorkspaceActions();
  const git = useGit();
  const [worktreeId, setWorktreeId] = useState("");

  const selectedWorktree = worktrees.find((item) => item.id === worktreeId);

  return (
    <section className="panel" aria-labelledby="git-heading">
      <h2 id="git-heading">Git</h2>
      <label htmlFor="git-worktree">Worktree</label>
      <select
        id="git-worktree"
        value={worktreeId}
        onChange={(event) => setWorktreeId(event.target.value)}
      >
        <option value="">Project root</option>
        {worktrees.map((worktree) => (
          <option key={worktree.id} value={worktree.id}>
            {worktree.branch}
            {worktree.isDirty ? " (dirty)" : ""}
          </option>
        ))}
      </select>
      <div className="button-row">
        <button
          className="button button--secondary"
          type="button"
          disabled={git.loading}
          onClick={() =>
            void actions.inspectGit(
              selectedWorktree?.sessionId
                ? { kind: "session", sessionId: selectedWorktree.sessionId }
                : { kind: "project", projectId: project.id },
            )
          }
        >
          Refresh status
        </button>
        <button
          className="button button--secondary"
          type="button"
          disabled={!worktreeId}
          onClick={() => {
            void (async () => {
              try {
                const plan = await actions.prepareRemoveWorktree(
                  parseWorktreeId(worktreeId),
                );
                if (plan.status === "blocked") {
                  onError(
                    `Worktree removal is blocked (${plan.blockers.join(", ")}).`,
                  );
                  return;
                }
                await actions.removeWorktree(
                  plan.worktreeId,
                  plan.confirmationToken,
                );
              } catch (caught) {
                onError(formatApiError(caught));
              }
            })();
          }}
        >
          Remove worktree
        </button>
        <button
          className="button button--secondary"
          type="button"
          onClick={() =>
            void actions
              .removeProject(project.id)
              .catch((caught: unknown) => onError(formatApiError(caught)))
          }
        >
          Remove project from app
        </button>
      </div>
      {git.error ? (
        <p className="form-error" role="alert">
          {git.error}
        </p>
      ) : null}
      {git.status ? (
        <p>
          {git.status.branch ?? "unknown branch"}
          {git.status.isDirty ? " · dirty" : " · clean"} ·{" "}
          {git.status.files.length} changed
        </p>
      ) : null}
      {git.diff ? (
        <pre className="diff-view">{git.diff.text || "(no diff)"}</pre>
      ) : null}
    </section>
  );
}
