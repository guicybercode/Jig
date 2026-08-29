import { createRef, StrictMode } from "react";
import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const xtermMocks = vi.hoisted(() => {
  interface Disposable {
    dispose(): void;
  }

  interface Addon extends Disposable {
    activate(terminal: MockTerminal): void;
  }

  type Listener<TValue> = (value: TValue) => void;

  class MockTerminal {
    readonly writeCalls: Array<string | Uint8Array> = [];
    readonly resizeCalls: Array<{ cols: number; rows: number }> = [];
    readonly dataListeners = new Set<Listener<string>>();
    readonly binaryListeners = new Set<Listener<string>>();
    readonly resizeListeners = new Set<
      Listener<{ cols: number; rows: number }>
    >();
    readonly addons: Addon[] = [];
    readonly options: {
      disableStdin?: boolean;
      screenReaderMode?: boolean;
    };
    readonly writeCallbacks: Array<() => void> = [];
    textarea: HTMLTextAreaElement | undefined;
    element: HTMLDivElement | undefined;
    xtermElement: HTMLDivElement | undefined;
    cols = 80;
    rows = 24;
    disposeCount = 0;
    focusCount = 0;

    constructor(options?: {
      disableStdin?: boolean;
      screenReaderMode?: boolean;
    }) {
      this.options = { ...options };
      terminalInstances.push(this);
    }

    loadAddon(addon: Addon): void {
      this.addons.push(addon);
      addon.activate(this);
    }

    open(element: HTMLElement): void {
      this.xtermElement = element.ownerDocument.createElement("div");
      this.xtermElement.className = "xterm";
      this.element = this.xtermElement;
      this.textarea = element.ownerDocument.createElement("textarea");
      this.xtermElement.append(this.textarea);
      element.append(this.xtermElement);
    }

    onData(listener: Listener<string>): Disposable {
      return registerListener(this.dataListeners, listener);
    }

    onBinary(listener: Listener<string>): Disposable {
      return registerListener(this.binaryListeners, listener);
    }

    onResize(
      listener: Listener<{ cols: number; rows: number }>,
    ): Disposable {
      return registerListener(this.resizeListeners, listener);
    }

    emitData(data: string): void {
      for (const listener of this.dataListeners) {
        listener(data);
      }
    }

    emitBinary(data: string): void {
      for (const listener of this.binaryListeners) {
        listener(data);
      }
    }

    write(data: string | Uint8Array, onParsed?: () => void): void {
      this.writeCalls.push(data);
      if (onParsed) {
        this.writeCallbacks.push(onParsed);
      }
    }

    flushWrites(): void {
      for (const callback of this.writeCallbacks.splice(0)) {
        callback();
      }
    }

    resize(cols: number, rows: number): void {
      this.cols = cols;
      this.rows = rows;
      this.resizeCalls.push({ cols, rows });
      for (const listener of this.resizeListeners) {
        listener({ cols, rows });
      }
    }

    focus(): void {
      this.focusCount += 1;
      this.textarea?.focus();
    }

    dispose(): void {
      this.disposeCount += 1;
      for (const addon of this.addons) {
        addon.dispose();
      }
      this.xtermElement?.remove();
    }
  }

  class MockFitAddon implements Addon {
    terminal: MockTerminal | null = null;
    disposeCount = 0;

    constructor() {
      fitAddonInstances.push(this);
    }

    activate(terminal: MockTerminal): void {
      this.terminal = terminal;
    }

    proposeDimensions(): { cols: number; rows: number } | undefined {
      return proposedDimensions;
    }

    fit(): void {}

    dispose(): void {
      this.disposeCount += 1;
    }
  }

  const terminalInstances: MockTerminal[] = [];
  const fitAddonInstances: MockFitAddon[] = [];
  let proposedDimensions: { cols: number; rows: number } | undefined = {
    cols: 100,
    rows: 30,
  };

  function registerListener<TValue>(
    listeners: Set<Listener<TValue>>,
    listener: Listener<TValue>,
  ): Disposable {
    listeners.add(listener);
    return {
      dispose() {
        listeners.delete(listener);
      },
    };
  }

  return {
    MockTerminal,
    MockFitAddon,
    terminalInstances,
    fitAddonInstances,
    setProposedDimensions(dimensions?: { cols: number; rows: number }) {
      proposedDimensions = dimensions;
    },
    reset() {
      terminalInstances.length = 0;
      fitAddonInstances.length = 0;
      proposedDimensions = { cols: 100, rows: 30 };
    },
  };
});

vi.mock("@xterm/xterm", () => ({ Terminal: xtermMocks.MockTerminal }));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: xtermMocks.MockFitAddon }));

import { TerminalSurface } from "./TerminalSurface";
import type { TerminalSurfaceHandle } from "./TerminalSurface";

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

describe("TerminalSurface", () => {
  beforeEach(() => {
    xtermMocks.reset();
    resizeObservers.length = 0;
    animationFrames.clear();
    nextAnimationFrameId = 1;
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
    vi.stubGlobal(
      "matchMedia",
      vi.fn(() => ({
        matches: false,
        media: "(prefers-reduced-motion: reduce)",
        onchange: null,
        addListener: vi.fn(),
        removeListener: vi.fn(),
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
        dispatchEvent: vi.fn(() => false),
      })),
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("mounts an accessible xterm host and disposes every resource", () => {
    const terminalRef = createRef<TerminalSurfaceHandle>();
    const { unmount } = render(
      <TerminalSurface ref={terminalRef} accessibleLabel="Codex terminal" />,
    );
    const terminal = requireItem(xtermMocks.terminalInstances, 0);
    const fitAddon = requireItem(xtermMocks.fitAddonInstances, 0);
    const observer = requireItem(resizeObservers, 0);

    expect(
      screen.getByRole("region", { name: "Codex terminal" }),
    ).toHaveAttribute("data-terminal-root", "true");
    expect(terminal.textarea).toHaveAttribute("aria-label", "Codex terminal");
    expect(terminal.textarea).toHaveAttribute("aria-describedby");
    expect(terminal.options.screenReaderMode).toBe(true);
    expect(observer.observe).toHaveBeenCalledOnce();
    const handle = requireValue(terminalRef.current);
    expect(handle.focus()).toBe(true);
    expect(terminal.focusCount).toBe(1);
    expect(handle.reset(12)).toBe(true);
    expect(handle.getCursor()).toBe(12);
    expect(terminal.disposeCount).toBe(1);
    expect(xtermMocks.terminalInstances).toHaveLength(2);
    const resetTerminal = requireItem(xtermMocks.terminalInstances, 1);
    expect(resetTerminal.focusCount).toBe(1);

    unmount();

    expect(observer.disconnect).toHaveBeenCalledOnce();
    expect(terminal.disposeCount).toBe(1);
    expect(resetTerminal.disposeCount).toBe(1);
    expect(fitAddon.disposeCount).toBe(1);
    expect(animationFrames.size).toBe(0);
  });

  it("forwards Ctrl+C, text, and binary input and suppresses them while read-only", () => {
    const onInput = vi.fn();
    const { rerender } = render(
      <TerminalSurface
        accessibleLabel="Interactive terminal"
        onInput={onInput}
      />,
    );
    const terminal = requireItem(xtermMocks.terminalInstances, 0);

    act(() => {
      terminal.emitData("\u0003");
      terminal.emitData("ls\r");
      terminal.emitBinary("\u0000\u00ff");
    });
    expect(onInput.mock.calls).toEqual([
      [{ kind: "text", data: "\u0003" }],
      [{ kind: "text", data: "ls\r" }],
      [{ kind: "binary", data: "\u0000\u00ff" }],
    ]);

    rerender(
      <TerminalSurface
        accessibleLabel="Interactive terminal"
        readOnly
        onInput={onInput}
      />,
    );
    act(() => {
      terminal.emitData("ignored");
      terminal.emitBinary("ignored");
    });

    expect(onInput).toHaveBeenCalledTimes(3);
    expect(terminal.options.disableStdin).toBe(true);
    expect(terminal.textarea).toHaveAttribute("aria-readonly", "true");
  });

  it("updates screen-reader output mode without recreating xterm", () => {
    const { rerender } = render(
      <TerminalSurface accessibleLabel="Accessible terminal" />,
    );
    const terminal = requireItem(xtermMocks.terminalInstances, 0);

    rerender(
      <TerminalSurface
        accessibleLabel="Accessible terminal"
        screenReaderMode={false}
      />,
    );

    expect(xtermMocks.terminalInstances).toHaveLength(1);
    expect(terminal.options.screenReaderMode).toBe(false);
  });

  it("coalesces ResizeObserver work and reports only changed dimensions", () => {
    const onResize = vi.fn();
    render(
      <TerminalSurface
        accessibleLabel="Resizable terminal"
        onResize={onResize}
      />,
    );
    const terminal = requireItem(xtermMocks.terminalInstances, 0);
    const observer = requireItem(resizeObservers, 0);

    flushAnimationFrames();
    expect(terminal.resizeCalls).toEqual([{ cols: 100, rows: 30 }]);
    expect(onResize).toHaveBeenCalledWith({ cols: 100, rows: 30 });

    xtermMocks.setProposedDimensions({ cols: 132, rows: 42 });
    observer.trigger();
    observer.trigger();
    observer.trigger();
    expect(animationFrames.size).toBe(1);
    flushAnimationFrames();

    expect(terminal.resizeCalls).toEqual([
      { cols: 100, rows: 30 },
      { cols: 132, rows: 42 },
    ]);
    expect(onResize).toHaveBeenCalledTimes(2);

    observer.trigger();
    flushAnimationFrames();
    expect(onResize).toHaveBeenCalledTimes(2);
  });

  it("treats initialCursor as mount-only state until an explicit reset", () => {
    const terminalRef = createRef<TerminalSurfaceHandle>();
    const { rerender } = render(
      <TerminalSurface
        ref={terminalRef}
        accessibleLabel="Stable terminal"
        initialCursor={4}
      />,
    );
    const terminal = requireItem(xtermMocks.terminalInstances, 0);
    const handle = requireValue(terminalRef.current);
    expect(handle.getCursor()).toBe(4);

    rerender(
      <TerminalSurface
        ref={terminalRef}
        accessibleLabel="Stable terminal"
        initialCursor={99}
      />,
    );

    expect(xtermMocks.terminalInstances).toHaveLength(1);
    expect(terminal.disposeCount).toBe(0);
    expect(handle.getCursor()).toBe(4);
  });

  it("recreates xterm so pending output cannot leak across a PTY reset", () => {
    const terminalRef = createRef<TerminalSurfaceHandle>();
    render(
      <TerminalSurface
        ref={terminalRef}
        accessibleLabel="Restarted terminal"
      />,
    );
    const firstTerminal = requireItem(xtermMocks.terminalInstances, 0);
    const handle = requireValue(terminalRef.current);

    expect(
      handle.writeOutput({ data: "old PTY", sequence: 1, replay: false }),
    ).toBe("queued");
    expect(handle.getCursor()).toBe(0);
    expect(handle.reset(0)).toBe(true);

    expect(firstTerminal.disposeCount).toBe(1);
    expect(xtermMocks.terminalInstances).toHaveLength(2);
    act(() => firstTerminal.flushWrites());
    expect(handle.getCursor()).toBe(0);
    const activeTerminal = requireItem(xtermMocks.terminalInstances, 1);
    expect(activeTerminal.writeCalls).toHaveLength(0);
  });

  it("defers cursor observers and contains observer failures", async () => {
    const terminalRef = createRef<TerminalSurfaceHandle>();
    const onCursorApplied = vi.fn(() => {
      throw new Error("consumer failed");
    });
    render(
      <TerminalSurface
        ref={terminalRef}
        accessibleLabel="Observed terminal"
        onCursorApplied={onCursorApplied}
      />,
    );
    const terminal = requireItem(xtermMocks.terminalInstances, 0);
    const handle = requireValue(terminalRef.current);

    handle.writeOutput({ data: "parsed", sequence: 1, replay: false });
    expect(onCursorApplied).not.toHaveBeenCalled();
    expect(() => terminal.flushWrites()).not.toThrow();

    await act(async () => Promise.resolve());
    expect(onCursorApplied).toHaveBeenCalledOnce();
    expect(onCursorApplied).toHaveBeenCalledWith(1);
  });

  it("survives StrictMode setup-cleanup-setup without live duplicate listeners", () => {
    const onInput = vi.fn();
    const { unmount } = render(
      <StrictMode>
        <TerminalSurface
          accessibleLabel="Strict terminal"
          onInput={onInput}
        />
      </StrictMode>,
    );
    expect(xtermMocks.terminalInstances).toHaveLength(2);
    const firstTerminal = requireItem(xtermMocks.terminalInstances, 0);
    const activeTerminal = requireItem(xtermMocks.terminalInstances, 1);
    const firstObserver = requireItem(resizeObservers, 0);

    expect(firstTerminal.disposeCount).toBe(1);
    expect(firstObserver.disconnect).toHaveBeenCalledOnce();
    act(() => {
      firstTerminal.emitData("stale");
      activeTerminal.emitData("active");
    });
    expect(onInput).toHaveBeenCalledOnce();
    expect(onInput).toHaveBeenCalledWith({ kind: "text", data: "active" });

    unmount();
    expect(activeTerminal.disposeCount).toBe(1);
  });
});

function flushAnimationFrames(): void {
  const queuedFrames = Array.from(animationFrames.values());
  animationFrames.clear();
  for (const callback of queuedFrames) {
    callback(performance.now());
  }
}

function requireItem<TValue>(
  values: readonly TValue[],
  index: number,
): TValue {
  const value = values[index];
  if (value === undefined) {
    throw new Error(`Expected item ${index} to exist.`);
  }
  return value;
}

function requireValue<TValue>(value: TValue | null | undefined): TValue {
  if (value === null || value === undefined) {
    throw new Error("Expected value to exist.");
  }
  return value;
}
