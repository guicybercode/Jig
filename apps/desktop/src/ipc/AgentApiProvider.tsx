import { createElement, type ReactNode } from "react";

import type { AgentApi } from "./agentApi";
import { AgentApiContext } from "./agentApiContext";

interface AgentApiProviderProps {
  readonly api: AgentApi;
  readonly children: ReactNode;
}

/** Provides the agent catalog client to the desktop shell. */
export function AgentApiProvider({ api, children }: AgentApiProviderProps) {
  return createElement(AgentApiContext.Provider, { value: api }, children);
}
