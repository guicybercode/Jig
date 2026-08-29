import { Icon } from "../../components/Icon";
import type { AgentRecord } from "../../../ipc/agentTypes";
import { availabilityLabel, enabledLabel, statusSummary } from "./agentStatus";

interface AgentCardProps {
  readonly agent: AgentRecord;
  readonly busy?: boolean;
  readonly onDetect: (agent: AgentRecord) => void;
  readonly onToggleEnabled: (agent: AgentRecord) => void;
  readonly onDiagnostics: (agent: AgentRecord) => void;
  readonly onEdit?: (agent: AgentRecord) => void;
  readonly onDelete?: (agent: AgentRecord) => void;
  readonly onUseInSession: (agent: AgentRecord) => void;
}

/** Renders one catalog agent with install/enabled state, never adapter-key-as-id. */
export function AgentCard({
  agent,
  busy = false,
  onDetect,
  onToggleEnabled,
  onDiagnostics,
  onEdit,
  onDelete,
  onUseInSession,
}: AgentCardProps) {
  const availability = availabilityLabel(agent);
  const enabled = enabledLabel(agent);

  return (
    <article className="agent-card" aria-labelledby={`agent-${agent.id}-name`}>
      <header className="agent-card__header">
        <div className="agent-card__identity">
          <h3 id={`agent-${agent.id}-name`}>{agent.displayName}</h3>
          <p className="agent-card__meta">
            <span className="agent-card__source">
              {agent.source === "built_in" ? "Built-in" : "Custom"}
            </span>
            <span className="agent-card__id" title="Public agent id">
              {agent.id}
            </span>
          </p>
        </div>
        <p className="agent-card__status" aria-label={statusSummary(agent)}>
          <span
            className={`status-pill status-pill--${availability}`}
          >
            <Icon name={agent.installed ? "check" : "warning"} />
            <span>{agent.installed ? "Installed" : "Missing"}</span>
          </span>
          <span className={`status-pill status-pill--${enabled}`}>
            <span className="status-pill__dot" aria-hidden="true" />
            <span>{agent.enabled ? "Enabled" : "Disabled"}</span>
          </span>
        </p>
      </header>
      <dl className="agent-card__details">
        <div>
          <dt>Executable</dt>
          <dd>
            <code>{agent.executable}</code>
          </dd>
        </div>
        <div>
          <dt>Path</dt>
          <dd>
            <code>{agent.resolvedPath ?? "Not resolved"}</code>
          </dd>
        </div>
        <div>
          <dt>Version</dt>
          <dd>{agent.version ?? "Unknown"}</dd>
        </div>
        <div>
          <dt>Arguments</dt>
          <dd>
            {agent.defaultArgs.length === 0
              ? "None"
              : agent.defaultArgs.map((argument, index) => (
                  <code key={`${index}-${argument}`} className="agent-card__arg">
                    {argument}
                  </code>
                ))}
          </dd>
        </div>
        {agent.envKeys.length > 0 ? (
          <div>
            <dt>Environment keys</dt>
            <dd>{agent.envKeys.join(", ")}</dd>
          </div>
        ) : null}
      </dl>
      {agent.warning ? <p className="agent-card__warning">{agent.warning}</p> : null}
      <div className="agent-card__actions">
        <button
          className="button button--secondary"
          type="button"
          onClick={() => onDetect(agent)}
          disabled={busy}
        >
          <Icon name="refresh" />
          <span>Detect</span>
        </button>
        <button
          className="button button--secondary"
          type="button"
          onClick={() => onToggleEnabled(agent)}
          disabled={busy}
          aria-pressed={agent.enabled}
        >
          {agent.enabled ? "Disable" : "Enable"}
        </button>
        <button
          className="button button--secondary"
          type="button"
          onClick={() => onDiagnostics(agent)}
        >
          Diagnostics
        </button>
        <button
          className="button button--secondary"
          type="button"
          onClick={() => onUseInSession(agent)}
        >
          Use in session
        </button>
        {onEdit ? (
          <button className="button button--secondary" type="button" onClick={() => onEdit(agent)}>
            Edit
          </button>
        ) : null}
        {onDelete ? (
          <button
            className="button button--danger"
            type="button"
            onClick={() => onDelete(agent)}
          >
            <Icon name="trash" />
            <span>Remove</span>
          </button>
        ) : null}
      </div>
    </article>
  );
}
