import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import type { IDisposable, ITheme, ITerminalOptions } from "@xterm/xterm";

import {
  createTerminalOutputWriter,
  type TerminalOutputChunk,
  type TerminalOutputData,
  type TerminalOutputGap,
  type TerminalWriteDisposition,
} from "./terminal-output-writer";

/** Dimensions accepted by the PTY wire contract. */
export interface TerminalDimensions {
  readonly cols: number;
  readonly rows: number;
}

/** Text and xterm byte-string input remain distinct for byte-safe IPC encoding. */
export type TerminalInput =
  | { readonly kind: "text"; readonly data: string }
  | { readonly kind: "binary"; readonly data: string };

/** Latest callbacks and mutable presentation options supplied by React. */
export interface TerminalRuntimeBindings {
  readonly accessibleLabel: string;
  readonly accessibleDescriptionId: string;
  readonly readOnly: boolean;
  readonly screenReaderMode: boolean;
  readonly onInput?: (input: TerminalInput) => void;
  readonly onResize?: (dimensions: TerminalDimensions) => void;
  readonly onCursorApplied?: (cursor: number) => void;
}

/** Options required to mount a terminal runtime into one DOM element. */
export interface CreateTerminalRuntimeOptions {
  readonly element: HTMLElement;
  readonly initialCursor: number;
  readonly getBindings: () => TerminalRuntimeBindings;
}

/** Non-React terminal lifetime and imperative operations. */
export interface TerminalRuntime {
  write(data: TerminalOutputData): void;
  writeOutput(chunk: TerminalOutputChunk): TerminalWriteDisposition;
  markOutputGap(gap: TerminalOutputGap): boolean;
  markReplayComplete(outputSequence: number): boolean;
  reset(cursor?: number): void;
  focus(): void;
  getCursor(): number;
  setAccessibleLabel(label: string): void;
  setReadOnly(readOnly: boolean): void;
  setScreenReaderMode(enabled: boolean): void;
  dispose(): void;
}

const MAX_TERMINAL_DIMENSION = 4096;
const DEFAULT_FONT_FAMILY =
  'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace';

/** Mounts xterm, input listeners, and one frame-coalesced resize observer. */
export function createTerminalRuntime({
  element,
  initialCursor,
  getBindings,
}: CreateTerminalRuntimeOptions): TerminalRuntime {
  let terminal: Terminal | null = null;
  let fitAddon: FitAddon | null = null;
  let terminalDisposables: IDisposable[] = [];
  let isDisposed = false;
  let lastDimensions: TerminalDimensions | null = null;
  let cursorNotificationGeneration = 0;

  const outputWriter = createTerminalOutputWriter(
    (data, onParsed) => requireTerminal().write(data, onParsed),
    initialCursor,
    scheduleCursorApplied,
  );
  mountTerminal();

  const resizeScheduler = createResizeScheduler(element, resizeTerminal);
  resizeScheduler.schedule();

  function resizeTerminal(): void {
    const activeTerminal = terminal;
    const activeFitAddon = fitAddon;
    if (isDisposed || !activeTerminal || !activeFitAddon) {
      return;
    }
    const proposed = activeFitAddon.proposeDimensions();
    if (!proposed) {
      return;
    }
    const dimensions = normalizeDimensions(proposed);
    if (!dimensions) {
      return;
    }
    if (
      activeTerminal.cols !== dimensions.cols ||
      activeTerminal.rows !== dimensions.rows
    ) {
      activeTerminal.resize(dimensions.cols, dimensions.rows);
    }
    notifyResize({
      cols: activeTerminal.cols,
      rows: activeTerminal.rows,
    });
  }

  function notifyResize(dimensions: TerminalDimensions): void {
    if (
      isDisposed ||
      (lastDimensions?.cols === dimensions.cols &&
        lastDimensions.rows === dimensions.rows)
    ) {
      return;
    }
    lastDimensions = dimensions;
    getBindings().onResize?.(dimensions);
  }

  function setAccessibleLabel(label: string): void {
    const textarea = terminal?.textarea;
    if (textarea) {
      textarea.setAttribute("aria-label", label);
    }
  }

  function setReadOnly(readOnly: boolean): void {
    const activeTerminal = terminal;
    if (!activeTerminal) {
      return;
    }
    activeTerminal.options.disableStdin = readOnly;
    const textarea = activeTerminal.textarea;
    if (textarea) {
      textarea.setAttribute("aria-readonly", String(readOnly));
    }
  }

  function setScreenReaderMode(enabled: boolean): void {
    if (terminal) {
      terminal.options.screenReaderMode = enabled;
    }
  }

  function dispose(): void {
    if (isDisposed) {
      return;
    }
    isDisposed = true;
    cursorNotificationGeneration += 1;
    resizeScheduler.dispose();
    outputWriter.dispose();
    disposeTerminal();
  }

  function reset(cursor = 0): void {
    if (isDisposed) {
      return;
    }
    const activeElement = element.ownerDocument.activeElement;
    const restoreFocus = Boolean(
      terminal?.textarea === activeElement ||
        terminal?.element?.contains(activeElement),
    );
    cursorNotificationGeneration += 1;
    disposeTerminal();
    outputWriter.reset(cursor);
    lastDimensions = null;
    mountTerminal();
    if (restoreFocus) {
      terminal?.focus();
    }
    resizeScheduler.schedule();
  }

  function scheduleCursorApplied(cursor: number): void {
    const notificationGeneration = cursorNotificationGeneration;
    queueMicrotask(() => {
      if (
        isDisposed ||
        notificationGeneration !== cursorNotificationGeneration
      ) {
        return;
      }
      try {
        getBindings().onCursorApplied?.(cursor);
      } catch {
        // Integration callbacks cannot be allowed to break xterm's parser loop.
      }
    });
  }

  function mountTerminal(): void {
    const mountedTerminal = new Terminal(
      createTerminalOptions(element, getBindings()),
    );
    const mountedFitAddon = new FitAddon();
    mountedTerminal.loadAddon(mountedFitAddon);
    mountedTerminal.open(element);
    setTerminalAccessibility(mountedTerminal, getBindings());
    terminal = mountedTerminal;
    fitAddon = mountedFitAddon;
    terminalDisposables = [
      mountedTerminal.onData((data) => {
        const bindings = getBindings();
        if (!isDisposed && !bindings.readOnly) {
          bindings.onInput?.({ kind: "text", data });
        }
      }),
      mountedTerminal.onBinary((data) => {
        const bindings = getBindings();
        if (!isDisposed && !bindings.readOnly) {
          bindings.onInput?.({ kind: "binary", data });
        }
      }),
      mountedTerminal.onResize((dimensions) => notifyResize(dimensions)),
    ];
  }

  function disposeTerminal(): void {
    for (const disposable of terminalDisposables) {
      disposable.dispose();
    }
    terminalDisposables = [];
    terminal?.dispose();
    terminal = null;
    fitAddon = null;
  }

  function requireTerminal(): Terminal {
    if (!terminal || isDisposed) {
      throw new Error("Terminal runtime is not mounted");
    }
    return terminal;
  }

  return {
    write: outputWriter.write,
    writeOutput: outputWriter.writeOutput,
    markOutputGap: outputWriter.markOutputGap,
    markReplayComplete: outputWriter.markReplayComplete,
    reset,
    focus: () => requireTerminal().focus(),
    getCursor: outputWriter.getCursor,
    setAccessibleLabel,
    setReadOnly,
    setScreenReaderMode,
    dispose,
  };
}

interface ResizeScheduler {
  schedule(): void;
  dispose(): void;
}

function createResizeScheduler(
  element: HTMLElement,
  resize: () => void,
): ResizeScheduler {
  const view = element.ownerDocument.defaultView;
  let isDisposed = false;
  let frameId: number | null = null;

  function requestFrame(callback: FrameRequestCallback): number {
    if (view?.requestAnimationFrame) {
      return view.requestAnimationFrame(callback);
    }
    return window.setTimeout(() => callback(performance.now()), 16);
  }

  function cancelFrame(id: number): void {
    if (view?.cancelAnimationFrame) {
      view.cancelAnimationFrame(id);
      return;
    }
    window.clearTimeout(id);
  }

  function schedule(): void {
    if (isDisposed) {
      return;
    }
    if (frameId !== null) {
      cancelFrame(frameId);
    }
    frameId = requestFrame(() => {
      frameId = null;
      resize();
    });
  }

  const ResizeObserverConstructor = view?.ResizeObserver;
  const resizeObserver = ResizeObserverConstructor
    ? new ResizeObserverConstructor(schedule)
    : null;
  resizeObserver?.observe(element);
  if (!resizeObserver) {
    view?.addEventListener("resize", schedule);
  }

  return {
    schedule,
    dispose() {
      if (isDisposed) {
        return;
      }
      isDisposed = true;
      resizeObserver?.disconnect();
      view?.removeEventListener("resize", schedule);
      if (frameId !== null) {
        cancelFrame(frameId);
        frameId = null;
      }
    },
  };
}

function createTerminalOptions(
  element: HTMLElement,
  bindings: TerminalRuntimeBindings,
): ITerminalOptions {
  const styles = element.ownerDocument.defaultView?.getComputedStyle(element);
  const reducedMotion =
    element.ownerDocument.defaultView?.matchMedia?.(
      "(prefers-reduced-motion: reduce)",
    ).matches ?? false;

  return {
    allowTransparency: false,
    convertEol: false,
    cursorBlink: !reducedMotion,
    cursorInactiveStyle: "outline",
    disableStdin: bindings.readOnly,
    fontFamily: readCssToken(styles, "--font-mono") ?? DEFAULT_FONT_FAMILY,
    fontSize: 13,
    lineHeight: 1.25,
    minimumContrastRatio: 4.5,
    screenReaderMode: bindings.screenReaderMode,
    scrollback: 10_000,
    smoothScrollDuration: 0,
    theme: createTerminalTheme(styles),
  };
}

function createTerminalTheme(styles?: CSSStyleDeclaration): ITheme {
  const theme: ITheme = {};
  const background = readCssToken(styles, "--color-inset");
  const foreground = readCssToken(styles, "--color-text");
  const cursor = readCssToken(styles, "--color-accent");
  const cursorAccent = readCssToken(styles, "--color-on-accent");
  const selectionBackground = readCssToken(
    styles,
    "--color-surface-active",
  );
  if (background) theme.background = background;
  if (foreground) theme.foreground = foreground;
  if (cursor) theme.cursor = cursor;
  if (cursorAccent) theme.cursorAccent = cursorAccent;
  if (selectionBackground) theme.selectionBackground = selectionBackground;
  if (foreground) theme.selectionForeground = foreground;
  return theme;
}

function readCssToken(
  styles: CSSStyleDeclaration | undefined,
  token: string,
): string | undefined {
  const value = styles?.getPropertyValue(token).trim();
  return value || undefined;
}

function normalizeDimensions(
  dimensions: TerminalDimensions,
): TerminalDimensions | null {
  const cols = normalizeDimension(dimensions.cols);
  const rows = normalizeDimension(dimensions.rows);
  return cols && rows ? { cols, rows } : null;
}

function normalizeDimension(value: number): number | null {
  if (!Number.isFinite(value) || value < 1) {
    return null;
  }
  return Math.min(Math.floor(value), MAX_TERMINAL_DIMENSION);
}

function setTerminalAccessibility(
  terminal: Terminal,
  bindings: TerminalRuntimeBindings,
): void {
  const textarea = terminal.textarea;
  if (!textarea) {
    return;
  }
  textarea.setAttribute("aria-label", bindings.accessibleLabel);
  textarea.setAttribute(
    "aria-describedby",
    bindings.accessibleDescriptionId,
  );
  textarea.setAttribute("aria-readonly", String(bindings.readOnly));
}
