import type { AgentRecord } from "../../../ipc/agentTypes";

export type AgentAvailability = "installed" | "missing";
export type AgentEnabledState = "enabled" | "disabled";

export function availabilityLabel(agent: AgentRecord): AgentAvailability {
  return agent.installed ? "installed" : "missing";
}

export function enabledLabel(agent: AgentRecord): AgentEnabledState {
  return agent.enabled ? "enabled" : "disabled";
}

export function statusSummary(agent: AgentRecord): string {
  return `${availabilityLabel(agent)}, ${enabledLabel(agent)}`;
}
