import {
  parseAgentId,
  parseDaemonInstanceId,
  parseProjectId,
  parseSessionId,
  type AgentRecord,
  type HelloResponse,
  type Project,
  type Session,
  type StateSnapshotResponse,
} from "../ipc";

export const PROJECT_ID = parseProjectId(
  "01900000-0000-7000-8000-000000000010",
);
export const AGENT_ID = parseAgentId("01900000-0000-7000-8000-000000000002");
export const SESSION_ID = parseSessionId(
  "01900000-0000-7000-8000-000000000003",
);
export const INSTANCE_ID = parseDaemonInstanceId(
  "01900000-0000-7000-8000-0000000000aa",
);

/** Canonical `system.hello` payload for UI tests. */
export function helloFixture(
  overrides: Partial<HelloResponse> = {},
): HelloResponse {
  return {
    protocolVersion: 1,
    daemonVersion: "0.1.0",
    instanceId: INSTANCE_ID,
    ...overrides,
  };
}

export function projectFixture(overrides: Partial<Project> = {}): Project {
  return {
    id: PROJECT_ID,
    name: "Demo",
    path: "/tmp/demo",
    currentBranch: "main",
    createdAtMs: 1,
    lastOpenedAtMs: 1,
    ...overrides,
  };
}

export function agentFixture(overrides: Partial<AgentRecord> = {}): AgentRecord {
  return {
    id: AGENT_ID,
    displayName: "Codex",
    source: "built_in",
    enabled: true,
    command: { executable: "codex", args: [], env: {} },
    ...overrides,
  };
}

export function sessionFixture(overrides: Partial<Session> = {}): Session {
  return {
    id: SESSION_ID,
    projectId: PROJECT_ID,
    name: "Demo session",
    agentId: AGENT_ID,
    cwd: "/tmp/demo",
    status: "running",
    createdAtMs: 1,
    updatedAtMs: 1,
    ...overrides,
  };
}

/** Canonical `state.snapshot` payload for UI tests. */
export function snapshotFixture(
  overrides: Partial<StateSnapshotResponse> = {},
): StateSnapshotResponse {
  return {
    schemaVersion: 1,
    projects: [projectFixture()],
    agents: [agentFixture()],
    sessions: [],
    worktrees: [],
    ...overrides,
  };
}
