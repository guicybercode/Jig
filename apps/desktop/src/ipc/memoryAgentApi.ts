import type { AgentApi } from "./agentApi";
import { AgentRequestError, type AgentRecord, type CustomAgentInput } from "./agentTypes";
import { BUILTIN_AGENT_IDS, createUuidV7, isUuidV7 } from "./uuid";
import { hasFieldErrors, validateCustomAgent } from "./validateCustomAgent";

export interface SimulatedInstall {
  path: string;
  version?: string;
}

export interface MemoryAgentApiOptions {
  installs?: Partial<Record<string, SimulatedInstall>>;
  failList?: boolean;
  empty?: boolean;
}

interface StoredAgent {
  record: AgentRecord;
  env: Record<string, string>;
}

const BUILTINS: Array<Omit<AgentRecord, "id" | "installed" | "enabled" | "envKeys">> = [
  {
    adapterKey: "codex",
    displayName: "Codex",
    source: "built_in",
    executable: "codex",
    defaultArgs: [],
    requiresPty: true,
  },
  {
    adapterKey: "claude",
    displayName: "Claude Code",
    source: "built_in",
    executable: "claude",
    defaultArgs: [],
    requiresPty: true,
  },
  {
    adapterKey: "gemini",
    displayName: "Gemini CLI",
    source: "built_in",
    executable: "gemini",
    defaultArgs: [],
    requiresPty: true,
  },
  {
    adapterKey: "opencode",
    displayName: "OpenCode",
    source: "built_in",
    executable: "opencode",
    defaultArgs: [],
    requiresPty: true,
  },
];

/** In-memory implementation of the agent IPC methods for UI tests and browser mode. */
export function createMemoryAgentApi(options: MemoryAgentApiOptions = {}): AgentApi {
  const agents = new Map<string, StoredAgent>();

  if (!options.empty) {
    for (const builtin of BUILTINS) {
      const id = BUILTIN_AGENT_IDS[builtin.adapterKey as keyof typeof BUILTIN_AGENT_IDS];
      const install = options.installs?.[builtin.adapterKey] ?? options.installs?.[builtin.executable];
      agents.set(id, {
        record: {
          ...builtin,
          id,
          enabled: true,
          installed: Boolean(install),
          resolvedPath: install?.path,
          version: install?.version,
          envKeys: [],
        },
        env: {},
      });
    }
  }

  const listAgents = () =>
    [...agents.values()]
      .map((entry) => entry.record)
      .sort((left, right) => {
        if (left.source !== right.source) {
          return left.source === "built_in" ? -1 : 1;
        }
        return left.displayName.localeCompare(right.displayName);
      });

  const requireAgent = (agentId: string): StoredAgent => {
    const entry = agents.get(agentId);
    if (!entry) {
      throw new AgentRequestError({
        code: "AGENT_NOT_FOUND",
        message: `No agent is registered for id ${agentId}`,
        action: "Refresh the agent list and try again.",
      });
    }
    return entry;
  };

  const detectOne = (entry: StoredAgent): StoredAgent => {
    const install = options.installs?.[entry.record.adapterKey] ??
      options.installs?.[entry.record.executable];
    const absolute =
      entry.record.executable.startsWith("/") || entry.record.executable.startsWith("~/");
    if (install) {
      entry.record = {
        ...entry.record,
        installed: true,
        resolvedPath: install.path,
        version: install.version,
        warning: undefined,
      };
    } else if (absolute) {
      entry.record = {
        ...entry.record,
        installed: true,
        resolvedPath: entry.record.executable,
        warning: undefined,
      };
    } else {
      entry.record = {
        ...entry.record,
        installed: false,
        resolvedPath: undefined,
        version: undefined,
        warning: "Install the CLI or add its directory to the executable search path.",
      };
    }
    return entry;
  };

  const assertCustomInput = (input: CustomAgentInput) => {
    const errors = validateCustomAgent(input);
    if (hasFieldErrors(errors)) {
      const message = Object.values(errors)[0] ?? "Invalid custom agent.";
      throw new AgentRequestError({
        code: "AGENT_INVALID_DEFINITION",
        message,
        action: "Fix the highlighted field. Arguments must stay an array.",
      });
    }
    const duplicate = listAgents().find(
      (agent) =>
        agent.displayName.toLowerCase() === input.displayName.trim().toLowerCase(),
    );
    if (duplicate) {
      throw new AgentRequestError({
        code: "AGENT_REGISTRY_ERROR",
        message: `agent display name is already registered: ${input.displayName.trim()}`,
        action: "Choose a unique name.",
      });
    }
  };

  const toRecord = (input: CustomAgentInput, id: string, adapterKey: string, enabled: boolean): StoredAgent => {
    const env: Record<string, string> = {};
    for (const entry of input.env) {
      if (entry.key.trim().length > 0) {
        env[entry.key.trim()] = entry.value;
      }
    }
    return {
      env,
      record: {
        id,
        adapterKey,
        displayName: input.displayName.trim(),
        source: "custom",
        enabled,
        installed: false,
        executable: input.executable.trim(),
        defaultArgs: input.args.filter((argument) => argument.length > 0),
        envKeys: Object.keys(env),
        requiresPty: input.requiresPty,
        defaultCwd: input.defaultCwd.trim() || undefined,
      },
    };
  };

  return {
    async list() {
      if (options.failList) {
        throw new AgentRequestError({
          code: "AGENT_UNAVAILABLE",
          message: "The agent catalog could not be loaded.",
          action: "Retry after the local daemon is connected.",
        });
      }
      return { agents: listAgents() };
    },

    async detect(agentId) {
      const targets = agentId ? [requireAgent(agentId)] : [...agents.values()];
      const diagnostics = targets.map((entry) => {
        detectOne(entry);
        return {
          agentId: entry.record.id,
          displayName: entry.record.displayName,
          installed: entry.record.installed,
          launchTest: entry.record.installed
            ? ({ status: "success" } as const)
            : ({ status: "not_found" } as const),
          searchedPaths: [],
          path: entry.record.resolvedPath,
          version: entry.record.version,
          warning: entry.record.warning,
        };
      });
      return { agents: listAgents(), diagnostics };
    },

    async setEnabled(agentId, enabled) {
      const entry = requireAgent(agentId);
      entry.record = { ...entry.record, enabled };
      return entry.record;
    },

    async createCustom(input) {
      assertCustomInput(input);
      const id = createUuidV7();
      if (!isUuidV7(id)) {
        throw new AgentRequestError({
          code: "AGENT_INVALID_DEFINITION",
          message: "Generated agent id was not UUIDv7.",
        });
      }
      const adapterKey = `custom-${id.slice(0, 8)}`;
      const stored = toRecord(input, id, adapterKey, true);
      agents.set(id, stored);
      detectOne(stored);
      return stored.record;
    },

    async updateCustom(agentId, input) {
      const current = requireAgent(agentId);
      if (current.record.source === "built_in") {
        throw new AgentRequestError({
          code: "AGENT_BUILTIN_PROTECTED",
          message: "Built-in agents can be disabled but not edited or removed.",
          action: "Create a custom agent if you need different arguments.",
        });
      }
      const errors = validateCustomAgent(input);
      if (hasFieldErrors(errors)) {
        throw new AgentRequestError({
          code: "AGENT_INVALID_DEFINITION",
          message: Object.values(errors)[0] ?? "Invalid custom agent.",
        });
      }
      const nameClash = listAgents().find(
        (agent) =>
          agent.id !== agentId &&
          agent.displayName.toLowerCase() === input.displayName.trim().toLowerCase(),
      );
      if (nameClash) {
        throw new AgentRequestError({
          code: "AGENT_REGISTRY_ERROR",
          message: `agent display name is already registered: ${input.displayName.trim()}`,
        });
      }
      const stored = toRecord(
        input,
        agentId,
        current.record.adapterKey,
        current.record.enabled,
      );
      agents.set(agentId, stored);
      detectOne(stored);
      return stored.record;
    },

    async removeCustom(agentId) {
      const entry = requireAgent(agentId);
      if (entry.record.source === "built_in") {
        throw new AgentRequestError({
          code: "AGENT_BUILTIN_PROTECTED",
          message: "Built-in agents can be disabled but not edited or removed.",
        });
      }
      agents.delete(agentId);
    },
  };
}
