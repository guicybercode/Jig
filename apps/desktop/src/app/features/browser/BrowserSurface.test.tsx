import { StrictMode } from "react";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { BrowserSurface } from "./BrowserSurface";
import type {
  BrowserRuntime,
  BrowserRuntimeEvent,
  BrowserRuntimeListener,
} from "./browser-runtime";

class TestResizeObserver implements ResizeObserver {
  readonly observe = vi.fn<(target: Element) => void>();
  readonly unobserve = vi.fn<(target: Element) => void>();
  readonly disconnect = vi.fn<() => void>();
  readonly callback: ResizeObserverCallback;

  constructor(callback: ResizeObserverCallback) {
    this.callback = callback;
    resizeObservers.push(this);
  }

  trigger(): void {
    this.callback([], this);
  }
}

const resizeObservers: TestResizeObserver[] = [];
const animationFrames = new Map<number, FrameRequestCallback>();
let nextAnimationFrameId = 1;
let browserRect = new DOMRect(40, 72, 640, 360);
let browserViewportRect = new DOMRect(0, 0, 1_024, 768);
let obstructionRect = new DOMRect();
let siblingCanvasRect = new DOMRect();

describe("BrowserSurface", () => {
  beforeEach(() => {
    resizeObservers.length = 0;
    animationFrames.clear();
    nextAnimationFrameId = 1;
    browserRect = new DOMRect(40, 72, 640, 360);
    browserViewportRect = new DOMRect(0, 0, 1_024, 768);
    obstructionRect = new DOMRect();
    siblingCanvasRect = new DOMRect();
    vi.stubGlobal("ResizeObserver", TestResizeObserver);
    vi.stubGlobal(
      "requestAnimationFrame",
      vi.fn((callback: FrameRequestCallback) => {
        const frameId = nextAnimationFrameId;
        nextAnimationFrameId += 1;
        animationFrames.set(frameId, callback);
        return frameId;
      }),
    );
    vi.stubGlobal(
      "cancelAnimationFrame",
      vi.fn((frameId: number) => {
        animationFrames.delete(frameId);
      }),
    );
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
      function getBoundingClientRect(this: HTMLElement) {
        if (this.hasAttribute("data-browser-surface-node-id")) {
          return browserRect;
        }
        if (this.hasAttribute("data-browser-viewport")) {
          return browserViewportRect;
        }
        if (this.hasAttribute("data-browser-obstruction")) {
          return obstructionRect;
        }
        if (this.hasAttribute("data-test-sibling-canvas-node")) {
          return siblingCanvasRect;
        }
        if (this.hasAttribute("data-test-own-canvas-node")) {
          return new DOMRect(24, 48, 680, 440);
        }
        return new DOMRect();
      },
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("renders accessible chrome and a safe inactive placeholder", async () => {
    const user = userEvent.setup();
    const harness = createRuntimeHarness();
    const onActivate = vi.fn();
    const onOpenExternal = vi.fn();

    const { container } = render(
      <BrowserSurface
        nodeId="browser-docs"
        url="https://example.com/docs"
        runtime={harness.runtime}
        onActivate={onActivate}
        onNavigate={vi.fn()}
        onOpenExternal={onOpenExternal}
      />,
    );

    expect(
      screen.getByRole("region", { name: "Integrated browser" }),
    ).toHaveAttribute("data-shortcut-scope", "true");
    expect(screen.getByRole("navigation", { name: "Browser controls" })).toBeVisible();
    expect(screen.getByRole("textbox", { name: "Address" })).toHaveValue(
      "https://example.com/docs",
    );
    expect(
      screen.getByText("Select this card to activate the integrated browser."),
    ).toBeVisible();
    expect(
      container.querySelector('[data-browser-surface-node-id="browser-docs"]'),
    ).toHaveAttribute("data-native-browser-visible", "false");
    expect(container.querySelector("iframe")).not.toBeInTheDocument();
    expect(harness.open).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: "Activate browser" }));
    expect(onActivate).toHaveBeenCalledOnce();

    await user.click(
      screen.getByRole("button", { name: "Open in default browser" }),
    );
    expect(onOpenExternal).toHaveBeenCalledWith("https://example.com/docs");
  });

  it("opens after subscribing, reflects native events, and cleans up", async () => {
    const harness = createRuntimeHarness();
    const onNavigate = vi.fn();
    const { unmount } = render(
      <BrowserSurface
        nodeId="browser-docs"
        url="https://example.com/"
        active
        runtime={harness.runtime}
        onActivate={vi.fn()}
        onNavigate={onNavigate}
        onOpenExternal={vi.fn()}
      />,
    );

    await waitFor(() => expect(harness.open).toHaveBeenCalledOnce());
    expect(harness.subscribe.mock.invocationCallOrder[0]).toBeLessThan(
      harness.open.mock.invocationCallOrder[0] ?? Number.POSITIVE_INFINITY,
    );
    expect(harness.open).toHaveBeenCalledWith({
      nodeId: "browser-docs",
      url: "https://example.com/",
      bounds: { x: 40, y: 72, width: 640, height: 360 },
      visible: true,
    });

    act(() => {
      harness.emit({
        type: "load-state",
        nodeId: "browser-docs",
        status: "started",
      });
    });
    expect(screen.getByText("Loading page…")).toBeVisible();

    act(() => {
      harness.emit({
        type: "location-changed",
        nodeId: "browser-docs",
        url: "https://example.com/redirected",
      });
      harness.emit({
        type: "load-state",
        nodeId: "browser-docs",
        status: "finished",
      });
    });
    expect(screen.queryByText("Loading page…")).not.toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Address" })).toHaveValue(
      "https://example.com/redirected",
    );
    expect(onNavigate).not.toHaveBeenCalled();

    harness.update.mockClear();
    harness.close.mockClear();
    unmount();
    expect(harness.update).toHaveBeenCalledWith({
      nodeId: "browser-docs",
      bounds: { x: 40, y: 72, width: 640, height: 360 },
      visible: false,
    });
    expect(harness.update.mock.invocationCallOrder[0]).toBeLessThan(
      harness.close.mock.invocationCallOrder[0] ?? Number.POSITIVE_INFINITY,
    );
    await waitFor(() => {
      expect(harness.unsubscribe).toHaveBeenCalledOnce();
      expect(harness.close).toHaveBeenCalledWith({ nodeId: "browser-docs" });
    });
  });

  it("keeps the last valid URL when a submitted address is unsafe", async () => {
    const user = userEvent.setup();
    const harness = createRuntimeHarness();
    const onNavigate = vi.fn();
    const onOpenExternal = vi.fn();
    render(
      <BrowserSurface
        nodeId="browser-safe"
        url="https://example.com/safe"
        active
        runtime={harness.runtime}
        onActivate={vi.fn()}
        onNavigate={onNavigate}
        onOpenExternal={onOpenExternal}
      />,
    );
    await waitFor(() => expect(harness.open).toHaveBeenCalledOnce());

    const address = screen.getByRole("textbox", { name: "Address" });
    await user.clear(address);
    await user.type(address, "file:///etc/passwd");
    await user.click(screen.getByRole("button", { name: "Go" }));

    expect(screen.getByRole("alert")).toHaveTextContent(
      "Enter a valid HTTP or HTTPS address without embedded credentials.",
    );
    expect(address).toHaveValue("file:///etc/passwd");
    expect(address).toHaveAttribute("aria-invalid", "true");
    expect(onNavigate).not.toHaveBeenCalled();
    expect(harness.navigate).not.toHaveBeenCalled();

    await user.click(
      screen.getByRole("button", { name: "Open in default browser" }),
    );
    expect(onOpenExternal).toHaveBeenCalledWith("https://example.com/safe");
  });

  it("normalizes a valid address and navigates the active native surface", async () => {
    const user = userEvent.setup();
    const harness = createRuntimeHarness();
    const onNavigate = vi.fn();
    render(
      <BrowserSurface
        nodeId="browser-search"
        url="https://example.com/"
        active
        runtime={harness.runtime}
        onActivate={vi.fn()}
        onNavigate={onNavigate}
        onOpenExternal={vi.fn()}
      />,
    );
    await waitFor(() => expect(harness.open).toHaveBeenCalledOnce());

    const address = screen.getByRole("textbox", { name: "Address" });
    await user.clear(address);
    await user.type(address, "openai.com/research");
    await user.click(screen.getByRole("button", { name: "Go" }));

    expect(address).toHaveValue("https://openai.com/research");
    expect(onNavigate).toHaveBeenCalledWith("https://openai.com/research");
    await waitFor(() =>
      expect(harness.navigate).toHaveBeenCalledWith({
        nodeId: "browser-search",
        url: "https://openai.com/research",
      }),
    );
    await waitFor(() =>
      expect(harness.focus).toHaveBeenCalledWith({ nodeId: "browser-search" }),
    );
  });

  it("navigates with a transient signed URL but exposes only its redacted form", async () => {
    const user = userEvent.setup();
    const harness = createRuntimeHarness();
    const onNavigate = vi.fn();
    render(
      <BrowserSurface
        nodeId="browser-signed"
        url="https://example.com/"
        active
        runtime={harness.runtime}
        onActivate={vi.fn()}
        onNavigate={onNavigate}
        onOpenExternal={vi.fn()}
      />,
    );
    await waitFor(() => expect(harness.open).toHaveBeenCalledOnce());

    const address = screen.getByRole("textbox", { name: "Address" });
    await user.clear(address);
    await user.type(
      address,
      "https://example.com/download?file=report&X-Amz-Signature=secret#preview",
    );
    await user.click(screen.getByRole("button", { name: "Go" }));

    expect(onNavigate).toHaveBeenCalledWith(
      "https://example.com/download?file=report",
    );
    await waitFor(() =>
      expect(harness.navigate).toHaveBeenCalledWith({
        nodeId: "browser-signed",
        url: "https://example.com/download?file=report&X-Amz-Signature=secret#preview",
      }),
    );
  });

  it("coalesces bounds work and forwards visibility changes", async () => {
    const harness = createRuntimeHarness();
    const props = {
      nodeId: "browser-layout",
      url: "https://example.com/",
      active: true,
      runtime: harness.runtime,
      onActivate: vi.fn(),
      onNavigate: vi.fn(),
      onOpenExternal: vi.fn(),
    } as const;
    const { rerender } = render(<BrowserSurface {...props} />);
    await waitFor(() => expect(harness.open).toHaveBeenCalledOnce());
    flushAnimationFrames();
    harness.update.mockClear();

    browserRect = new DOMRect(84, 96, 720, 440);
    const observer = requireItem(resizeObservers, 0);
    observer.trigger();
    observer.trigger();
    window.dispatchEvent(new Event("resize"));
    window.dispatchEvent(new Event("scroll"));
    expect(animationFrames.size).toBe(1);
    flushAnimationFrames();

    await waitFor(() =>
      expect(harness.update).toHaveBeenCalledTimes(1),
    );
    expect(harness.update).toHaveBeenLastCalledWith({
      nodeId: "browser-layout",
      bounds: { x: 84, y: 96, width: 720, height: 440 },
      visible: true,
    });

    harness.update.mockClear();
    rerender(
      <BrowserSurface
        {...props}
        visible={false}
        unavailableReason="Use 100% zoom to interact with this page."
      />,
    );
    expect(harness.update).toHaveBeenCalledWith({
      nodeId: "browser-layout",
      bounds: { x: 84, y: 96, width: 720, height: 440 },
      visible: false,
    });
    flushAnimationFrames();
    expect(
      screen.getByText("Use 100% zoom to interact with this page."),
    ).toBeVisible();
  });

  it("fails closed when the native slot is clipped or covered by chrome", async () => {
    const harness = createRuntimeHarness();
    obstructionRect = new DOMRect(20, 20, 800, 100);
    const { container } = render(
      <div className="canvas-workspace">
        <div data-browser-viewport="true">
          <div data-browser-obstruction="true" />
          <BrowserSurface
            nodeId="browser-contained"
            url="https://example.com/"
            active
            runtime={harness.runtime}
            onActivate={vi.fn()}
            onNavigate={vi.fn()}
            onOpenExternal={vi.fn()}
          />
        </div>
      </div>,
    );

    await waitFor(() => expect(harness.open).toHaveBeenCalledOnce());
    expect(harness.open).toHaveBeenCalledWith(
      expect.objectContaining({ visible: false }),
    );

    harness.update.mockClear();
    obstructionRect = new DOMRect(0, 0, 0, 0);
    container
      .querySelector("[data-browser-viewport]")
      ?.dispatchEvent(new Event("scroll", { bubbles: false }));
    flushAnimationFrames();
    await waitFor(() =>
      expect(harness.update).toHaveBeenCalledWith(
        expect.objectContaining({ visible: true }),
      ),
    );

    harness.update.mockClear();
    browserRect = new DOMRect(-20, 72, 640, 360);
    window.dispatchEvent(new Event("scroll"));
    flushAnimationFrames();
    await waitFor(() =>
      expect(harness.update).toHaveBeenCalledWith(
        expect.objectContaining({ visible: false }),
      ),
    );
  });

  it("treats sibling canvas cards, but not its own card, as obstructions", async () => {
    const harness = createRuntimeHarness();
    const props = {
      nodeId: "browser-overlap",
      url: "https://example.com/",
      active: true,
      runtime: harness.runtime,
      onActivate: vi.fn(),
      onNavigate: vi.fn(),
      onOpenExternal: vi.fn(),
    } as const;
    const layout = (withSibling: boolean) => (
      <div data-browser-viewport="true">
        <article className="canvas-node" data-test-own-canvas-node="true">
          <BrowserSurface {...props} />
        </article>
        {withSibling ? (
          <article
            className="canvas-node"
            data-test-sibling-canvas-node="true"
          />
        ) : null}
      </div>
    );
    const { rerender } = render(layout(false));

    await waitFor(() => expect(harness.open).toHaveBeenCalledOnce());
    expect(harness.open).toHaveBeenCalledWith(
      expect.objectContaining({ visible: true }),
    );

    harness.update.mockClear();
    siblingCanvasRect = new DOMRect(100, 100, 320, 240);
    rerender(layout(true));
    flushAnimationFrames();

    await waitFor(() =>
      expect(harness.update).toHaveBeenCalledWith(
        expect.objectContaining({ visible: false }),
      ),
    );
  });

  it("hides an open native surface when its DOM geometry becomes invalid", async () => {
    const harness = createRuntimeHarness();
    render(
      <BrowserSurface
        nodeId="browser-invalid-geometry"
        url="https://example.com/"
        active
        runtime={harness.runtime}
        onActivate={vi.fn()}
        onNavigate={vi.fn()}
        onOpenExternal={vi.fn()}
      />,
    );

    await waitFor(() => expect(harness.open).toHaveBeenCalledOnce());
    flushAnimationFrames();
    harness.update.mockClear();

    browserRect = new DOMRect(0, 0, 0, 0);
    window.dispatchEvent(new Event("resize"));
    flushAnimationFrames();

    await waitFor(() =>
      expect(harness.update).toHaveBeenCalledWith({
        nodeId: "browser-invalid-geometry",
        bounds: { x: 40, y: 72, width: 640, height: 360 },
        visible: false,
      }),
    );
  });

  it("never loads remote content in a non-Tauri environment", async () => {
    const user = userEvent.setup();
    const harness = createRuntimeHarness(false);
    const onNavigate = vi.fn();
    const { container } = render(
      <BrowserSurface
        nodeId="browser-web-fallback"
        url="https://example.com/"
        active
        runtime={harness.runtime}
        onActivate={vi.fn()}
        onNavigate={onNavigate}
        onOpenExternal={vi.fn()}
      />,
    );

    expect(
      screen.getByText(/available only in the desktop app/i),
    ).toBeVisible();
    expect(container.querySelector("iframe")).not.toBeInTheDocument();
    expect(harness.subscribe).not.toHaveBeenCalled();
    expect(harness.open).not.toHaveBeenCalled();

    const address = screen.getByRole("textbox", { name: "Address" });
    await user.clear(address);
    await user.type(address, "openai.com");
    await user.click(screen.getByRole("button", { name: "Go" }));
    expect(onNavigate).toHaveBeenCalledWith("https://openai.com/");
    expect(harness.navigate).not.toHaveBeenCalled();
  });

  it("contains StrictMode activation churn and performs final cleanup", async () => {
    const harness = createRuntimeHarness();
    const { unmount } = render(
      <StrictMode>
        <BrowserSurface
          nodeId="browser-strict"
          url="https://example.com/"
          active
          runtime={harness.runtime}
          onActivate={vi.fn()}
          onNavigate={vi.fn()}
          onOpenExternal={vi.fn()}
        />
      </StrictMode>,
    );
    await waitFor(() => expect(harness.open).toHaveBeenCalled());

    unmount();
    await waitFor(() => expect(harness.close).toHaveBeenCalled());
    expect(animationFrames.size).toBe(0);
  });

  it("reopens after deactivation races with a pending native open", async () => {
    let resolveFirstOpen: (() => void) | undefined;
    const harness = createRuntimeHarness();
    harness.open.mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          resolveFirstOpen = resolve;
        }),
    );
    const props = {
      nodeId: "browser-activation-race",
      url: "https://example.com/",
      runtime: harness.runtime,
      onActivate: vi.fn(),
      onNavigate: vi.fn(),
      onOpenExternal: vi.fn(),
    } as const;
    const { rerender } = render(<BrowserSurface {...props} active />);
    await waitFor(() => expect(harness.open).toHaveBeenCalledOnce());

    rerender(<BrowserSurface {...props} active={false} />);
    rerender(<BrowserSurface {...props} active />);
    resolveFirstOpen?.();

    await waitFor(() => expect(harness.close).toHaveBeenCalled());
    await waitFor(() => expect(harness.open).toHaveBeenCalledTimes(2));
  });
});

interface RuntimeHarness {
  readonly runtime: BrowserRuntime;
  readonly open: ReturnType<typeof vi.fn<BrowserRuntime["open"]>>;
  readonly navigate: ReturnType<typeof vi.fn<BrowserRuntime["navigate"]>>;
  readonly update: ReturnType<typeof vi.fn<BrowserRuntime["update"]>>;
  readonly focus: ReturnType<typeof vi.fn<BrowserRuntime["focus"]>>;
  readonly close: ReturnType<typeof vi.fn<BrowserRuntime["close"]>>;
  readonly subscribe: ReturnType<
    typeof vi.fn<NonNullable<BrowserRuntime["subscribe"]>>
  >;
  readonly unsubscribe: ReturnType<typeof vi.fn>;
  emit(event: BrowserRuntimeEvent): void;
}

function createRuntimeHarness(available = true): RuntimeHarness {
  let listener: BrowserRuntimeListener | null = null;
  const open = vi.fn<BrowserRuntime["open"]>(async () => undefined);
  const navigate = vi.fn<BrowserRuntime["navigate"]>(async () => undefined);
  const update = vi.fn<BrowserRuntime["update"]>(async () => undefined);
  const focus = vi.fn<BrowserRuntime["focus"]>(async () => undefined);
  const close = vi.fn<BrowserRuntime["close"]>(async () => undefined);
  const unsubscribe = vi.fn(() => {
    listener = null;
  });
  const subscribe = vi.fn<NonNullable<BrowserRuntime["subscribe"]>>(
    async (_nodeId, nextListener) => {
      listener = nextListener;
      return unsubscribe;
    },
  );
  return {
    runtime: {
      isAvailable: () => available,
      open,
      navigate,
      update,
      reload: vi.fn(async () => undefined),
      goBack: vi.fn(async () => undefined),
      goForward: vi.fn(async () => undefined),
      focus,
      close,
      openExternal: vi.fn(async () => undefined),
      subscribe,
    },
    open,
    navigate,
    update,
    focus,
    close,
    subscribe,
    unsubscribe,
    emit(event) {
      listener?.(event);
    },
  };
}

function flushAnimationFrames(): void {
  while (animationFrames.size > 0) {
    const frames = [...animationFrames.entries()];
    animationFrames.clear();
    for (const [frameId, callback] of frames) callback(frameId);
  }
}

function requireItem<T>(items: readonly T[], index: number): T {
  const item = items[index];
  if (item === undefined) throw new Error(`Missing item at index ${index}`);
  return item;
}
