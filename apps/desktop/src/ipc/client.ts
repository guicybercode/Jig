import { PROTOCOL_V1, isIpcEvent, type DaemonEvent, type IpcClient, type IpcMethod, type RequestPayloadMap, type ResponsePayloadMap } from "./methods";
import { IpcRequestError, toApiError } from "./errors";
import { parseRequestId } from "./ids";
import type { IpcTransport } from "./transport";

function newRequestId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return "00000000-0000-7000-8000-000000000000";
}

function asTypedEvent(value: unknown): DaemonEvent | null {
  if (typeof value !== "object" || value === null) {
    return null;
  }
  const record = value as {
    kind?: unknown;
    version?: unknown;
    event?: unknown;
    sequence?: unknown;
    payload?: unknown;
  };
  if (record.kind !== "event" || record.version !== PROTOCOL_V1) {
    return null;
  }
  if (typeof record.event !== "string" || !isIpcEvent(record.event)) {
    return null;
  }
  if (typeof record.sequence !== "number") {
    return null;
  }
  return {
    kind: "event",
    version: PROTOCOL_V1,
    event: record.event,
    sequence: record.sequence,
    payload: record.payload as DaemonEvent["payload"],
  } as DaemonEvent;
}

/** Wraps a transport in the project-owned typed IPC client. */
export function createIpcClient(transport: IpcTransport): IpcClient {
  return {
    async request<M extends IpcMethod>(
      method: M,
      payload: RequestPayloadMap[M],
    ): Promise<ResponsePayloadMap[M]> {
      parseRequestId(newRequestId());
      try {
        return (await transport.send(method, payload)) as ResponsePayloadMap[M];
      } catch (error) {
        throw new IpcRequestError(toApiError(error));
      }
    },
    subscribe(listener) {
      return transport.listen((raw) => {
        const event = asTypedEvent(raw);
        if (event) {
          listener(event);
        }
      });
    },
  };
}
