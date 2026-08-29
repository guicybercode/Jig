import { createContext, createElement, useContext, type ReactNode } from "react";

import type { AgentApi } from "./agentApi";

const AgentApiContext = createContext<AgentApi | null>(null);

interface AgentApiProviderProps {
  readonly api: AgentApi;
  readonly children: ReactNode;
}

/** Provides the agent catalog client to the desktop shell. */
export function AgentApiProvider({ api, children }: AgentApiProviderProps) {
  return createElement(AgentApiContext.Provider, { value: api }, children);
}

/** Returns the injected agent catalog client. */
export function useAgentApi(): AgentApi {
  const api = useContext(AgentApiContext);
  if (!api) {
    throw new Error("useAgentApi must be used within AgentApiProvider");
  }
  return api;
}
