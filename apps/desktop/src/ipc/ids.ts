/** Branded identifier so project, session, and worktree IDs cannot be mixed. */
export type Brand<T, B extends string> = T & { readonly __brand: B };

export type ProjectId = Brand<string, "ProjectId">;
export type SessionId = Brand<string, "SessionId">;
export type WorktreeId = Brand<string, "WorktreeId">;
export type RequestId = Brand<string, "RequestId">;
export type DaemonInstanceId = Brand<string, "DaemonInstanceId">;
export type AgentId = Brand<string, "AgentId">;

const UUID_PATTERN =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

function requireNonEmpty(value: string, label: string): string {
  if (value.length === 0) {
    throw new Error(`${label} must not be empty`);
  }
  return value;
}

function requireUuid(value: string, label: string): string {
  const trimmed = requireNonEmpty(value, label);
  if (!UUID_PATTERN.test(trimmed)) {
    throw new Error(`${label} must be a UUID string`);
  }
  return trimmed;
}

/** Parses a project identifier from an untrusted string. */
export function parseProjectId(value: string): ProjectId {
  return requireUuid(value, "project id") as ProjectId;
}

/** Parses a session identifier from an untrusted string. */
export function parseSessionId(value: string): SessionId {
  return requireUuid(value, "session id") as SessionId;
}

/** Parses a worktree identifier from an untrusted string. */
export function parseWorktreeId(value: string): WorktreeId {
  return requireUuid(value, "worktree id") as WorktreeId;
}

/** Parses an IPC request identifier from an untrusted string. */
export function parseRequestId(value: string): RequestId {
  return requireUuid(value, "request id") as RequestId;
}

/** Parses a daemon instance identifier from an untrusted string. */
export function parseDaemonInstanceId(value: string): DaemonInstanceId {
  return requireUuid(value, "daemon instance id") as DaemonInstanceId;
}

/** Parses an agent identifier from an untrusted string. */
export function parseAgentId(value: string): AgentId {
  return requireUuid(value, "agent id") as AgentId;
}
