import type { SessionView } from "./types";

export interface CreateSessionInput {
  projectId: string;
  name: string;
  agentId: string;
  isolateWorktree: boolean;
}

export interface IpcClient {
  createSession(input: CreateSessionInput): Promise<SessionView>;
  subscribe(sessionId: string): Promise<void>;
  unsubscribe(sessionId: string): Promise<void>;
  write(sessionId: string, data: string): Promise<void>;
  stopSession(sessionId: string): Promise<void>;
}

export class RecordingIpcClient implements IpcClient {
  readonly calls: string[] = [];
  readonly createdSessions: CreateSessionInput[] = [];

  async createSession(input: CreateSessionInput): Promise<SessionView> {
    this.calls.push(`createSession:${input.name}`);
    this.createdSessions.push({ ...input });
    return {
      id: "session-created",
      projectId: input.projectId,
      name: input.name,
      status: "starting",
      agentName: input.agentId,
    };
  }

  async subscribe(sessionId: string): Promise<void> {
    this.calls.push(`subscribe:${sessionId}`);
  }

  async unsubscribe(sessionId: string): Promise<void> {
    this.calls.push(`unsubscribe:${sessionId}`);
  }

  async write(sessionId: string, data: string): Promise<void> {
    this.calls.push(`write:${sessionId}:${data}`);
  }

  async stopSession(sessionId: string): Promise<void> {
    this.calls.push(`stop:${sessionId}`);
  }
}
