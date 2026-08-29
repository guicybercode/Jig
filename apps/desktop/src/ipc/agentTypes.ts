export const AGENT_METHODS = {
  list: "agent.list",
  detect: "agent.detect",
  setEnabled: "agent.set_enabled",
  customCreate: "agent.custom.create",
  customUpdate: "agent.custom.update",
  customRemove: "agent.custom.remove",
} as const;

export type AgentSource = "built_in" | "custom";

export type LaunchTestStatus =
  | { status: "success" }
  | { status: "not_found" }
  | { status: "not_executable"; candidate: string }
  | { status: "timeout" }
  | { status: "failed"; message: string };

/** Public catalog row. Environment values are never present. */
export interface AgentRecord {
  id: string;
  adapterKey: string;
  displayName: string;
  source: AgentSource;
  enabled: boolean;
  installed: boolean;
  executable: string;
  defaultArgs: string[];
  envKeys: string[];
  requiresPty: boolean;
  resolvedPath?: string;
  version?: string;
  warning?: string;
  defaultCwd?: string;
}

export interface AgentDiagnosticsReport {
  agentId: string;
  displayName: string;
  installed: boolean;
  launchTest: LaunchTestStatus;
  searchedPaths: string[];
  path?: string;
  version?: string;
  warning?: string;
}

export interface AgentListResponse {
  agents: AgentRecord[];
}

export interface AgentDetectResponse {
  agents: AgentRecord[];
  diagnostics: AgentDiagnosticsReport[];
}

export interface CustomAgentInput {
  displayName: string;
  executable: string;
  args: string[];
  env: Array<{ key: string; value: string }>;
  defaultCwd: string;
  requiresPty: boolean;
}

export interface AgentApiError {
  code: string;
  message: string;
  action?: string;
}

export class AgentRequestError extends Error {
  readonly code: string;
  readonly action?: string;

  constructor(error: AgentApiError) {
    super(error.message);
    this.name = "AgentRequestError";
    this.code = error.code;
    this.action = error.action;
  }
}

export interface CreateSessionDraft {
  name: string;
  agentId: string;
}
