import { useCallback, useEffect, useState } from "react";

import { Icon } from "../../components/Icon";
import { useAgentApi } from "../../../ipc/agentApiContext";
import type {
  AgentDiagnosticsReport,
  AgentRecord,
  CustomAgentInput,
} from "../../../ipc/agentTypes";
import { AgentCard } from "./AgentCard";
import { CreateSessionDialog } from "./CreateSessionDialog";
import { CustomAgentDialog } from "./CustomAgentDialog";
import { DeleteAgentDialog } from "./DeleteAgentDialog";
import { DiagnosticsDialog } from "./DiagnosticsDialog";

interface AgentsViewProps {
  readonly hasProject: boolean;
  readonly onSessionDraft?: (draft: { name: string; agentId: string }) => void;
}

type DialogState =
  | { type: "none" }
  | { type: "create" }
  | { type: "edit"; agent: AgentRecord }
  | { type: "delete"; agent: AgentRecord }
  | { type: "diagnostics"; agent: AgentRecord }
  | { type: "session"; agent?: AgentRecord };

/** Agent catalog, diagnostics, and custom CRUD. */
export function AgentsView({ hasProject, onSessionDraft }: AgentsViewProps) {
  const api = useAgentApi();
  const [agents, setAgents] = useState<AgentRecord[]>([]);
  const [diagnostics, setDiagnostics] = useState<Record<string, AgentDiagnosticsReport>>(
    {},
  );
  const [status, setStatus] = useState<"loading" | "ready" | "error">("loading");
  const [error, setError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [dialog, setDialog] = useState<DialogState>({ type: "none" });

  const load = useCallback(async () => {
    setStatus("loading");
    setError(null);
    try {
      const response = await api.list();
      setAgents(response.agents);
      setStatus("ready");
    } catch (cause) {
      setStatus("error");
      setError(cause instanceof Error ? cause.message : "Could not load agents.");
    }
  }, [api]);

  useEffect(() => {
    let cancelled = false;

    async function loadInitialAgents() {
      try {
        const response = await api.list();
        if (cancelled) {
          return;
        }
        setAgents(response.agents);
        setStatus("ready");
      } catch (cause) {
        if (cancelled) {
          return;
        }
        setStatus("error");
        setError(
          cause instanceof Error ? cause.message : "Could not load agents.",
        );
      }
    }

    void loadInitialAgents();
    return () => {
      cancelled = true;
    };
  }, [api]);

  async function detect(agent?: AgentRecord) {
    setBusyId(agent?.id ?? "all");
    try {
      const response = await api.detect(agent?.id);
      setAgents(response.agents);
      setDiagnostics((current) => {
        const next = { ...current };
        for (const report of response.diagnostics) {
          next[report.agentId] = report;
        }
        return next;
      });
      setNotice(
        agent
          ? `Detection finished for ${agent.displayName}.`
          : "Detection finished for all agents.",
      );
      return response;
    } catch (cause) {
      setNotice(cause instanceof Error ? cause.message : "Detection failed.");
      return undefined;
    } finally {
      setBusyId(null);
    }
  }

  async function openDiagnostics(agent: AgentRecord) {
    const response = await detect(agent);
    const latest =
      response?.agents.find((item) => item.id === agent.id) ?? agent;
    setDialog({ type: "diagnostics", agent: latest });
  }

  async function toggleEnabled(agent: AgentRecord) {
    setBusyId(agent.id);
    try {
      const updated = await api.setEnabled(agent.id, !agent.enabled);
      setAgents((current) =>
        current.map((item) => (item.id === updated.id ? updated : item)),
      );
      setNotice(
        `${updated.displayName} is now ${updated.enabled ? "enabled" : "disabled"}.`,
      );
    } catch (cause) {
      setNotice(cause instanceof Error ? cause.message : "Could not update agent.");
    } finally {
      setBusyId(null);
    }
  }

  async function saveCustom(input: CustomAgentInput, agent?: AgentRecord) {
    const saved = agent
      ? await api.updateCustom(agent.id, input)
      : await api.createCustom(input);
    setAgents((current) => {
      const without = current.filter((item) => item.id !== saved.id);
      return [...without, saved];
    });
    setNotice(`${saved.displayName} saved.`);
  }

  async function removeCustom(agent: AgentRecord) {
    await api.removeCustom(agent.id);
    setAgents((current) => current.filter((item) => item.id !== agent.id));
    setDialog({ type: "none" });
    setNotice(`${agent.displayName} removed from the catalog.`);
  }

  const builtins = agents.filter((agent) => agent.source === "built_in");
  const customs = agents.filter((agent) => agent.source === "custom");

  return (
    <main id="workspace" className="workspace" tabIndex={-1}>
      <header className="workspace__header">
        <div>
          <p className="workspace__eyebrow">Settings</p>
          <h1>Agents</h1>
        </div>
        <div className="workspace__header-actions">
          <button
            className="button button--secondary"
            type="button"
            onClick={() => void detect()}
            disabled={status !== "ready"}
          >
            <Icon name="refresh" />
            <span>Detect all</span>
          </button>
          <button
            className="button button--primary"
            type="button"
            onClick={() => setDialog({ type: "create" })}
            disabled={status !== "ready"}
          >
            <Icon name="plus" />
            <span>Add custom agent</span>
          </button>
        </div>
      </header>

      <div className="agents-view" aria-busy={status === "loading"}>
        {notice ? (
          <p className="visually-hidden" role="status" aria-live="polite">
            {notice}
          </p>
        ) : null}

        {status === "loading" ? (
          <div className="panel-state" role="status">
            Loading agents…
          </div>
        ) : null}

        {status === "error" ? (
          <div className="panel-state panel-state--error" role="alert">
            <p>{error}</p>
            <button className="button button--secondary" type="button" onClick={() => void load()}>
              Retry
            </button>
          </div>
        ) : null}

        {status === "ready" && agents.length === 0 ? (
          <div className="panel-state" role="status">
            <h2>No agents in the catalog</h2>
            <p>Add a custom agent or restore the built-in defaults.</p>
          </div>
        ) : null}

        {status === "ready" && builtins.length > 0 ? (
          <section className="agent-section" aria-labelledby="builtin-agents-heading">
            <h2 id="builtin-agents-heading">Built-in</h2>
            <div className="agent-grid">
              {builtins.map((agent) => (
                <AgentCard
                  key={agent.id}
                  agent={agent}
                  busy={busyId === agent.id || busyId === "all"}
                  onDetect={(item) => void detect(item)}
                  onToggleEnabled={(item) => void toggleEnabled(item)}
                  onDiagnostics={(item) => void openDiagnostics(item)}
                  onUseInSession={(item) => setDialog({ type: "session", agent: item })}
                />
              ))}
            </div>
          </section>
        ) : null}

        {status === "ready" ? (
          <section className="agent-section" aria-labelledby="custom-agents-heading">
            <h2 id="custom-agents-heading">Custom</h2>
            {customs.length === 0 ? (
              <p className="panel-state__copy">
                No custom agents yet. Add an executable and an argument list.
              </p>
            ) : (
              <div className="agent-grid">
                {customs.map((agent) => (
                  <AgentCard
                    key={agent.id}
                    agent={agent}
                    busy={busyId === agent.id || busyId === "all"}
                    onDetect={(item) => void detect(item)}
                    onToggleEnabled={(item) => void toggleEnabled(item)}
                    onDiagnostics={(item) => void openDiagnostics(item)}
                    onEdit={(item) => setDialog({ type: "edit", agent: item })}
                    onDelete={(item) => setDialog({ type: "delete", agent: item })}
                    onUseInSession={(item) =>
                      setDialog({ type: "session", agent: item })
                    }
                  />
                ))}
              </div>
            )}
          </section>
        ) : null}
      </div>

      {dialog.type === "create" || dialog.type === "edit" ? (
        <CustomAgentDialog
          agent={dialog.type === "edit" ? dialog.agent : undefined}
          onClose={() => setDialog({ type: "none" })}
          onSave={async (input) => {
            await saveCustom(
              input,
              dialog.type === "edit" ? dialog.agent : undefined,
            );
          }}
        />
      ) : null}
      {dialog.type === "delete" ? (
        <DeleteAgentDialog
          agent={dialog.agent}
          onClose={() => setDialog({ type: "none" })}
          onConfirm={async () => {
            await removeCustom(dialog.agent);
          }}
        />
      ) : null}
      {dialog.type === "diagnostics" ? (
        <DiagnosticsDialog
          agent={dialog.agent}
          diagnostics={diagnostics[dialog.agent.id]}
          onClose={() => setDialog({ type: "none" })}
        />
      ) : null}
      {dialog.type === "session" ? (
        <CreateSessionDialog
          agents={agents}
          hasProject={hasProject}
          initialAgentId={dialog.agent?.id}
          onClose={() => setDialog({ type: "none" })}
          onCreate={(draft) => {
            onSessionDraft?.(draft);
            setDialog({ type: "none" });
            setNotice(`Session “${draft.name}” would use agent ${draft.agentId}.`);
          }}
        />
      ) : null}
    </main>
  );
}
