import type {
  AgentChangedEvent,
  AgentCustomCreateRequest,
  AgentCustomRemoveRequest,
  AgentCustomUpdateRequest,
  AgentDetectRequest,
  AgentDetectResponse,
  AgentListResponse,
  AgentRecord,
  AgentRemovedEvent,
  AgentSetEnabledRequest,
  ApiError,
  DaemonShuttingDownEvent,
  DiagnosticsResponse,
  EmptyRequest,
  EmptyResponse,
  GitDiffRequest,
  GitDiffResponse,
  GitStatusChangedEvent,
  GitStatusRequest,
  GitStatusResponse,
  HelloRequest,
  HelloResponse,
  Project,
  ProjectAddRequest,
  ProjectChangedEvent,
  ProjectListResponse,
  ProjectRemoveRequest,
  ProjectRemovedEvent,
  ProjectRenameRequest,
  Session,
  SessionChangedEvent,
  SessionCreateRequest,
  SessionDeletedEvent,
  SessionExitedEvent,
  SessionIdRequest,
  SessionListRequest,
  SessionListResponse,
  SessionOutputEvent,
  SessionOutputGapEvent,
  SessionRenameRequest,
  SessionReplayCompleteEvent,
  SessionResizeRequest,
  SessionStatusChangedEvent,
  SessionSubscribeRequest,
  SessionWriteRequest,
  StateSnapshotResponse,
  WorktreeChangedEvent,
  WorktreePrepareRemoveRequest,
  WorktreePrepareRemoveResponse,
  WorktreeRemoveRequest,
  WorktreeRemovedEvent,
} from "./domain";
import type { RequestId } from "./ids";

/** Protocol major version spoken by this client. */
export const PROTOCOL_V1 = 1;

/** Beta v1 request catalog, in the authoritative Rust contract order. */
export const IPC_METHODS = [
  "system.hello",
  "state.snapshot",
  "project.add",
  "project.list",
  "project.rename",
  "project.remove",
  "agent.list",
  "agent.detect",
  "agent.set_enabled",
  "agent.custom.create",
  "agent.custom.update",
  "agent.custom.remove",
  "session.create",
  "session.list",
  "session.rename",
  "session.start",
  "session.restart",
  "session.stop",
  "session.delete",
  "session.write",
  "session.resize",
  "session.subscribe",
  "session.unsubscribe",
  "git.status",
  "git.diff",
  "worktree.prepare_remove",
  "worktree.remove",
  "diagnostics.get",
] as const;

export type IpcMethod = (typeof IPC_METHODS)[number];

/** Beta v1 event catalog, in the authoritative Rust contract order. */
export const IPC_EVENTS = [
  "project.updated",
  "project.removed",
  "agent.updated",
  "agent.removed",
  "session.created",
  "session.updated",
  "session.deleted",
  "session.output",
  "session.replay_complete",
  "session.output_gap",
  "session.status_changed",
  "session.exited",
  "worktree.updated",
  "worktree.removed",
  "git.status_changed",
  "daemon.shutting_down",
] as const;

export type IpcEvent = (typeof IPC_EVENTS)[number];

export type RequestPayloadMap = {
  "system.hello": HelloRequest;
  "state.snapshot": EmptyRequest;
  "project.add": ProjectAddRequest;
  "project.list": EmptyRequest;
  "project.rename": ProjectRenameRequest;
  "project.remove": ProjectRemoveRequest;
  "agent.list": EmptyRequest;
  "agent.detect": AgentDetectRequest;
  "agent.set_enabled": AgentSetEnabledRequest;
  "agent.custom.create": AgentCustomCreateRequest;
  "agent.custom.update": AgentCustomUpdateRequest;
  "agent.custom.remove": AgentCustomRemoveRequest;
  "session.create": SessionCreateRequest;
  "session.list": SessionListRequest;
  "session.rename": SessionRenameRequest;
  "session.start": SessionIdRequest;
  "session.restart": SessionIdRequest;
  "session.stop": SessionIdRequest;
  "session.delete": SessionIdRequest;
  "session.write": SessionWriteRequest;
  "session.resize": SessionResizeRequest;
  "session.subscribe": SessionSubscribeRequest;
  "session.unsubscribe": SessionIdRequest;
  "git.status": GitStatusRequest;
  "git.diff": GitDiffRequest;
  "worktree.prepare_remove": WorktreePrepareRemoveRequest;
  "worktree.remove": WorktreeRemoveRequest;
  "diagnostics.get": EmptyRequest;
};

export type ResponsePayloadMap = {
  "system.hello": HelloResponse;
  "state.snapshot": StateSnapshotResponse;
  "project.add": Project;
  "project.list": ProjectListResponse;
  "project.rename": Project;
  "project.remove": EmptyResponse;
  "agent.list": AgentListResponse;
  "agent.detect": AgentDetectResponse;
  "agent.set_enabled": AgentRecord;
  "agent.custom.create": AgentRecord;
  "agent.custom.update": AgentRecord;
  "agent.custom.remove": EmptyResponse;
  "session.create": Session;
  "session.list": SessionListResponse;
  "session.rename": Session;
  "session.start": Session;
  "session.restart": Session;
  "session.stop": Session;
  "session.delete": EmptyResponse;
  "session.write": EmptyResponse;
  "session.resize": EmptyResponse;
  "session.subscribe": EmptyResponse;
  "session.unsubscribe": EmptyResponse;
  "git.status": GitStatusResponse;
  "git.diff": GitDiffResponse;
  "worktree.prepare_remove": WorktreePrepareRemoveResponse;
  "worktree.remove": EmptyResponse;
  "diagnostics.get": DiagnosticsResponse;
};

export type EventPayloadMap = {
  "project.updated": ProjectChangedEvent;
  "project.removed": ProjectRemovedEvent;
  "agent.updated": AgentChangedEvent;
  "agent.removed": AgentRemovedEvent;
  "session.created": SessionChangedEvent;
  "session.updated": SessionChangedEvent;
  "session.deleted": SessionDeletedEvent;
  "session.output": SessionOutputEvent;
  "session.replay_complete": SessionReplayCompleteEvent;
  "session.output_gap": SessionOutputGapEvent;
  "session.status_changed": SessionStatusChangedEvent;
  "session.exited": SessionExitedEvent;
  "worktree.updated": WorktreeChangedEvent;
  "worktree.removed": WorktreeRemovedEvent;
  "git.status_changed": GitStatusChangedEvent;
  "daemon.shutting_down": DaemonShuttingDownEvent;
};

export type TypedRequest<M extends IpcMethod> = {
  kind: "request";
  version: typeof PROTOCOL_V1;
  requestId: RequestId;
  method: M;
  payload: RequestPayloadMap[M];
};

export type TypedSuccess<M extends IpcMethod> = {
  kind: "response";
  version: typeof PROTOCOL_V1;
  requestId: RequestId;
  status: "success";
  data: ResponsePayloadMap[M];
};

export type TypedFailure = {
  kind: "response";
  version: typeof PROTOCOL_V1;
  requestId: RequestId;
  status: "error";
  error: ApiError;
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

/** Returns whether `value` is a Beta v1 request method. */
export function isIpcMethod(value: string): value is IpcMethod {
  return METHOD_SET.has(value);
}

/** Returns whether `value` is a Beta v1 daemon event. */
export function isIpcEvent(value: string): value is IpcEvent {
  return EVENT_SET.has(value);
}

/** Typed daemon IPC boundary used by the desktop bridge. */
export type IpcClient = {
  request<M extends IpcMethod>(
    method: M,
    payload: RequestPayloadMap[M],
  ): Promise<ResponsePayloadMap[M]>;
  subscribe(listener: (event: TypedEvent<IpcEvent>) => void): () => void;
};
