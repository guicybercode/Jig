import type {
  AgentDetectResponse,
  AgentListResponse,
  AgentRecord,
  CustomAgentInput,
} from "./agentTypes";

/** Typed client for agent catalog IPC methods. */
export interface AgentApi {
  list(): Promise<AgentListResponse>;
  detect(agentId?: string): Promise<AgentDetectResponse>;
  setEnabled(agentId: string, enabled: boolean): Promise<AgentRecord>;
  createCustom(input: CustomAgentInput): Promise<AgentRecord>;
  updateCustom(agentId: string, input: CustomAgentInput): Promise<AgentRecord>;
  removeCustom(agentId: string): Promise<void>;
}
