export type SessionStatus =
  | "starting"
  | "running"
  | "idle"
  | "exited"
  | "failed"
  | "unknown";

export interface ApiError {
  code: string;
  message: string;
  action?: string;
  details?: Record<string, unknown>;
}

export interface ProjectView {
  id: string;
  name: string;
  path: string;
}

export interface SessionView {
  id: string;
  projectId: string;
  name: string;
  status: SessionStatus;
  agentName: string;
}

export interface TerminalView {
  sessionId: string;
  name: string;
}
