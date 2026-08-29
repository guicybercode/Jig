import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openPath } from "@tauri-apps/plugin-opener";

import {
  IpcContractError,
  decodeAgentDetectResponse,
  decodeAgentRecord,
  decodeApiError,
  decodeHello,
  decodeProject,
  decodeSession,
  decodeSnapshot,
  decodeWorktree,
  requireArray,
  requireBoolean,
  requireNumber,
  requireRecord,
  requireString,
} from "./schema";
import type {
  AddProjectInput,
  AgentRecord,
  ApiErrorData,
  BootstrapResult,
  CreateCustomAgentInput,
  CreateSessionInput,
  DiagnosticsSnapshot,
  EventEnvelope,
  GitStatus,
  GitStatusFile,
  GitTarget,
  Project,
  RemoveWorktreeInput,
  RenameProjectInput,
  RenameSessionInput,
  RequestEnvelope,
  ResponseEnvelope,
  Session,
  SessionIdInput,
  WorktreeRemovalPreparation,
} from "./types";

export type AppPlatform = "macos" | "linux" | "unknown";
export type IpcEventHandler = (event: EventEnvelope) => void;
export type IpcEventErrorHandler = (error: IpcError) => void;
export type Unsubscribe = () => void;

/** The sole frontend interface to daemon and native desktop capabilities. */
export interface IpcClient {
  readonly platform: AppPlatform;
  initialize(): Promise<BootstrapResult>;
  subscribe(
    handler: IpcEventHandler,
    onError: IpcEventErrorHandler,
  ): Promise<Unsubscribe>;
  addProject(input: AddProjectInput): Promise<Project>;
  renameProject(input: RenameProjectInput): Promise<Project>;
  removeProject(projectId: string): Promise<void>;
  createCustomAgent(input: CreateCustomAgentInput): Promise<AgentRecord>;
  createSession(input: CreateSessionInput): Promise<Session>;
  startSession(input: SessionIdInput): Promise<Session>;
  stopSession(input: SessionIdInput): Promise<Session>;
  restartSession(input: SessionIdInput): Promise<Session>;
  renameSession(input: RenameSessionInput): Promise<Session>;
  deleteSession(input: SessionIdInput): Promise<void>;
  getGitStatus(target: GitTarget): Promise<GitStatus>;
  prepareWorktreeRemoval(worktreeId: string): Promise<WorktreeRemovalPreparation>;
  removeWorktree(input: RemoveWorktreeInput): Promise<void>;
  getDiagnostics(): Promise<DiagnosticsSnapshot>;
  openPath(path: string): Promise<void>;
}

/** Error with stable daemon metadata that is safe to render in the UI. */
export class IpcError extends Error implements ApiErrorData {
  readonly code: string;
  readonly action?: string;
  readonly details?: Readonly<Record<string, unknown>>;

  constructor(data: ApiErrorData) {
    super(data.message);
    this.name = "IpcError";
    this.code = data.code;
    this.action = data.action;
    this.details = data.details;
  }
}

/** Converts transport, contract, and daemon failures to one actionable shape. */
export function toIpcError(error: unknown): IpcError {
  if (error instanceof IpcError) {
    return error;
  }
  if (error instanceof IpcContractError) {
    return new IpcError(
      {
        code: error.code,
        message: "Jig received an invalid response from the local daemon.",
        action: "Open Diagnostics and verify that desktop and daemon versions match.",
        details: { reason: error.message },
      },
    );
  }
  if (isApiErrorLike(error)) {
    return new IpcError(decodeApiError(error));
  }
  const message = error instanceof Error ? error.message : String(error);
  return new IpcError(
    {
      code: "daemon_unavailable",
      message: "The local daemon could not be reached.",
      action: "Retry the connection or open Diagnostics for startup details.",
      details: message ? { reason: message } : undefined,
    },
  );
}

class TauriIpcClient implements IpcClient {
  readonly platform = detectPlatform();

  async initialize(): Promise<BootstrapResult> {
    const hello = decodeHello(await this.request("system.hello", {}));
    if (hello.protocolVersion !== 1) {
      throw new IpcError({
        code: "unsupported_protocol_version",
        message: `The daemon uses protocol ${hello.protocolVersion}; this desktop expects protocol 1.`,
        action: "Update Jig so the desktop and daemon versions match.",
      });
    }
    const snapshot = decodeSnapshot(await this.request("state.snapshot", {}));
    const agentIds = snapshot.agents
      .filter((agent) => agent.enabled)
      .map((agent) => agent.id);
    const agentDetections = agentIds.length
      ? decodeAgentDetectResponse(
          await this.request("agent.detect", { agentIds }),
        )
      : [];
    return { hello, snapshot, agentDetections };
  }

  async subscribe(
    handler: IpcEventHandler,
    onError: IpcEventErrorHandler,
  ): Promise<Unsubscribe> {
    const unlisten = await listen<unknown>("daemon:event", ({ payload }) => {
      try {
        const record = requireRecord(payload, "daemon event");
        const kind = requireString(record.kind, "event.kind");
        const version = requireNumber(record.version, "event.version");
        if (kind !== "event" || version !== 1) {
          throw new IpcContractError("Daemon event envelope is incompatible");
        }
        handler({
          kind: "event",
          version: 1,
          event: requireString(record.event, "event.event"),
          sequence: requireNumber(record.sequence, "event.sequence"),
          payload: record.payload,
        });
      } catch (error) {
        onError(toIpcError(error));
      }
    });
    return unlisten;
  }

  async addProject(input: AddProjectInput): Promise<Project> {
    return decodeProject(await this.request("project.add", input));
  }

  async renameProject(input: RenameProjectInput): Promise<Project> {
    return decodeProject(await this.request("project.rename", input));
  }

  async removeProject(projectId: string): Promise<void> {
    await this.request("project.remove", { projectId });
  }

  async createCustomAgent(
    input: CreateCustomAgentInput,
  ): Promise<AgentRecord> {
    return decodeAgentRecord(
      await this.request("agent.custom.create", input),
    );
  }

  async createSession(input: CreateSessionInput): Promise<Session> {
    return decodeSession(await this.request("session.create", input));
  }

  async startSession(input: SessionIdInput): Promise<Session> {
    return decodeSession(await this.request("session.start", input));
  }

  async stopSession(input: SessionIdInput): Promise<Session> {
    return decodeSession(await this.request("session.stop", input));
  }

  async restartSession(input: SessionIdInput): Promise<Session> {
    return decodeSession(await this.request("session.restart", input));
  }

  async renameSession(input: RenameSessionInput): Promise<Session> {
    return decodeSession(await this.request("session.rename", input));
  }

  async deleteSession(input: SessionIdInput): Promise<void> {
    await this.request("session.delete", input);
  }

  async getGitStatus(target: GitTarget): Promise<GitStatus> {
    return decodeGitStatus(await this.request("git.status", { target }));
  }

  async prepareWorktreeRemoval(
    worktreeId: string,
  ): Promise<WorktreeRemovalPreparation> {
    return decodeRemovalPreparation(
      await this.request("worktree.prepare_remove", { worktreeId }),
    );
  }

  async removeWorktree(input: RemoveWorktreeInput): Promise<void> {
    await this.request("worktree.remove", input);
  }

  async getDiagnostics(): Promise<DiagnosticsSnapshot> {
    return decodeDiagnostics(await this.request("diagnostics.get", {}));
  }

  async openPath(path: string): Promise<void> {
    try {
      await openPath(path);
    } catch (error) {
      throw new IpcError({
        code: "path_open_failed",
        message: "The working directory could not be opened.",
        action: "Copy the path and open it manually in your preferred application.",
        details: {
          reason: error instanceof Error ? error.message : String(error),
        },
      });
    }
  }

  private async request(method: string, payload: unknown): Promise<unknown> {
    const request: RequestEnvelope<unknown> = {
      kind: "request",
      version: 1,
      requestId: createRequestId(),
      method,
      payload,
    };
    try {
      const raw = await invoke<unknown>("daemon_request", { request });
      return unwrapResponse(raw, request.requestId);
    } catch (error) {
      throw toIpcError(error);
    }
  }
}

/** Creates a production client; exported so wire-level transport tests stay real. */
export function createTauriIpcClient(): IpcClient {
  return new TauriIpcClient();
}

/** Shared production client; tests inject an `IpcClient` into the provider. */
export const tauriIpcClient: IpcClient = createTauriIpcClient();

function unwrapResponse(value: unknown, requestId: string): unknown {
  const record = requireRecord(value, "daemon response");
  if (
    record.kind !== "response" ||
    record.version !== 1 ||
    record.requestId !== requestId
  ) {
    throw new IpcContractError("Daemon response envelope does not match the request");
  }
  const response = record as unknown as ResponseEnvelope<unknown>;
  if (response.status === "success") {
    return response.data;
  }
  if (response.status === "error") {
    throw new IpcError(decodeApiError(response.error));
  }
  throw new IpcContractError("Daemon response has an unknown status");
}

function decodeGitStatus(value: unknown): GitStatus {
  const record = requireRecord(value, "Git status");
  const counts = requireRecord(record.counts, "Git status counts");
  const files = requireArray(record.files, "Git status files").map((file) => {
    const item = requireRecord(file, "Git status file");
    const kind = requireString(item.kind, "Git status file kind");
    if (!["modified", "added", "deleted", "untracked"].includes(kind)) {
      throw new IpcContractError(`Invalid Git change kind: ${kind}`);
    }
    return {
      path: requireString(item.path, "Git status file path"),
      originalPath:
        item.originalPath === undefined
          ? undefined
          : requireString(item.originalPath, "Git original path"),
      kind: kind as GitStatusFile["kind"],
      staged: requireBoolean(item.staged, "Git staged flag"),
      unstaged: requireBoolean(item.unstaged, "Git unstaged flag"),
    };
  });
  return {
    branch:
      record.branch === undefined
        ? undefined
        : requireString(record.branch, "Git branch"),
    files,
    counts: {
      modified: requireNumber(counts.modified, "modified count"),
      added: requireNumber(counts.added, "added count"),
      deleted: requireNumber(counts.deleted, "deleted count"),
      untracked: requireNumber(counts.untracked, "untracked count"),
    },
    hasStaged: requireBoolean(record.hasStaged, "hasStaged"),
    hasTrackedChanges: requireBoolean(
      record.hasTrackedChanges,
      "hasTrackedChanges",
    ),
    hasUntracked: requireBoolean(record.hasUntracked, "hasUntracked"),
    isDirty: requireBoolean(record.isDirty, "isDirty"),
  };
}

function decodeRemovalPreparation(value: unknown): WorktreeRemovalPreparation {
  const record = requireRecord(value, "worktree removal preparation");
  const status = requireString(record.status, "worktree removal status");
  const worktreeId = requireString(record.worktreeId, "worktreeId");
  if (status === "ready") {
    return {
      status,
      worktreeId,
      confirmationToken: requireString(
        record.confirmationToken,
        "confirmationToken",
      ),
      expiresAtMs: requireNumber(record.expiresAtMs, "expiresAtMs"),
    };
  }
  if (status === "blocked") {
    return {
      status,
      worktreeId,
      isDirty: requireBoolean(record.isDirty, "worktree dirty flag"),
      blockers: requireArray(record.blockers, "worktree blockers").map(
        decodeWorktreeRemovalBlocker,
      ),
    };
  }
  throw new IpcContractError(`Invalid worktree removal status: ${status}`);
}

function decodeDiagnostics(value: unknown): DiagnosticsSnapshot {
  const record = requireRecord(value, "diagnostics");
  return {
    daemonVersion: requireString(record.daemonVersion, "daemonVersion"),
    protocolVersion: requireNumber(record.protocolVersion, "protocolVersion"),
    schemaVersion: requireNumber(record.schemaVersion, "schemaVersion"),
    daemonInstanceId: requireString(
      record.daemonInstanceId,
      "daemonInstanceId",
    ),
    dataPath: requireString(record.dataPath, "dataPath"),
    runtimePath: requireString(record.runtimePath, "runtimePath"),
    logPath: requireString(record.logPath, "logPath"),
    effectivePath: requireArray(record.effectivePath, "effectivePath").map(
      (path) => requireString(path, "effectivePath entry"),
    ),
    recentIssues: requireArray(record.recentIssues, "recentIssues").map(
      (issue) => {
        const item = requireRecord(issue, "diagnostic issue");
        return {
          code: requireString(item.code, "diagnostic issue.code"),
          message: requireString(item.message, "diagnostic issue.message"),
          action:
            item.action === undefined
              ? undefined
              : requireString(item.action, "diagnostic issue.action"),
        };
      },
    ),
  };
}

function decodeWorktreeRemovalBlocker(
  value: unknown,
): import("./types").WorktreeRemovalBlocker {
  const blocker = requireString(value, "worktree blocker");
  switch (blocker) {
    case "staged_changes":
    case "tracked_changes":
    case "untracked_files":
    case "ignored_files":
    case "assume_unchanged":
    case "skip_worktree":
    case "locked":
    case "running":
    case "in_use":
    case "unknown":
      return blocker;
    default:
      return "unknown";
  }
}

function detectPlatform(): AppPlatform {
  const platform = navigator.platform.toLowerCase();
  if (platform.includes("mac")) {
    return "macos";
  }
  if (platform.includes("linux")) {
    return "linux";
  }
  return "unknown";
}

let fallbackRequestSequence = 0;

function createRequestId(): string {
  if (typeof globalThis.crypto?.randomUUID === "function") {
    return globalThis.crypto.randomUUID();
  }
  fallbackRequestSequence += 1;
  const bytes = new Uint8Array(16);
  if (typeof globalThis.crypto?.getRandomValues === "function") {
    globalThis.crypto.getRandomValues(bytes);
  } else {
    let seed = Date.now() ^ fallbackRequestSequence;
    for (let index = 0; index < bytes.length; index += 1) {
      seed = Math.imul(seed ^ (seed >>> 15), 1 | seed);
      bytes[index] = seed & 0xff;
    }
  }
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0"));
  return `${hex.slice(0, 4).join("")}-${hex.slice(4, 6).join("")}-${hex
    .slice(6, 8)
    .join("")}-${hex.slice(8, 10).join("")}-${hex.slice(10).join("")}`;
}

function isApiErrorLike(value: unknown): value is ApiErrorData {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    "message" in value
  );
}

/** Decoders exported for the event router. */
export const eventDecoders = {
  project: decodeProject,
  agent: decodeAgentRecord,
  session: decodeSession,
  worktree: decodeWorktree,
};
