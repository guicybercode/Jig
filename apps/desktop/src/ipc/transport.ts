import { disconnectedError } from "./errors";
import type { IpcMethod, RequestPayloadMap } from "./methods";

/** Lowest-level send/listen boundary. Tests inject a mock instead of Tauri. */
export type IpcTransport = {
  send<M extends IpcMethod>(
    method: M,
    payload: RequestPayloadMap[M],
  ): Promise<unknown>;
  listen(listener: (event: unknown) => void): () => void;
};

/** Always rejects. Used in the browser and when the sidecar is absent. */
export function createDisconnectedTransport(): IpcTransport {
  return {
    async send() {
      throw disconnectedError;
    },
    listen() {
      return () => undefined;
    },
  };
}

type TauriCore = {
  invoke: (command: string, args?: Record<string, unknown>) => Promise<unknown>;
};

type TauriEvent = {
  listen: (
    event: string,
    handler: (event: { payload: unknown }) => void,
  ) => Promise<() => void>;
};

/** Returns whether this document is running inside a Tauri webview. */
export function isTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in globalThis;
}

/**
 * Forwards typed methods through the Tauri bridge.
 *
 * `protocol_info` is intentionally unused: it is not `system.hello`.
 */
export function createTauriTransport(): IpcTransport {
  if (!isTauriRuntime()) {
    return createDisconnectedTransport();
  }

  const listeners = new Set<(event: unknown) => void>();
  let unsubscribePromise: Promise<() => void> | undefined;

  async function core(): Promise<TauriCore> {
    return import("@tauri-apps/api/core") as Promise<TauriCore>;
  }

  return {
    async send(method, payload) {
      const { invoke } = await core();
      return invoke("daemon_invoke", { method, payload });
    },
    listen(listener) {
      listeners.add(listener);
      if (!unsubscribePromise) {
        unsubscribePromise = import("@tauri-apps/api/event")
          .then((api) => {
            const events = api as unknown as TauriEvent;
            return events.listen("daemon-event", (event) => {
              for (const current of listeners) {
                current(event.payload);
              }
            });
          })
          .catch(() => () => undefined);
      }
      return () => {
        listeners.delete(listener);
      };
    },
  };
}
