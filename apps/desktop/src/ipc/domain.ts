import type {
  AgentId,
  DaemonInstanceId,
  ProjectId,
  RequestId,
  SessionId,
  WorktreeId,
} from "./ids";

export const SESSION_STATUSES = [
  "starting",
  "running",
  "idle",
  "exited",
  "failed",
  "unknown",
] as const;

export type SessionStatus = (typeof SESSION_STATUSES)[number];
export type AgentSource = "built_in" | "custom";
export type WorktreeState =
  | "creating"
  | "active"
  | "remove_pending"
  | "orphaned";

export type ApiError = {
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

export type AgentCommand = {
  executable: string;
  args: string[];
  env: Record<string, string>;
};

export type AgentDefinition = {
  id: AgentId;
  displayName: string;
  description?: string;
  source: AgentSource;
  command: CommandSpec;
};

export type AgentRecord = {
  id: AgentId;
  displayName: string;
  description?: string;
  source: AgentSource;
  command: AgentCommand;
  enabled: boolean;
};

export type AgentDetection = {
  agentId: AgentId;
  available: boolean;
  executablePath?: string;
  errorCode?: string;
};

export type Project = {
  id: ProjectId;
  name: string;
  path: string;
  repositoryRoot?: string;
  currentBranch?: string;
  createdAtMs: number;
  lastOpenedAtMs: number;
};

export type Session = {
  id: SessionId;
  projectId: ProjectId;
  name: string;
  agentId: AgentId;
  cwd: string;
  pid?: number;
  ptyId?: string;
  branch?: string;
  worktreeId?: WorktreeId;
  worktreePath?: string;
  status: SessionStatus;
  exitCode?: number;
  createdAtMs: number;
  updatedAtMs: number;
  lastActivityAtMs?: number;
  errorCode?: string;
};

export type Worktree = {
  id: WorktreeId;
  projectId: ProjectId;
  sessionId?: SessionId;
  path: string;
  branch: string;
  isDirty: boolean;
  state: WorktreeState;
  createdAtMs: number;
  updatedAtMs: number;
};

export type EmptyRequest = Record<string, never>;
export type EmptyResponse = Record<string, never>;
export type HelloRequest = EmptyRequest;

export type HelloResponse = {
  protocolVersion: number;
  daemonVersion: string;
  instanceId: DaemonInstanceId;
};

export type StateSnapshotResponse = {
  schemaVersion: number;
  projects: Project[];
  agents: AgentRecord[];
  sessions: Session[];
  worktrees: Worktree[];
};

export type ProjectAddRequest = {
  path: string;
  name?: string;
};

export type ProjectRenameRequest = {
  projectId: ProjectId;
  name: string;
};

export type ProjectRemoveRequest = {
  projectId: ProjectId;
};

export type ProjectListResponse = {
  projects: Project[];
};

export type AgentListResponse = {
  agents: AgentRecord[];
};

export type AgentDetectRequest = {
  agentIds?: AgentId[];
};

export type AgentDetectResponse = {
  detections: AgentDetection[];
};

export type AgentCustomCreateRequest = {
  displayName: string;
  command: AgentCommand;
};

export type AgentCustomUpdateRequest = {
  agentId: AgentId;
  displayName: string;
  command: AgentCommand;
};

export type AgentSetEnabledRequest = {
  agentId: AgentId;
  enabled: boolean;
};

export type AgentCustomRemoveRequest = {
  agentId: AgentId;
};

export type SessionIsolation = "current" | "new_worktree";

export type SessionCreateRequest = {
  projectId: ProjectId;
  name: string;
  agentId: AgentId;
  isolation: SessionIsolation;
  relativeDirectory?: string;
};

export type SessionListRequest = {
  projectId?: ProjectId;
};

export type SessionIdRequest = {
  sessionId: SessionId;
};

export type SessionRenameRequest = SessionIdRequest & {
  name: string;
};

export type SessionWriteRequest = SessionIdRequest & {
  base64: string;
};

export type SessionResizeRequest = SessionIdRequest & {
  columns: number;
  rows: number;
};

export type SessionSubscribeRequest = SessionIdRequest & {
  cursor?: number;
};

export type SessionListResponse = {
  sessions: Session[];
};

export type GitTarget =
  | { kind: "project"; projectId: ProjectId }
  | { kind: "session"; sessionId: SessionId }
  | { kind: "worktree"; worktreeId: WorktreeId };

export type GitStatusRequest = { target: GitTarget };
export type GitDiffRequest = { target: GitTarget; path?: string };
export type GitChangeKind =
  | "modified"
  | "added"
  | "deleted"
  | "untracked"
  | "renamed"
  | "ignored";

export type GitChangedFile = {
  path: string;
  originalPath?: string;
  kind: GitChangeKind;
  staged: boolean;
  unstaged: boolean;
};

export type GitStatusCounts = {
  modified: number;
  added: number;
  deleted: number;
  untracked: number;
  renamed: number;
  ignored: number;
};

export type GitStatusResponse = {
  branch?: string;
  files: GitChangedFile[];
  counts: GitStatusCounts;
  hasStaged: boolean;
  hasTrackedChanges: boolean;
  hasUntracked: boolean;
  isDirty: boolean;
};

export type GitDiffResponse = { text: string; truncated: boolean; binary: boolean };

export type WorktreePrepareRemoveRequest = { worktreeId: WorktreeId };

export type WorktreeRemoveRequest = {
  worktreeId: WorktreeId;
  confirmationToken: string;
};

export type WorktreeRemovalBlocker =
  | "staged_changes"
  | "tracked_changes"
  | "untracked_files"
  | "ignored_files"
  | "assume_unchanged"
  | "skip_worktree"
  | "locked"
  | "running"
  | "in_use"
  | "unknown";

export type WorktreePrepareRemoveResponse =
  | {
      status: "ready";
      worktreeId: WorktreeId;
      confirmationToken: string;
      expiresAtMs: number;
    }
  | {
      status: "blocked";
      worktreeId: WorktreeId;
      isDirty: boolean;
      blockers: WorktreeRemovalBlocker[];
    };

export type DiagnosticIssue = {
  code: string;
  message: string;
  action?: string;
};

export type DiagnosticsResponse = {
  daemonVersion: string;
  protocolVersion: number;
  schemaVersion: number;
  daemonInstanceId: DaemonInstanceId;
  dataPath: string;
  runtimePath: string;
  logPath: string;
  effectivePath: string[];
  recentIssues: DiagnosticIssue[];
};

export type ProjectChangedEvent = { project: Project };
export type ProjectRemovedEvent = { projectId: ProjectId };
export type AgentChangedEvent = { agent: AgentRecord };
export type AgentRemovedEvent = { agentId: AgentId };
export type SessionChangedEvent = { session: Session };
export type SessionDeletedEvent = { sessionId: SessionId };

export type SessionOutputEvent = {
  sessionId: SessionId;
  base64: string;
  outputSequence: number;
  replay: boolean;
};

export type SessionReplayCompleteEvent = {
  sessionId: SessionId;
  outputSequence: number;
};

export type SessionOutputGapEvent = {
  sessionId: SessionId;
  requestedCursor: number;
  firstAvailableSequence: number;
  latestSequence: number;
};

export type SessionStatusChangedEvent = {
  sessionId: SessionId;
  previousStatus: SessionStatus;
  status: SessionStatus;
  changedAtMs: number;
  reasonCode?: string;
};

export type SessionExitedEvent = {
  sessionId: SessionId;
  exitCode?: number;
  status: SessionStatus;
  exitedAtMs: number;
};

export type WorktreeChangedEvent = { worktree: Worktree };
export type WorktreeRemovedEvent = { worktreeId: WorktreeId };

export type GitStatusChangedEvent = {
  target: GitTarget;
  status: GitStatusResponse;
};

export type DaemonShuttingDownEvent = {
  reasonCode: string;
  activeSessionCount: number;
};

export type EnvelopeKind = "request" | "response" | "event";

export type RequestEnvelope<T> = {
  kind: "request";
  version: 1;
  requestId: RequestId;
  method: string;
  payload: T;
};

export type ResponseEnvelope<T> =
  | {
      kind: "response";
      version: 1;
      requestId: RequestId;
      status: "success";
      data: T;
    }
  | {
      kind: "response";
      version: 1;
      requestId: RequestId;
      status: "error";
      error: ApiError;
    };

export type EventEnvelope<T> = {
  kind: "event";
  version: 1;
  event: string;
  sequence: number;
  payload: T;
};
