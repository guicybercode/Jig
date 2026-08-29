import type {
  AgentCreateCustomRequest,
  AgentDeleteCustomRequest,
  AgentDetectResponse,
  AgentListResponse,
  AgentUpdateCustomRequest,
  CustomAgent,
  DaemonStatusChangedEvent,
  DiagnosticsSnapshot,
  EmptyPayload,
  GitDiff,
  GitObserveRequest,
  GitStatus,
  GitStatusChangedEvent,
  HelloRequest,
  HelloResponse,
  Project,
  ProjectAddRequest,
  ProjectChangedEvent,
  ProjectIdRequest,
  ProjectListResponse,
  ProjectRenameRequest,
  Session,
  SessionCreateRequest,
  SessionExitedEvent,
  SessionIdRequest,
  SessionListRequest,
  SessionListResponse,
  SessionOutputEvent,
  SessionResizeRequest,
  SessionStatusChangedEvent,
  SessionWriteRequest,
  StateSnapshot,
  Worktree,
  WorktreeCreateRequest,
  WorktreeListRequest,
  WorktreeListResponse,
  WorktreeRemoveRequest,
  WorktreeRemoveResponse,
} from "./domain";

/** Protocol major version spoken by this client. */
export const PROTOCOL_V1 = 1;

/** Sorted v1 request catalog. Keep in sync with `protocol/catalog.json`. */
export const IPC_METHODS = [
  "agent.create_custom",
  "agent.delete_custom",
  "agent.detect",
  "agent.list",
  "agent.update_custom",
  "diagnostics.get",
  "git.diff",
  "git.status",
  "project.add",
  "project.list",
  "project.remove",
  "project.rename",
  "session.create",
  "session.delete",
  "session.list",
  "session.resize",
  "session.restart",
  "session.start",
  "session.stop",
  "session.write",
  "state.snapshot",
  "system.hello",
  "worktree.create",
  "worktree.list",
  "worktree.remove",
] as const;

export type IpcMethod = (typeof IPC_METHODS)[number];

/** Sorted v1 event catalog. Keep in sync with `protocol/catalog.json`. */
export const IPC_EVENTS = [
  "daemon.status_changed",
  "git.status_changed",
  "project.changed",
  "session.exited",
  "session.output",
  "session.status_changed",
] as const;

export type IpcEvent = (typeof IPC_EVENTS)[number];

export type RequestPayloadMap = {
  "system.hello": HelloRequest;
  "state.snapshot": EmptyPayload;
  "project.list": EmptyPayload;
  "project.add": ProjectAddRequest;
  "project.remove": ProjectIdRequest;
  "project.rename": ProjectRenameRequest;
  "agent.list": EmptyPayload;
  "agent.detect": EmptyPayload;
  "agent.create_custom": AgentCreateCustomRequest;
  "agent.update_custom": AgentUpdateCustomRequest;
  "agent.delete_custom": AgentDeleteCustomRequest;
  "session.list": SessionListRequest;
  "session.create": SessionCreateRequest;
  "session.start": SessionIdRequest;
  "session.write": SessionWriteRequest;
  "session.resize": SessionResizeRequest;
  "session.stop": SessionIdRequest;
  "session.restart": SessionIdRequest;
  "session.delete": SessionIdRequest;
  "worktree.list": WorktreeListRequest;
  "worktree.create": WorktreeCreateRequest;
  "worktree.remove": WorktreeRemoveRequest;
  "git.status": GitObserveRequest;
  "git.diff": GitObserveRequest;
  "diagnostics.get": EmptyPayload;
};

export type ResponsePayloadMap = {
  "system.hello": HelloResponse;
  "state.snapshot": StateSnapshot;
  "project.list": ProjectListResponse;
  "project.add": Project;
  "project.remove": EmptyPayload;
  "project.rename": Project;
  "agent.list": AgentListResponse;
  "agent.detect": AgentDetectResponse;
  "agent.create_custom": CustomAgent;
  "agent.update_custom": CustomAgent;
  "agent.delete_custom": EmptyPayload;
  "session.list": SessionListResponse;
  "session.create": Session;
  "session.start": Session;
  "session.write": EmptyPayload;
  "session.resize": EmptyPayload;
  "session.stop": Session;
  "session.restart": Session;
  "session.delete": EmptyPayload;
  "worktree.list": WorktreeListResponse;
  "worktree.create": Worktree;
  "worktree.remove": WorktreeRemoveResponse;
  "git.status": GitStatus;
  "git.diff": GitDiff;
  "diagnostics.get": DiagnosticsSnapshot;
};

export type EventPayloadMap = {
  "session.output": SessionOutputEvent;
  "session.status_changed": SessionStatusChangedEvent;
  "session.exited": SessionExitedEvent;
  "project.changed": ProjectChangedEvent;
  "git.status_changed": GitStatusChangedEvent;
  "daemon.status_changed": DaemonStatusChangedEvent;
};

export type TypedRequest<M extends IpcMethod> = {
  kind: "request";
  version: typeof PROTOCOL_V1;
  requestId: string;
  method: M;
  payload: RequestPayloadMap[M];
};

export type TypedSuccess<M extends IpcMethod> = {
  kind: "response";
  version: typeof PROTOCOL_V1;
  requestId: string;
  status: "success";
  data: ResponsePayloadMap[M];
};

export type TypedFailure = {
  kind: "response";
  version: typeof PROTOCOL_V1;
  requestId: string;
  status: "error";
  error: import("./domain").ApplicationError;
};

export type TypedEvent<E extends IpcEvent> = {
  kind: "event";
  version: typeof PROTOCOL_V1;
  event: E;
  sequence: number;
  payload: EventPayloadMap[E];
};

const METHOD_SET = new Set<string>(IPC_METHODS);
const EVENT_SET = new Set<string>(IPC_EVENTS);

type AssertComplete<T extends Record<IpcMethod, unknown>> = T;
type AssertEventsComplete<T extends Record<IpcEvent, unknown>> = T;

export type CompleteRequestMap = AssertComplete<RequestPayloadMap>;
export type CompleteResponseMap = AssertComplete<ResponsePayloadMap>;
export type CompleteEventMap = AssertEventsComplete<EventPayloadMap>;

/** Returns whether `value` is a v1 request method. */
export function isIpcMethod(value: string): value is IpcMethod {
  return METHOD_SET.has(value);
}

/** Returns whether `value` is a v1 daemon event. */
export function isIpcEvent(value: string): value is IpcEvent {
  return EVENT_SET.has(value);
}

/**
 * Desktop IPC client contract. The Tauri bridge forwards these methods to
 * `cli-masterd`. React code must not spawn processes or open SQLite.
 */
export type IpcClient = {
  request<M extends IpcMethod>(
    method: M,
    payload: RequestPayloadMap[M],
  ): Promise<ResponsePayloadMap[M]>;
  subscribe(listener: (event: TypedEvent<IpcEvent>) => void): () => void;
};
