import type {
  AgentDetection,
  AgentId,
  AgentRecord,
  DaemonInstanceId,
  DiagnosticsResponse,
  GitDiffResponse,
  GitStatusResponse,
  GitTarget,
  Project,
  ProjectId,
  Session,
  SessionId,
  SessionIsolation,
  StateSnapshotResponse,
  Worktree,
  WorktreeId,
  WorktreePrepareRemoveResponse,
} from "../../ipc";

/** High-level daemon reachability shown in the shell. */
export type ConnectionPhase =
  | "loading"
  | "disconnected"
  | "incompatible"
  | "error"
  | "ready";

export type NotificationKind = "info" | "warning" | "error";

export type DialogName = "newSession" | "customAgent";

export type ConnectionState = {
  readonly phase: ConnectionPhase;
  readonly message: string | null;
  readonly protocolVersion: number | null;
  readonly daemonVersion: string | null;
  readonly instanceId: DaemonInstanceId | null;
};

export type SelectionState = {
  readonly projectId: ProjectId | null;
  readonly sessionId: SessionId | null;
  readonly visibleSessionIds: readonly SessionId[];
};

export type DialogState = {
  readonly newSession: boolean;
  readonly customAgent: boolean;
};

export type Notification = {
  readonly id: string;
  readonly kind: NotificationKind;
  readonly message: string;
};

export type PendingState = {
  readonly creatingProject: boolean;
  readonly creatingSession: boolean;
  readonly creatingAgent: boolean;
};

export type GitViewState = {
  readonly status: GitStatusResponse | null;
  readonly diff: GitDiffResponse | null;
  readonly error: string | null;
  readonly loading: boolean;
};

/** React-visible workspace metadata. PTY bytes are never stored here. */
export type WorkspaceState = {
  readonly connection: ConnectionState;
  readonly snapshot: StateSnapshotResponse | null;
  readonly snapshotGeneration: number;
  readonly detections: readonly AgentDetection[];
  readonly selection: SelectionState;
  readonly dialogs: DialogState;
  readonly notifications: readonly Notification[];
  readonly pending: PendingState;
  readonly git: GitViewState;
  readonly optimisticProjects: readonly Project[];
};

export type CreateSessionInput = {
  readonly agentId: AgentId;
  readonly name: string;
  readonly isolation: SessionIsolation;
};

export type CreateCustomAgentInput = {
  readonly displayName: string;
  readonly executable: string;
  readonly args: readonly string[];
};

export type WorkspaceActions = {
  refresh(): Promise<void>;
  reconnect(): Promise<void>;
  addProject(path: string, name?: string): Promise<void>;
  removeProject(projectId: ProjectId): Promise<void>;
  selectProject(projectId: ProjectId | null): void;
  createCustomAgent(input: CreateCustomAgentInput): Promise<void>;
  createSession(input: CreateSessionInput): Promise<void>;
  stopSession(sessionId: SessionId): Promise<void>;
  deleteSession(sessionId: SessionId): Promise<void>;
  focusSession(sessionId: SessionId): void;
  toggleVisible(sessionId: SessionId): void;
  writeSession(sessionId: SessionId, base64: string): Promise<void>;
  resizeSession(
    sessionId: SessionId,
    columns: number,
    rows: number,
  ): Promise<void>;
  subscribeSession(sessionId: SessionId, cursor?: number): Promise<void>;
  getDiagnostics(): Promise<DiagnosticsResponse>;
  inspectGit(target: GitTarget): Promise<void>;
  prepareRemoveWorktree(
    worktreeId: WorktreeId,
  ): Promise<WorktreePrepareRemoveResponse>;
  removeWorktree(
    worktreeId: WorktreeId,
    confirmationToken: string,
  ): Promise<void>;
  openDialog(name: DialogName): void;
  closeDialog(name: DialogName): void;
  dismissNotification(id: string): void;
};

export const EMPTY_PROJECTS: Project[] = [];
export const EMPTY_AGENTS: AgentRecord[] = [];
export const EMPTY_SESSIONS: Session[] = [];
export const EMPTY_WORKTREES: Worktree[] = [];
export const EMPTY_SESSION_IDS: SessionId[] = [];
export const EMPTY_DETECTIONS: AgentDetection[] = [];
export const EMPTY_NOTIFICATIONS: Notification[] = [];

export const INITIAL_DIALOGS: DialogState = {
  newSession: false,
  customAgent: false,
};

export const INITIAL_PENDING: PendingState = {
  creatingProject: false,
  creatingSession: false,
  creatingAgent: false,
};

export const INITIAL_GIT: GitViewState = {
  status: null,
  diff: null,
  error: null,
  loading: false,
};

/** Builds the default shell state for a given connection phase. */
export function createInitialWorkspaceState(
  phase: ConnectionPhase = "disconnected",
): WorkspaceState {
  return {
    connection: {
      phase,
      message: null,
      protocolVersion: null,
      daemonVersion: null,
      instanceId: null,
    },
    snapshot: null,
    snapshotGeneration: 0,
    detections: EMPTY_DETECTIONS,
    selection: {
      projectId: null,
      sessionId: null,
      visibleSessionIds: EMPTY_SESSION_IDS,
    },
    dialogs: INITIAL_DIALOGS,
    notifications: EMPTY_NOTIFICATIONS,
    pending: INITIAL_PENDING,
    git: INITIAL_GIT,
    optimisticProjects: EMPTY_PROJECTS,
  };
}
