import { useEffect, useMemo, useState } from "react";

import { formatApiError, type Project } from "../../../lib/ipc";
import { useWorkspace } from "../../workspace/WorkspaceContext";
import { TerminalGrid } from "./TerminalGrid";

interface WorkspaceMainProps {
  readonly project: Project;
}

/** Project workspace: sessions, Git, and custom agents. */
export function WorkspaceMain({ project }: WorkspaceMainProps) {
  const workspace = useWorkspace();
  const [name, setName] = useState("");
  const [agentId, setAgentId] = useState("");
  const [createWorktree, setCreateWorktree] = useState(false);
  const [customName, setCustomName] = useState("Fake agent");
  const [customExecutable, setCustomExecutable] = useState("/bin/cat");
  const [customArgs, setCustomArgs] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const projectSessions = workspace.sessions.filter(
    (session) => session.projectId === project.id,
  );
  const projectWorktrees = workspace.worktrees.filter(
    (worktree) => worktree.projectId === project.id,
  );
  const launchableAgents = useMemo(
    () => workspace.agents.filter((agent) => agent.enabled),
    [workspace.agents],
  );

  useEffect(() => {
    if (!agentId && launchableAgents[0]) {
      setAgentId(launchableAgents[0].id);
    }
  }, [agentId, launchableAgents]);

  return (
    <main id="workspace" className="workspace" tabIndex={-1}>
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
      {workspace.loading ? (
        <p className="workspace__loading">Loading workspace…</p>
      ) : null}
      {error ? (
        <p className="form-error" role="alert">
          {error}
        </p>
      ) : null}
      <div className="workspace__panels">
        <section className="panel" aria-labelledby="new-session-heading">
          <h2 id="new-session-heading">New session</h2>
          <form
            id="new-session-form"
            className="stack-form"
            onSubmit={(event) => {
              event.preventDefault();
              setError(null);
              setBusy(true);
              void workspace
                .createSession({
                  agentId,
                  name: name.trim() || undefined,
                  createWorktree,
                })
                .catch((caught: unknown) => setError(formatApiError(caught)))
                .finally(() => setBusy(false));
            }}
          >
            <label htmlFor="session-name">Session name</label>
            <input
              id="session-name"
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
            <label htmlFor="session-agent">Agent</label>
            <select
              id="session-agent"
              value={agentId}
              onChange={(event) => setAgentId(event.target.value)}
            >
              {launchableAgents.map((agent) => (
                <option key={agent.id} value={agent.id}>
                  {agent.displayName}
                  {agent.detected ? "" : " (not found)"}
                </option>
              ))}
            </select>
            <label className="checkbox">
              <input
                type="checkbox"
                checked={createWorktree}
                onChange={(event) => setCreateWorktree(event.target.checked)}
              />
              Create a new Git worktree
            </label>
            <button className="button button--primary" type="submit" disabled={busy}>
              Start session
            </button>
          </form>
        </section>
        <section className="panel" aria-labelledby="custom-agent-heading">
          <h2 id="custom-agent-heading">Custom agent</h2>
          <form
            className="stack-form"
            onSubmit={(event) => {
              event.preventDefault();
              setError(null);
              const args = customArgs
                .split(/\s+/)
                .map((part) => part.trim())
                .filter(Boolean);
              void workspace
                .createCustomAgent(customName.trim(), customExecutable.trim(), args)
                .catch((caught: unknown) => setError(formatApiError(caught)));
            }}
          >
            <label htmlFor="custom-name">Display name</label>
            <input
              id="custom-name"
              value={customName}
              onChange={(event) => setCustomName(event.target.value)}
            />
            <label htmlFor="custom-executable">Executable</label>
            <input
              id="custom-executable"
              value={customExecutable}
              onChange={(event) => setCustomExecutable(event.target.value)}
            />
            <label htmlFor="custom-args">Arguments</label>
            <input
              id="custom-args"
              value={customArgs}
              onChange={(event) => setCustomArgs(event.target.value)}
              placeholder="one token per word"
            />
            <button className="button button--secondary" type="submit">
              Register custom agent
            </button>
          </form>
          <ul className="agent-list">
            {workspace.agents.map((agent) => (
              <li key={agent.id}>
                {agent.displayName} · {agent.executable}
                {agent.detected ? " · detected" : " · missing"}
              </li>
            ))}
          </ul>
        </section>
        <GitPanel
          worktrees={projectWorktrees}
          onError={setError}
        />
      </div>
      <TerminalGrid sessions={projectSessions} />
      {projectSessions.length > 0 ? (
        <div className="session-toolbar">
          {projectSessions.map((session) => (
            <div key={session.id} className="session-toolbar__item">
              <button
                type="button"
                className="button button--secondary"
                onClick={() => void workspace.stopSession(session.id).catch((caught: unknown) => setError(formatApiError(caught)))}
              >
                Stop {session.name}
              </button>
              <button
                type="button"
                className="button button--secondary"
                onClick={() =>
                  void workspace
                    .deleteSession(session.id)
                    .catch((caught: unknown) => setError(formatApiError(caught)))
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
  worktrees,
  onError,
}: {
  readonly worktrees: ReturnType<typeof useWorkspace>["worktrees"];
  readonly onError: (message: string) => void;
}) {
  const workspace = useWorkspace();
  const [worktreeId, setWorktreeId] = useState("");

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
          onClick={() =>
            void workspace
              .inspectGit(worktreeId || undefined)
              .catch((caught: unknown) => onError(formatApiError(caught)))
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
                const plan = await workspace.prepareRemoveWorktree(worktreeId);
                if (plan.inUse) {
                  onError("Stop the session before removing this worktree.");
                  return;
                }
                if (plan.isDirty) {
                  onError(
                    "Worktree has uncommitted changes. Removal is blocked until you confirm a dirty removal.",
                  );
                  return;
                }
                await workspace.removeWorktree(
                  worktreeId,
                  plan.confirmationToken,
                  false,
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
            void workspace
              .removeProject(workspace.selectedProjectId ?? "")
              .catch((caught: unknown) => onError(formatApiError(caught)))
          }
        >
          Remove project from app
        </button>
      </div>
      {workspace.gitStatus ? (
        <p>
          {workspace.gitStatus.branch}
          {workspace.gitStatus.isDirty ? " · dirty" : " · clean"} ·{" "}
          {workspace.gitStatus.changedFileCount} changed
        </p>
      ) : null}
      {workspace.gitDiff ? (
        <pre className="diff-view">{workspace.gitDiff.text || "(no diff)"}</pre>
      ) : null}
    </section>
  );
}
