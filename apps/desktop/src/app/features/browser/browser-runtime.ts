import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";

/** Logical window-relative bounds used by the native child webview. */
export interface BrowserBounds {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

/** Arguments required to activate a browser surface. */
export interface BrowserOpenRequest {
  readonly nodeId: string;
  readonly url: string;
  readonly bounds: BrowserBounds;
  readonly visible: boolean;
}

/** Arguments required to navigate the active browser surface. */
export interface BrowserNavigateRequest {
  readonly nodeId: string;
  readonly url: string;
}

/** Presentation changes for the active native surface. */
export interface BrowserUpdateRequest {
  readonly nodeId: string;
  readonly bounds: BrowserBounds;
  readonly visible: boolean;
}

/** Identifies a browser surface without exposing its native webview label. */
export interface BrowserNodeRequest {
  readonly nodeId: string;
}

/** Events emitted by the native browser host to the trusted main webview. */
export type BrowserRuntimeEvent =
  | {
      readonly type: "location-changed";
      readonly nodeId: string;
      readonly url: string;
    }
  | {
      readonly type: "load-state";
      readonly nodeId: string;
      readonly status: "started" | "finished";
    };

export type BrowserRuntimeListener = (event: BrowserRuntimeEvent) => void;
export type BrowserRuntimeUnsubscribe = () => void;

/** Project-owned boundary around browser-related Tauri APIs. */
export interface BrowserRuntime {
  /** Reports whether native child webviews are available in this process. */
  isAvailable(): boolean;
  /** Activates a node, closing any different surface owned by this runtime. */
  open(request: BrowserOpenRequest): Promise<void>;
  /** Navigates the active node to a validated HTTP(S) URL. */
  navigate(request: BrowserNavigateRequest): Promise<void>;
  /** Synchronizes native bounds and visibility with the DOM reservation. */
  update(request: BrowserUpdateRequest): Promise<void>;
  reload(request: BrowserNodeRequest): Promise<void>;
  goBack(request: BrowserNodeRequest): Promise<void>;
  goForward(request: BrowserNodeRequest): Promise<void>;
  close(request: BrowserNodeRequest): Promise<void>;
  /** Opens a validated URL through the operating system's default browser. */
  openExternal(url: string): Promise<void>;
  /** Optionally observes location and loading events for one canvas node. */
  subscribe?(
    nodeId: string,
    listener: BrowserRuntimeListener,
  ): Promise<BrowserRuntimeUnsubscribe>;
}

/** Narrow injectable facade used to unit-test the runtime without Tauri. */
export interface BrowserRuntimeBridge {
  isTauri(): boolean;
  invoke(command: string, args: Record<string, unknown>): Promise<unknown>;
  listen(
    eventName: string,
    listener: (payload: unknown) => void,
  ): Promise<BrowserRuntimeUnsubscribe>;
  openExternal(url: string): Promise<void>;
}

export type BrowserRuntimeErrorCode =
  | "unavailable"
  | "invalid-node"
  | "invalid-url"
  | "invalid-bounds";

/** Expected runtime failures that are safe for UI code to classify. */
export class BrowserRuntimeError extends Error {
  readonly code: BrowserRuntimeErrorCode;

  constructor(code: BrowserRuntimeErrorCode, message: string) {
    super(message);
    this.name = "BrowserRuntimeError";
    this.code = code;
  }
}

const COMMAND = {
  open: "browser_surface_open",
  navigate: "browser_surface_navigate",
  update: "browser_surface_update",
  reload: "browser_surface_reload",
  goBack: "browser_surface_go_back",
  goForward: "browser_surface_go_forward",
  close: "browser_surface_close",
} as const;

const EVENT = {
  locationChanged: "browser:location-changed",
  loadState: "browser:load-state",
} as const;

const MAX_NODE_ID_LENGTH = 128;
const MAX_URL_LENGTH = 2_048;
const NODE_ID_PATTERN = /^[a-zA-Z0-9_.:-]+$/;

const defaultBridge: BrowserRuntimeBridge = {
  isTauri,
  invoke: (command, args) => invoke(command, args),
  listen: (eventName, listener) =>
    listen<unknown>(eventName, ({ payload }) => listener(payload)),
  openExternal: (url) => openUrl(url),
};

/**
 * Creates the single-surface browser controller.
 *
 * Native operations are serialized so a late cleanup from an old React card
 * cannot close a newly activated card.
 */
export function createBrowserRuntime(
  bridge: BrowserRuntimeBridge = defaultBridge,
): BrowserRuntime {
  let activeNodeId: string | null = null;
  let operationTail: Promise<void> = Promise.resolve();

  function available(): boolean {
    try {
      return bridge.isTauri();
    } catch {
      return false;
    }
  }

  function enqueue(operation: () => Promise<void>): Promise<void> {
    const pending = operationTail.then(operation, operation);
    operationTail = pending.catch(() => undefined);
    return pending;
  }

  function requireAvailable(): void {
    if (!available()) {
      throw new BrowserRuntimeError(
        "unavailable",
        "The integrated browser is available only in the desktop app.",
      );
    }
  }

  async function invokeRequest(
    command: (typeof COMMAND)[keyof typeof COMMAND],
    request: Record<string, unknown>,
  ): Promise<void> {
    await bridge.invoke(command, { request });
  }

  return {
    isAvailable: available,

    async open(request) {
      const nodeId = validateNodeId(request.nodeId);
      const url = validateUrl(request.url);
      const bounds = validateBounds(request.bounds);
      return enqueue(async () => {
        requireAvailable();
        if (activeNodeId && activeNodeId !== nodeId) {
          await invokeRequest(COMMAND.close, { nodeId: activeNodeId });
          activeNodeId = null;
        }
        await invokeRequest(COMMAND.open, {
          nodeId,
          url,
          bounds,
          visible: request.visible,
        });
        activeNodeId = nodeId;
      });
    },

    async navigate(request) {
      const nodeId = validateNodeId(request.nodeId);
      const url = validateUrl(request.url);
      return enqueue(async () => {
        requireAvailable();
        if (activeNodeId !== nodeId) return;
        await invokeRequest(COMMAND.navigate, { nodeId, url });
      });
    },

    async update(request) {
      const nodeId = validateNodeId(request.nodeId);
      const bounds = validateBounds(request.bounds);
      return enqueue(async () => {
        requireAvailable();
        if (activeNodeId !== nodeId) return;
        await invokeRequest(COMMAND.update, {
          nodeId,
          bounds,
          visible: request.visible,
        });
      });
    },

    async reload(request) {
      const nodeId = validateNodeId(request.nodeId);
      return enqueue(async () => {
        requireAvailable();
        if (activeNodeId !== nodeId) return;
        await invokeRequest(COMMAND.reload, { nodeId });
      });
    },

    async goBack(request) {
      const nodeId = validateNodeId(request.nodeId);
      return enqueue(async () => {
        requireAvailable();
        if (activeNodeId !== nodeId) return;
        await invokeRequest(COMMAND.goBack, { nodeId });
      });
    },

    async goForward(request) {
      const nodeId = validateNodeId(request.nodeId);
      return enqueue(async () => {
        requireAvailable();
        if (activeNodeId !== nodeId) return;
        await invokeRequest(COMMAND.goForward, { nodeId });
      });
    },

    async close(request) {
      const nodeId = validateNodeId(request.nodeId);
      return enqueue(async () => {
        requireAvailable();
        if (activeNodeId !== nodeId) return;
        await invokeRequest(COMMAND.close, { nodeId });
        activeNodeId = null;
      });
    },

    async openExternal(value) {
      requireAvailable();
      await bridge.openExternal(validateUrl(value));
    },

    async subscribe(nodeIdValue, listener) {
      const nodeId = validateNodeId(nodeIdValue);
      if (!available()) return () => undefined;

      const unlistenLocation = await bridge.listen(
        EVENT.locationChanged,
        (payload) => {
          const event = parseLocationEvent(payload, nodeId);
          if (event) notifyListener(listener, event);
        },
      );
      try {
        const unlistenLoadState = await bridge.listen(
          EVENT.loadState,
          (payload) => {
            const event = parseLoadStateEvent(payload, nodeId);
            if (event) notifyListener(listener, event);
          },
        );
        let subscribed = true;
        return () => {
          if (!subscribed) return;
          subscribed = false;
          unlistenLocation();
          unlistenLoadState();
        };
      } catch (error) {
        unlistenLocation();
        throw error;
      }
    },
  };
}

/** Shared production controller that enforces one active browser surface. */
export const defaultBrowserRuntime = createBrowserRuntime();

function validateNodeId(value: string): string {
  if (
    value.length === 0 ||
    value.length > MAX_NODE_ID_LENGTH ||
    !NODE_ID_PATTERN.test(value)
  ) {
    throw new BrowserRuntimeError(
      "invalid-node",
      "The browser node identifier is invalid.",
    );
  }
  return value;
}

function validateUrl(value: string): string {
  if (
    value.length === 0 ||
    value.length > MAX_URL_LENGTH ||
    /[\u0000-\u001f\u007f]/.test(value)
  ) {
    throw invalidUrlError();
  }
  try {
    const url = new URL(value);
    if (
      (url.protocol !== "https:" && url.protocol !== "http:") ||
      !url.hostname ||
      url.username ||
      url.password
    ) {
      throw invalidUrlError();
    }
    return url.toString();
  } catch (error) {
    if (error instanceof BrowserRuntimeError) throw error;
    throw invalidUrlError();
  }
}

function invalidUrlError(): BrowserRuntimeError {
  return new BrowserRuntimeError(
    "invalid-url",
    "The browser address must be an HTTP(S) URL without credentials.",
  );
}

function validateBounds(bounds: BrowserBounds): BrowserBounds {
  if (
    !Number.isFinite(bounds.x) ||
    !Number.isFinite(bounds.y) ||
    !Number.isFinite(bounds.width) ||
    !Number.isFinite(bounds.height) ||
    bounds.width <= 0 ||
    bounds.height <= 0
  ) {
    throw new BrowserRuntimeError(
      "invalid-bounds",
      "The browser surface bounds are invalid.",
    );
  }
  return {
    x: bounds.x,
    y: bounds.y,
    width: bounds.width,
    height: bounds.height,
  };
}

function parseLocationEvent(
  payload: unknown,
  expectedNodeId: string,
): BrowserRuntimeEvent | null {
  if (!isRecord(payload) || payload.nodeId !== expectedNodeId) return null;
  if (typeof payload.url !== "string") return null;
  try {
    return {
      type: "location-changed",
      nodeId: expectedNodeId,
      url: validateUrl(payload.url),
    };
  } catch {
    return null;
  }
}

function parseLoadStateEvent(
  payload: unknown,
  expectedNodeId: string,
): BrowserRuntimeEvent | null {
  if (
    !isRecord(payload) ||
    payload.nodeId !== expectedNodeId ||
    (payload.status !== "started" && payload.status !== "finished")
  ) {
    return null;
  }
  return {
    type: "load-state",
    nodeId: expectedNodeId,
    status: payload.status,
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function notifyListener(
  listener: BrowserRuntimeListener,
  event: BrowserRuntimeEvent,
): void {
  try {
    listener(event);
  } catch {
    // Consumer failures must not break Tauri's global event listener.
  }
}
