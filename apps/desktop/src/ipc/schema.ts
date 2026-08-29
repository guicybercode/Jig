import type {
  AgentDetection,
  AgentRecord,
  ApiErrorData,
  HelloResponse,
  Project,
  Session,
  StateSnapshot,
  Worktree,
} from "./types";

/** Raised when a payload crosses the webview boundary with an invalid shape. */
export class IpcContractError extends Error {
  readonly code = "invalid_ipc_payload";

  constructor(message: string) {
    super(message);
    this.name = "IpcContractError";
  }
}

export function decodeHello(value: unknown): HelloResponse {
  const record = requireRecord(value, "system.hello response");
  return {
    protocolVersion: requireNumber(record.protocolVersion, "protocolVersion"),
    daemonVersion: requireString(record.daemonVersion, "daemonVersion"),
    instanceId: requireString(record.instanceId, "instanceId"),
  };
}

export function decodeSnapshot(value: unknown): StateSnapshot {
  const record = requireRecord(value, "state.snapshot response");
  return {
    schemaVersion: requireNumber(record.schemaVersion, "schemaVersion"),
    projects: requireArray(record.projects, "projects").map(decodeProject),
    agents: requireArray(record.agents, "agents").map(decodeAgentRecord),
    sessions: requireArray(record.sessions, "sessions").map(decodeSession),
    worktrees: requireArray(record.worktrees, "worktrees").map(decodeWorktree),
  };
}

export function decodeProject(value: unknown): Project {
  const record = requireRecord(value, "project");
  const availability = optionalString(record.availability, "availability");
  if (
    availability !== undefined &&
    !["available", "missing", "not_repository", "unknown"].includes(
      availability,
    )
  ) {
    throw new IpcContractError(`Invalid project availability: ${availability}`);
  }
  return {
    id: requireString(record.id, "project.id"),
    name: requireString(record.name, "project.name"),
    path: requireString(record.path, "project.path"),
    repositoryRoot: optionalString(
      record.repositoryRoot,
      "project.repositoryRoot",
    ),
    currentBranch: optionalString(
      record.currentBranch,
      "project.currentBranch",
    ),
    createdAtMs: requireNumber(record.createdAtMs, "project.createdAtMs"),
    lastOpenedAtMs: requireNumber(
      record.lastOpenedAtMs,
      "project.lastOpenedAtMs",
    ),
    availability: availability as Project["availability"],
    availabilityMessage: optionalString(
      record.availabilityMessage,
      "project.availabilityMessage",
    ),
  };
}

export function decodeAgentRecord(value: unknown): AgentRecord {
  const record = requireRecord(value, "agent");
  const source = requireString(record.source, "agent.source");
  if (source !== "built_in" && source !== "custom") {
    throw new IpcContractError(`Invalid agent source: ${source}`);
  }
  const command = requireRecord(record.command, "agent.command");
  const rawEnv = requireRecord(command.env ?? {}, "agent.command.env");
  const env: Record<string, string> = {};
  for (const [key, envValue] of Object.entries(rawEnv)) {
    env[key] = requireString(envValue, `agent.command.env.${key}`);
  }
  return {
    id: requireString(record.id, "agent.id"),
    displayName: requireString(record.displayName, "agent.displayName"),
    description: optionalString(record.description, "agent.description"),
    source,
    command: {
      executable: requireString(command.executable, "agent.command.executable"),
      args: requireArray(command.args, "agent.command.args").map((argument) =>
        requireString(argument, "agent.command.args[]"),
      ),
      env,
    },
    enabled: requireBoolean(record.enabled, "agent.enabled"),
  };
}

export function decodeAgentDetectResponse(value: unknown): readonly AgentDetection[] {
  const record = requireRecord(value, "agent.detect response");
  return requireArray(record.detections, "agent detections").map(
    decodeAgentDetection,
  );
}

export function decodeAgentDetection(value: unknown): AgentDetection {
  const record = requireRecord(value, "agent detection");
  return {
    agentId: requireString(record.agentId, "agent detection.agentId"),
    available: requireBoolean(
      record.available,
      "agent detection.available",
    ),
    executablePath: optionalString(
      record.executablePath,
      "agent detection.executablePath",
    ),
    errorCode: optionalString(
      record.errorCode,
      "agent detection.errorCode",
    ),
  };
}

export function decodeSession(value: unknown): Session {
  const record = requireRecord(value, "session");
  const rawStatus = requireString(record.status, "session.status");
  const status = [
    "starting",
    "running",
    "idle",
    "exited",
    "failed",
    "unknown",
  ].includes(rawStatus)
    ? (rawStatus as Session["status"])
    : "unknown";
  return {
    id: requireString(record.id, "session.id"),
    projectId: requireString(record.projectId, "session.projectId"),
    name: requireString(record.name, "session.name"),
    agentId: requireString(record.agentId, "session.agentId"),
    cwd: requireString(record.cwd, "session.cwd"),
    pid: optionalNumber(record.pid, "session.pid"),
    ptyId: optionalString(record.ptyId, "session.ptyId"),
    branch: optionalString(record.branch, "session.branch"),
    worktreeId: optionalString(record.worktreeId, "session.worktreeId"),
    worktreePath: optionalString(
      record.worktreePath,
      "session.worktreePath",
    ),
    status,
    exitCode: optionalNumber(record.exitCode, "session.exitCode"),
    createdAtMs: requireNumber(record.createdAtMs, "session.createdAtMs"),
    updatedAtMs: requireNumber(record.updatedAtMs, "session.updatedAtMs"),
    lastActivityAtMs: optionalNumber(
      record.lastActivityAtMs,
      "session.lastActivityAtMs",
    ),
    errorCode: optionalString(record.errorCode, "session.errorCode"),
  };
}

export function decodeWorktree(value: unknown): Worktree {
  const record = requireRecord(value, "worktree");
  const state = requireString(record.state, "worktree.state");
  if (!["creating", "active", "remove_pending", "orphaned"].includes(state)) {
    throw new IpcContractError(`Invalid worktree state: ${state}`);
  }
  return {
    id: requireString(record.id, "worktree.id"),
    projectId: requireString(record.projectId, "worktree.projectId"),
    sessionId: optionalString(record.sessionId, "worktree.sessionId"),
    path: requireString(record.path, "worktree.path"),
    branch: requireString(record.branch, "worktree.branch"),
    isDirty: requireBoolean(record.isDirty, "worktree.isDirty"),
    state: state as Worktree["state"],
    createdAtMs: requireNumber(record.createdAtMs, "worktree.createdAtMs"),
    updatedAtMs: requireNumber(record.updatedAtMs, "worktree.updatedAtMs"),
  };
}

export function decodeApiError(value: unknown): ApiErrorData {
  const record = requireRecord(value, "API error");
  return {
    code: requireString(record.code, "error.code"),
    message: requireString(record.message, "error.message"),
    action: optionalString(record.action, "error.action"),
    details:
      record.details === undefined
        ? undefined
        : requireRecord(record.details, "error.details"),
  };
}

export function requireRecord(
  value: unknown,
  label: string,
): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new IpcContractError(`${label} must be an object`);
  }
  return value as Record<string, unknown>;
}

export function requireArray(value: unknown, label: string): readonly unknown[] {
  if (!Array.isArray(value)) {
    throw new IpcContractError(`${label} must be an array`);
  }
  return value;
}

export function requireString(value: unknown, label: string): string {
  if (typeof value !== "string") {
    throw new IpcContractError(`${label} must be a string`);
  }
  return value;
}

export function requireNumber(value: unknown, label: string): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw new IpcContractError(`${label} must be a finite number`);
  }
  return value;
}

export function requireBoolean(value: unknown, label: string): boolean {
  if (typeof value !== "boolean") {
    throw new IpcContractError(`${label} must be a boolean`);
  }
  return value;
}

function optionalString(value: unknown, label: string): string | undefined {
  return value === undefined || value === null
    ? undefined
    : requireString(value, label);
}

function optionalNumber(value: unknown, label: string): number | undefined {
  return value === undefined || value === null
    ? undefined
    : requireNumber(value, label);
}
