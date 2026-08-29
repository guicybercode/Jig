/** Stable machine-readable error returned by the local daemon. */
export interface ApiErrorData {
  readonly code: string;
  readonly message: string;
  readonly action?: string;
  readonly details?: Readonly<Record<string, unknown>>;
}

/** Shell-free launch configuration exposed by the IPC wire contract. */
export interface AgentCommand {
  readonly executable: string;
  readonly args: readonly string[];
  readonly env: Readonly<Record<string, string>>;
}

export type AgentSource = "built_in" | "custom";

/** Public agent record matching `cli_master_core::wire::AgentRecord`. */
export interface AgentRecord {
  readonly id: string;
  readonly displayName: string;
  readonly description?: string;
  readonly source: AgentSource;
  readonly command: AgentCommand;
  readonly enabled: boolean;
}

/** Result returned by the official `agent.detect` method. */
export interface AgentDetection {
  readonly agentId: string;
  readonly available: boolean;
  readonly executablePath?: string;
  readonly errorCode?: string;
}

export type ProjectAvailability =
  | "available"
  | "missing"
  | "not_repository"
  | "unknown";

/** Public project definition matching `cli_master_core::Project`. */
export interface Project {
  readonly id: string;
  readonly name: string;
  readonly path: string;
  readonly repositoryRoot?: string;
  readonly currentBranch?: string;
  readonly createdAtMs: number;
  readonly lastOpenedAtMs: number;
  /** Forward-compatible repository refresh state when supplied by the daemon. */
  readonly availability?: ProjectAvailability;
  readonly availabilityMessage?: string;
}

export type SessionStatus =
  | "starting"
  | "running"
  | "idle"
  | "exited"
  | "failed"
  | "unknown";

/** Public session definition matching `cli_master_core::Session`. */
export interface Session {
  readonly id: string;
  readonly projectId: string;
  readonly name: string;
  readonly agentId: string;
  readonly cwd: string;
  readonly pid?: number;
  readonly ptyId?: string;
  readonly branch?: string;
  readonly worktreeId?: string;
  readonly worktreePath?: string;
  readonly status: SessionStatus;
  readonly exitCode?: number;
  readonly createdAtMs: number;
  readonly updatedAtMs: number;
  /** Optional v1 metadata populated by lifecycle-aware daemon snapshots. */
  readonly lastActivityAtMs?: number;
  readonly errorCode?: string;
}

/** Public managed worktree definition matching `cli_master_core::Worktree`. */
export interface Worktree {
  readonly id: string;
  readonly projectId: string;
  readonly sessionId?: string;
  readonly path: string;
  readonly branch: string;
  readonly isDirty: boolean;
  readonly state: WorktreeState;
  readonly createdAtMs: number;
  readonly updatedAtMs: number;
}

export type WorktreeState =
  | "creating"
  | "active"
  | "remove_pending"
  | "orphaned";

export interface HelloResponse {
  readonly protocolVersion: number;
  readonly daemonVersion: string;
  readonly instanceId: string;
}

export interface StateSnapshot {
  readonly schemaVersion: number;
  readonly projects: readonly Project[];
  readonly agents: readonly AgentRecord[];
  readonly sessions: readonly Session[];
  readonly worktrees: readonly Worktree[];
}

export interface BootstrapResult {
  readonly hello: HelloResponse;
  readonly snapshot: StateSnapshot;
  /** Detection is a separate official response and is joined by agent ID. */
  readonly agentDetections: readonly AgentDetection[];
}

export interface RequestEnvelope<TPayload> {
  readonly kind: "request";
  readonly version: 1;
  readonly requestId: string;
  readonly method: string;
  readonly payload: TPayload;
}

export type ResponseEnvelope<TData> =
  | {
      readonly kind: "response";
      readonly version: 1;
      readonly requestId: string;
      readonly status: "success";
      readonly data: TData;
    }
  | {
      readonly kind: "response";
      readonly version: 1;
      readonly requestId: string;
      readonly status: "error";
      readonly error: ApiErrorData;
    };

export interface EventEnvelope<TPayload = unknown> {
  readonly kind: "event";
  readonly version: 1;
  readonly event: string;
  readonly sequence: number;
  readonly payload: TPayload;
}

export interface AddProjectInput {
  readonly path: string;
  readonly name?: string;
}

export interface RenameProjectInput {
  readonly projectId: string;
  readonly name: string;
}

export interface CreateCustomAgentInput {
  readonly displayName: string;
  readonly command: {
    readonly executable: string;
    readonly args: readonly string[];
    readonly env: Readonly<Record<string, string>>;
  };
}

export type SessionIsolation = "current" | "new_worktree";

/** Exact daemon-authoritative `SessionCreateRequest` payload. */
export interface CreateSessionInput {
  readonly projectId: string;
  readonly name: string;
  readonly agentId: string;
  readonly isolation: SessionIsolation;
  readonly relativeDirectory?: string;
}

export interface SessionIdInput {
  readonly sessionId: string;
}

export interface RenameSessionInput extends SessionIdInput {
  readonly name: string;
}

export interface GitStatusFile {
  readonly path: string;
  readonly originalPath?: string;
  readonly kind: "modified" | "added" | "deleted" | "untracked";
  readonly staged: boolean;
  readonly unstaged: boolean;
}

export interface GitStatus {
  readonly branch?: string;
  readonly files: readonly GitStatusFile[];
  readonly counts: {
    readonly modified: number;
    readonly added: number;
    readonly deleted: number;
    readonly untracked: number;
  };
  readonly hasStaged: boolean;
  readonly hasTrackedChanges: boolean;
  readonly hasUntracked: boolean;
  readonly isDirty: boolean;
}

/** Exact tagged `GitTarget` wire shape. */
export type GitTarget =
  | { readonly kind: "project"; readonly projectId: string }
  | { readonly kind: "session"; readonly sessionId: string };

export interface DiagnosticIssue {
  readonly code: string;
  readonly message: string;
  readonly action?: string;
}

export interface DiagnosticsSnapshot {
  readonly daemonVersion: string;
  readonly protocolVersion: number;
  readonly schemaVersion: number;
  readonly daemonInstanceId: string;
  readonly dataPath: string;
  readonly runtimePath: string;
  readonly logPath: string;
  readonly effectivePath: readonly string[];
  readonly recentIssues: readonly DiagnosticIssue[];
}

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

export type WorktreeRemovalPreparation =
  | {
      readonly status: "ready";
      readonly worktreeId: string;
      readonly confirmationToken: string;
      readonly expiresAtMs: number;
    }
  | {
      readonly status: "blocked";
      readonly worktreeId: string;
      readonly isDirty: boolean;
      readonly blockers: readonly WorktreeRemovalBlocker[];
    };

export interface RemoveWorktreeInput {
  readonly worktreeId: string;
  readonly confirmationToken: string;
}
