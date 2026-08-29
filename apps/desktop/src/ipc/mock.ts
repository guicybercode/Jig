import { PROTOCOL_V1, type DaemonEvent, type EventPayloadMap, type IpcClient, type IpcEvent, type IpcMethod, type RequestPayloadMap, type ResponsePayloadMap, type TypedEvent } from "./methods";
import { IpcRequestError, disconnectedError } from "./errors";
import type { ApiError } from "./domain";

type HandlerMap = {
  [M in IpcMethod]?: (
    payload: RequestPayloadMap[M],
  ) => Promise<ResponsePayloadMap[M]> | ResponsePayloadMap[M];
};

type QueuedRequest = {
  method: IpcMethod;
  payload: unknown;
  execute: () => Promise<unknown>;
  resolve: (value: unknown) => void;
  reject: (error: unknown) => void;
};

/** Test double for the project-owned IPC client. Never mocks Tauri APIs. */
export type MockIpcClient = IpcClient & {
  readonly calls: Array<{ method: IpcMethod; payload: unknown }>;
  stall(): void;
  autoResolve(): void;
  readonly queuedCount: number;
  setHandler<M extends IpcMethod>(
    method: M,
    handler: HandlerMap[M],
  ): void;
  emit<E extends IpcEvent>(event: E, payload: EventPayloadMap[E], sequence?: number): void;
  flushNext(): Promise<void>;
  flushLast(): Promise<void>;
  flushMatching(method: IpcMethod): Promise<void>;
};

/** Creates an in-memory client whose handlers tests own. */
export function createMockIpcClient(handlers: HandlerMap = {}): MockIpcClient {
  const resolvedHandlers: HandlerMap = { ...handlers };
  const calls: Array<{ method: IpcMethod; payload: unknown }> = [];
  const listeners = new Set<(event: DaemonEvent) => void>();
  const queue: QueuedRequest[] = [];
  let stalled = false;
  let sequence = 0;

  function executeHandler<M extends IpcMethod>(
    method: M,
    payload: RequestPayloadMap[M],
    handler: HandlerMap[M] | undefined,
  ): Promise<ResponsePayloadMap[M]> {
    if (!handler) {
      return Promise.reject(
        new IpcRequestError({
          ...disconnectedError,
          message: `No mock handler for ${method}.`,
        }),
      );
    }
    return Promise.resolve(
      (
        handler as (
          next: RequestPayloadMap[M],
        ) => Promise<ResponsePayloadMap[M]> | ResponsePayloadMap[M]
      )(payload),
    );
  }

  async function settle(item: QueuedRequest): Promise<void> {
    try {
      item.resolve(await item.execute());
    } catch (error) {
      item.reject(error);
    }
    await Promise.resolve();
  }

  const client: MockIpcClient = {
    calls,
    stall() {
      stalled = true;
    },
    autoResolve() {
      stalled = false;
    },
    get queuedCount() {
      return queue.length;
    },
    setHandler(method, handler) {
      resolvedHandlers[method] = handler;
    },
    emit(event, payload, explicitSequence) {
      sequence += 1;
      const envelope: TypedEvent<typeof event> = {
        kind: "event",
        version: PROTOCOL_V1,
        event,
        sequence: explicitSequence ?? sequence,
        payload,
      };
      for (const listener of listeners) {
        listener(envelope as DaemonEvent);
      }
    },
    async request(method, payload) {
      calls.push({ method, payload });
      const handler = resolvedHandlers[method];
      if (!stalled) {
        return executeHandler(method, payload, handler);
      }
      return new Promise((resolve, reject) => {
        queue.push({
          method,
          payload,
          execute: () => executeHandler(method, payload, handler),
          resolve: resolve as (value: unknown) => void,
          reject,
        });
      });
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    async flushNext() {
      const next = queue.shift();
      if (!next) {
        return;
      }
      await settle(next);
    },
    async flushLast() {
      const next = queue.pop();
      if (!next) {
        return;
      }
      await settle(next);
    },
    async flushMatching(method) {
      const index = queue.findIndex((item) => item.method === method);
      if (index < 0) {
        return;
      }
      const [next] = queue.splice(index, 1);
      await settle(next);
    },
  };

  return client;
}

/** Helper for tests that need a failing method. */
export function rejectWith(error: ApiError): () => never {
  return () => {
    throw new IpcRequestError(error);
  };
}
