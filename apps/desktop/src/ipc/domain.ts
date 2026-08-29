import type {
  AgentId,
  DaemonInstanceId,
  ProjectId,
  SessionId,
  WorktreeId,
} from "./ids";

export const SESSION_STATUSES = [
  "created",
  "starting",
  "running",
  "idle",
  "stopping",
  "exited",
  "failed",
  "unknown",
] as const;

export type SessionStatus = (typeof SESSION_STATUSES)[number];

export const LIVE_SESSION_STATUSES = [
  "starting",
  "running",
  "idle",
  "stopping",
] as const satisfies readonly SessionStatus[];

export type LiveSessionStatus = (typeof LIVE_SESSION_STATUSES)[number];

const SESSION_TRANSITIONS: ReadonlyArray<readonly [SessionStatus, SessionStatus]> =
  [
    ["created", "starting"],
    ["created", "failed"],
    ["starting", "running"],
    ["starting", "failed"],
    ["starting", "stopping"],
    ["running", "idle"],
    ["running", "stopping"],
    ["running", "exited"],
    ["running", "failed"],
    ["idle", "running"],
    ["idle", "stopping"],
    ["idle", "exited"],
    ["idle", "failed"],
    ["stopping", "exited"],
    ["stopping", "failed"],
    ["exited", "starting"],
    ["failed", "starting"],
    ["unknown", "starting"],
    ["unknown", "failed"],
  ];

const SESSION_TRANSITION_SET = new Set(
  SESSION_TRANSITIONS.map(([from, to]) => `${from}->${to}`),
);

/** Returns whether a session may currently own a process group. */
export function isLiveSessionStatus(
  status: SessionStatus,
): status is LiveSessionStatus {
  return (LIVE_SESSION_STATUSES as readonly string[]).includes(status);
}

/** Returns whether `next` is a legal daemon-side status change. */
export function canTransitionSessionStatus(
  from: SessionStatus,
  to: SessionStatus,
): boolean {
  return SESSION_TRANSITION_SET.has(`${from}->${to}`);
}

/** Status after a new daemon instance loads a persisted row. */
export function recoveredSessionStatus(status: SessionStatus): SessionStatus {
  return isLiveSessionStatus(status) ? "unknown" : status;
}

export type AgentSource = "built_in" | "custom";
export type WorktreeState =
  | "creating"
  | "active"
  | "remove_pending"
  | "orphaned";
export type DetectionStatus = "found" | "not_found" | "not_executable";
export type DaemonLifecycle = "starting" | "ready" | "shutting_down";
export type MetadataChange = "added" | "updated" | "removed";

export type ApplicationError = {
  code: string;
  message: string;
  action?: string;
  details?: Record<string, unknown>;
};

export type CommandSpec = {
  executable: string;
  args: string[];
  cwd: string;
  env: Record<string, string>;
};

export type AgentDefinition = {
  id: AgentId;
  displayName: string;
  description?: string;
  source: AgentSource;
  enabled: boolean;
};

export type CustomAgent = {
  id: AgentId;
  displayName: string;
  executable: string;
  args: string[];
  env: Record<string, string>;
  enabled: boolean;
  createdAt: string;
  updatedAt: string;
};

export type AgentDetection = {
  agentId: AgentId;
  status: DetectionStatus;
  executable?: string;
};

export type Project = {
  id: ProjectId;
  name: string;
  path: string;
  createdAt: string;
  lastOpenedAt: string;
};

export type Session = {
  id: SessionId;
  projectId: ProjectId;
  name: string;
  agentId: AgentId;
  cwd: string;
  worktreeId?: WorktreeId;
  status: SessionStatus;
  exitCode?: number;
  errorCode?: string;
  createdAt: string;
  updatedAt: string;
  lastActivityAt?: string;
};

export type Worktree = {
  id: WorktreeId;
  projectId: ProjectId;
  sessionId?: SessionId;
  path: string;
  branch: string;
  state: WorktreeState;
  createdAt: string;
  updatedAt: string;
};

export type GitStatus = {
  projectId: ProjectId;
  worktreeId?: WorktreeId;
  branch?: string;
  upstream?: string;
  ahead: number;
  behind: number;
  changedFiles: number;
  stagedFiles: number;
  untrackedFiles: number;
  isDirty: boolean;
  observedAt: string;
};

export type GitDiff = {
  projectId: ProjectId;
  worktreeId?: WorktreeId;
  text: string;
  truncated: boolean;
};

export type DaemonStatus = {
  instanceId: DaemonInstanceId;
  lifecycle: DaemonLifecycle;
  protocolVersion: number;
  appVersion: string;
  platform: "linux" | "macos";
};

export type EmptyPayload = Record<string, never>;

export type HelloRequest = {
  protocolVersion: number;
  client: string;
};

export type HelloResponse = {
  protocolVersion: number;
  daemonInstanceId: DaemonInstanceId;
  appVersion: string;
  platform: "linux" | "macos";
};

export type StateSnapshot = {
  daemon: DaemonStatus;
  projects: Project[];
  agents: AgentDefinition[];
  customAgents: CustomAgent[];
  sessions: Session[];
  worktrees: Worktree[];
};

export type ProjectAddRequest = {
  path: string;
  name?: string;
};

export type ProjectIdRequest = {
  projectId: ProjectId;
};

export type ProjectRenameRequest = {
  projectId: ProjectId;
  name: string;
};

export type ProjectListResponse = {
  projects: Project[];
};

export type AgentListResponse = {
  agents: AgentDefinition[];
};

export type AgentDetectResponse = {
  detections: AgentDetection[];
};

export type AgentCreateCustomRequest = {
  id?: AgentId;
  displayName: string;
  executable: string;
  args?: string[];
  env?: Record<string, string>;
};

export type AgentUpdateCustomRequest = {
  id: AgentId;
  displayName?: string;
  executable?: string;
  args?: string[];
  env?: Record<string, string>;
  enabled?: boolean;
};

export type AgentDeleteCustomRequest = {
  id: AgentId;
};

export type SessionListRequest = {
  projectId?: ProjectId;
};

export type SessionListResponse = {
  sessions: Session[];
};

export type SessionCreateRequest = {
  projectId: ProjectId;
  agentId: AgentId;
  name: string;
  worktreeId?: WorktreeId;
};

export type SessionIdRequest = {
  sessionId: SessionId;
};

export type SessionWriteRequest = {
  sessionId: SessionId;
  dataBase64: string;
};

export type SessionResizeRequest = {
  sessionId: SessionId;
  cols: number;
  rows: number;
};

export type WorktreeListRequest = {
  projectId: ProjectId;
};

export type WorktreeListResponse = {
  worktrees: Worktree[];
};

export type WorktreeCreateRequest = {
  projectId: ProjectId;
  branch?: string;
  name?: string;
};

export type WorktreeRemoveRequest = {
  worktreeId: WorktreeId;
  confirmationToken?: string;
  allowDirty?: boolean;
};

export type WorktreeRemoveResponse = {
  removed: boolean;
  isDirty: boolean;
  inUse: boolean;
  confirmationToken?: string;
};

export type GitObserveRequest = {
  projectId: ProjectId;
  worktreeId?: WorktreeId;
};

export type DiagnosticsSnapshot = {
  appVersion: string;
  protocolVersion: number;
  platform: "linux" | "macos";
  dataDir: string;
  schemaVersion: number;
  liveSessionCount: number;
};

export type SessionOutputEvent = {
  sessionId: SessionId;
  sequence: number;
  dataBase64: string;
};

export type SessionStatusChangedEvent = {
  sessionId: SessionId;
  from: SessionStatus;
  to: SessionStatus;
  changedAt: string;
  reason?: string;
};

export type SessionExitedEvent = {
  sessionId: SessionId;
  status: SessionStatus;
  exitCode?: number;
  error?: ApplicationError;
};

export type ProjectChangedEvent = {
  change: MetadataChange;
  project?: Project;
  projectId?: ProjectId;
};

export type GitStatusChangedEvent = {
  status: GitStatus;
};

export type DaemonStatusChangedEvent = {
  status: DaemonStatus;
};

export type EnvelopeKind = "request" | "response" | "event";

export type RequestEnvelope<T> = {
  kind: "request";
  version: 1;
  requestId: string;
  method: string;
  payload: T;
};

export type ResponseEnvelope<T> =
  | {
      kind: "response";
      version: 1;
      requestId: string;
      status: "success";
      data: T;
    }
  | {
      kind: "response";
      version: 1;
      requestId: string;
      status: "error";
      error: ApplicationError;
    };

export type EventEnvelope<T> = {
  kind: "event";
  version: 1;
  event: string;
  sequence: number;
  payload: T;
};
