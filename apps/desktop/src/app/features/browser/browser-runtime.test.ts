import { describe, expect, it, vi } from "vitest";

import {
  createBrowserRuntime,
  type BrowserRuntimeBridge,
  type BrowserRuntimeListener,
} from "./browser-runtime";

const FIRST_BOUNDS = { x: 12, y: 24, width: 640, height: 360 } as const;
const SECOND_BOUNDS = { x: -20, y: 40, width: 720, height: 420 } as const;

describe("browser runtime", () => {
  it("serializes the native command contract and keeps one active surface", async () => {
    const harness = createBridgeHarness();
    const runtime = createBrowserRuntime(harness.bridge);

    await runtime.open({
      nodeId: "browser-one",
      url: "https://example.com/",
      bounds: FIRST_BOUNDS,
      visible: true,
    });
    await runtime.navigate({
      nodeId: "browser-one",
      url: "https://example.com/docs",
    });
    await runtime.update({
      nodeId: "browser-one",
      bounds: SECOND_BOUNDS,
      visible: false,
    });
    await runtime.reload({ nodeId: "browser-one" });
    await runtime.goBack({ nodeId: "browser-one" });
    await runtime.goForward({ nodeId: "browser-one" });
    await runtime.open({
      nodeId: "browser-two",
      url: "https://openai.com/",
      bounds: FIRST_BOUNDS,
      visible: true,
    });
    await runtime.close({ nodeId: "browser-one" });
    await runtime.close({ nodeId: "browser-two" });

    expect(harness.invoke.mock.calls.map(([command]) => command)).toEqual([
      "browser_surface_open",
      "browser_surface_navigate",
      "browser_surface_update",
      "browser_surface_reload",
      "browser_surface_go_back",
      "browser_surface_go_forward",
      "browser_surface_close",
      "browser_surface_open",
      "browser_surface_close",
    ]);
    expect(harness.invoke).toHaveBeenNthCalledWith(1, "browser_surface_open", {
      request: {
        nodeId: "browser-one",
        url: "https://example.com/",
        bounds: FIRST_BOUNDS,
        visible: true,
      },
    });
    expect(harness.invoke).toHaveBeenCalledWith("browser_surface_update", {
      request: {
        nodeId: "browser-one",
        bounds: SECOND_BOUNDS,
        visible: false,
      },
    });
    expect(harness.invoke).toHaveBeenNthCalledWith(7, "browser_surface_close", {
      request: { nodeId: "browser-one" },
    });
  });

  it("rejects unavailable native operations without opening a web fallback", async () => {
    const harness = createBridgeHarness(false);
    const runtime = createBrowserRuntime(harness.bridge);
    const windowOpen = vi.spyOn(window, "open");

    expect(runtime.isAvailable()).toBe(false);
    await expect(
      runtime.open({
        nodeId: "browser-web",
        url: "https://example.com/",
        bounds: FIRST_BOUNDS,
        visible: true,
      }),
    ).rejects.toMatchObject({
      code: "unavailable",
    });
    await expect(runtime.openExternal("https://example.com/")).rejects.toThrow(
      "desktop app",
    );
    const unsubscribe = await runtime.subscribe?.("browser-web", vi.fn());
    unsubscribe?.();

    expect(harness.invoke).not.toHaveBeenCalled();
    expect(harness.listen).not.toHaveBeenCalled();
    expect(harness.openExternal).not.toHaveBeenCalled();
    expect(windowOpen).not.toHaveBeenCalled();
  });

  it("validates node ids, URLs, and bounds before crossing the bridge", async () => {
    const harness = createBridgeHarness();
    const runtime = createBrowserRuntime(harness.bridge);

    await expect(
      runtime.open({
        nodeId: "bad node",
        url: "https://example.com/",
        bounds: FIRST_BOUNDS,
        visible: true,
      }),
    ).rejects.toMatchObject({
      code: "invalid-node",
    });
    await expect(
      runtime.open({
        nodeId: "browser-safe",
        url: "https://user:secret@example.com/",
        bounds: FIRST_BOUNDS,
        visible: true,
      }),
    ).rejects.toMatchObject({
      code: "invalid-url",
    });
    await expect(
      runtime.open({
        nodeId: "browser-safe",
        url: "https://example.com/",
        bounds: { ...FIRST_BOUNDS, width: 0 },
        visible: true,
      }),
    ).rejects.toMatchObject({
      code: "invalid-bounds",
    });

    expect(harness.invoke).not.toHaveBeenCalled();
  });

  it("filters native events by node and disposes both subscriptions once", async () => {
    const harness = createBridgeHarness();
    const runtime = createBrowserRuntime(harness.bridge);
    const listener = vi.fn<BrowserRuntimeListener>();

    const unsubscribe = await runtime.subscribe?.("browser-one", listener);
    harness.emit("browser:location-changed", {
      nodeId: "browser-two",
      url: "https://ignored.example/",
    });
    harness.emit("browser:location-changed", {
      nodeId: "browser-one",
      url: "file:///etc/passwd",
    });
    harness.emit("browser:load-state", {
      nodeId: "browser-one",
      status: "unknown",
    });
    harness.emit("browser:location-changed", {
      nodeId: "browser-one",
      url: "https://example.com/docs",
    });
    harness.emit("browser:load-state", {
      nodeId: "browser-one",
      status: "started",
    });
    harness.emit("browser:load-state", {
      nodeId: "browser-one",
      status: "finished",
    });

    expect(listener.mock.calls).toEqual([
      [
        {
          type: "location-changed",
          nodeId: "browser-one",
          url: "https://example.com/docs",
        },
      ],
      [{ type: "load-state", nodeId: "browser-one", status: "started" }],
      [{ type: "load-state", nodeId: "browser-one", status: "finished" }],
    ]);

    unsubscribe?.();
    unsubscribe?.();
    expect(harness.unlistenByEvent.get("browser:location-changed")).toHaveBeenCalledOnce();
    expect(harness.unlistenByEvent.get("browser:load-state")).toHaveBeenCalledOnce();
  });

  it("opens only safe canonical URLs through the injected system opener", async () => {
    const harness = createBridgeHarness();
    const runtime = createBrowserRuntime(harness.bridge);

    await runtime.openExternal("https://example.com/docs");
    await expect(
      runtime.openExternal("javascript:alert(1)"),
    ).rejects.toMatchObject({ code: "invalid-url" });

    expect(harness.openExternal).toHaveBeenCalledOnce();
    expect(harness.openExternal).toHaveBeenCalledWith(
      "https://example.com/docs",
    );
  });
});

interface BridgeHarness {
  readonly bridge: BrowserRuntimeBridge;
  readonly invoke: ReturnType<typeof vi.fn<BrowserRuntimeBridge["invoke"]>>;
  readonly listen: ReturnType<typeof vi.fn<BrowserRuntimeBridge["listen"]>>;
  readonly openExternal: ReturnType<
    typeof vi.fn<BrowserRuntimeBridge["openExternal"]>
  >;
  readonly unlistenByEvent: Map<string, ReturnType<typeof vi.fn>>;
  emit(eventName: string, payload: unknown): void;
}

function createBridgeHarness(available = true): BridgeHarness {
  const listeners = new Map<string, Set<(payload: unknown) => void>>();
  const unlistenByEvent = new Map<string, ReturnType<typeof vi.fn>>();
  const invoke = vi.fn<BrowserRuntimeBridge["invoke"]>(async () => undefined);
  const openExternal = vi.fn<BrowserRuntimeBridge["openExternal"]>(
    async () => undefined,
  );
  const listen = vi.fn<BrowserRuntimeBridge["listen"]>(
    async (eventName, listener) => {
      const eventListeners = listeners.get(eventName) ?? new Set();
      eventListeners.add(listener);
      listeners.set(eventName, eventListeners);
      const unlisten = vi.fn(() => eventListeners.delete(listener));
      unlistenByEvent.set(eventName, unlisten);
      return unlisten;
    },
  );
  return {
    bridge: {
      isTauri: () => available,
      invoke,
      listen,
      openExternal,
    },
    invoke,
    listen,
    openExternal,
    unlistenByEvent,
    emit(eventName, payload) {
      for (const listener of listeners.get(eventName) ?? []) listener(payload);
    },
  };
}
