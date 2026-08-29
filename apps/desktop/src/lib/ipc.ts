export interface ApiError {
  readonly code: string;
  readonly message: string;
  readonly action?: string;
  readonly details?: Record<string, unknown>;
}

export interface DaemonHello {
  readonly protocolVersion: number;
  readonly appVersion: string;
  readonly instanceId: string;
  readonly platform: string;
}

export interface Project {
  readonly id: string;
  readonly name: string;
  readonly path: string;
  readonly repositoryRoot?: string;
  readonly currentBranch?: string;
}

export interface AgentInfo {
  readonly id: string;
  readonly displayName: string;
  readonly source: "built_in" | "custom";
  readonly enabled: boolean;
  readonly detected: boolean;
  readonly executable: string;
  readonly args: string[];
}

export interface Session {
  readonly id: string;
  readonly projectId: string;
  readonly name: string;
  readonly agentId: string;
  readonly cwd: string;
  readonly status: string;
  readonly branch?: string;
  readonly worktreeId?: string;
  readonly worktreePath?: string;
  readonly errorCode?: string;
}

export interface Worktree {
  readonly id: string;
  readonly projectId: string;
  readonly sessionId?: string;
  readonly path: string;
  readonly branch: string;
  readonly state: string;
  readonly isDirty: boolean;
}

export interface GitStatus {
  readonly branch: string;
  readonly isDirty: boolean;
  readonly changedFileCount: number;
  readonly changedFiles: string[];
}

export interface GitDiff {
  readonly text: string;
  readonly truncated: boolean;
}

export interface StateSnapshot {
  readonly daemon: DaemonHello;
  readonly projects: Project[];
  readonly agents: AgentInfo[];
  readonly sessions: Session[];
  readonly worktrees: Worktree[];
}

export interface IpcClient {
  request(method: string, payload?: unknown): Promise<unknown>;
}

export function formatApiError(error: unknown): string {
  if (isApiError(error)) {
    return error.action
      ? `${error.message} ${error.action}`
      : error.message;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return "The local daemon request failed.";
}

export function isApiError(value: unknown): value is ApiError {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    "message" in value &&
    typeof (value as ApiError).code === "string" &&
    typeof (value as ApiError).message === "string"
  );
}

export const disconnectedError: ApiError = {
  code: "DAEMON_UNAVAILABLE",
  message: "The local daemon is not connected.",
  action: "Open CLI Master on this machine so cli-masterd can start.",
};

export async function createTauriClient(): Promise<IpcClient> {
  if (!("__TAURI_INTERNALS__" in window)) {
    return {
      async request() {
        throw disconnectedError;
      },
    };
  }
  const { invoke } = await import("@tauri-apps/api/core");
  return {
    async request(method, payload = {}) {
      return invoke("daemon_request", { method, payload });
    },
  };
}
