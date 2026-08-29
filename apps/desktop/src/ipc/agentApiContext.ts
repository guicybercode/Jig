import { createContext, useContext } from "react";

import type { AgentApi } from "./agentApi";

/** Single context shared by the agent catalog provider and consumers. */
export const AgentApiContext = createContext<AgentApi | null>(null);

/** Returns the injected agent catalog client. */
export function useAgentApi(): AgentApi {
  const api = useContext(AgentApiContext);
  if (!api) {
    throw new Error("useAgentApi must be used within AgentApiProvider");
  }
  return api;
}
