import {
  useEffect,
  useId,
  useImperativeHandle,
  useLayoutEffect,
  useRef,
} from "react";
import type { Ref } from "react";

import "@xterm/xterm/css/xterm.css";
import "./terminal-surface.css";

import {
  createTerminalRuntime,
  type TerminalDimensions,
  type TerminalInput,
  type TerminalRuntime,
  type TerminalRuntimeBindings,
} from "./terminal-runtime";
import type {
  TerminalOutputChunk,
  TerminalOutputData,
  TerminalOutputGap,
  TerminalWriteDisposition,
} from "./terminal-output-writer";

/** Commands exposed to event subscriptions without routing bytes through React. */
export interface TerminalSurfaceHandle {
  /** Writes raw local data to the mounted xterm instance. */
  write(data: TerminalOutputData): boolean;
  /** Applies one sequenced daemon chunk exactly once. */
  writeOutput(chunk: TerminalOutputChunk): TerminalWriteDisposition | "unavailable";
  /** Inserts a visible marker for an unavailable replay range. */
  markOutputGap(gap: TerminalOutputGap): boolean;
  /** Moves replay to its live boundary and marks any missing tail. */
  markReplayComplete(outputSequence: number): boolean;
  /** Clears xterm and starts tracking a fresh PTY cursor. */
  reset(cursor?: number): boolean;
  /** Moves keyboard focus directly to xterm's input control. */
  focus(): boolean;
  /** Returns the applied cursor, or null before mount/after unmount. */
  getCursor(): number | null;
}

/** Props for the isolated, imperative xterm surface. */
export interface TerminalSurfaceProps {
  /** React 19 ref-as-prop handle used by the IPC/event adapter. */
  readonly ref?: Ref<TerminalSurfaceHandle>;
  /** Accessible name applied to both the terminal region and xterm input. */
  readonly accessibleLabel: string;
  /** Last sequence already rendered before subscribing for replay. */
  readonly initialCursor?: number;
  /** Prevents xterm from emitting user input while retaining selection/copy. */
  readonly readOnly?: boolean;
  /** Enables xterm's accessible output tree for VoiceOver/NVDA users. */
  readonly screenReaderMode?: boolean;
  /** Receives textual or byte-preserving binary xterm input. */
  readonly onInput?: (input: TerminalInput) => void;
  /** Receives only measured terminal dimension changes. */
  readonly onResize?: (dimensions: TerminalDimensions) => void;
  /** Receives the highest contiguous sequence parsed by xterm. */
  readonly onCursorApplied?: (cursor: number) => void;
  /** Optional integration class; terminal styling remains feature-local. */
  readonly className?: string;
}

/** Owns one xterm lifetime while keeping all PTY output outside React state. */
export function TerminalSurface({
  ref,
  accessibleLabel,
  initialCursor = 0,
  readOnly = false,
  screenReaderMode = true,
  onInput,
  onResize,
  onCursorApplied,
  className,
}: TerminalSurfaceProps) {
  const viewportRef = useRef<HTMLDivElement>(null);
  const runtimeRef = useRef<TerminalRuntime | null>(null);
  const initialCursorRef = useRef(initialCursor);
  const descriptionId = useId();
  const bindingsRef = useRef<TerminalRuntimeBindings>({
    accessibleLabel,
    accessibleDescriptionId: descriptionId,
    readOnly,
    screenReaderMode,
    onInput,
    onResize,
    onCursorApplied,
  });

  useLayoutEffect(() => {
    bindingsRef.current = {
      accessibleLabel,
      accessibleDescriptionId: descriptionId,
      readOnly,
      screenReaderMode,
      onInput,
      onResize,
      onCursorApplied,
    };
  }, [
    accessibleLabel,
    descriptionId,
    onCursorApplied,
    onInput,
    onResize,
    readOnly,
    screenReaderMode,
  ]);

  useLayoutEffect(() => {
    const element = viewportRef.current;
    if (!element) {
      return undefined;
    }
    const runtime = createTerminalRuntime({
      element,
      initialCursor: initialCursorRef.current,
      getBindings: () => bindingsRef.current,
    });
    runtimeRef.current = runtime;
    return () => {
      if (runtimeRef.current === runtime) {
        runtimeRef.current = null;
      }
      runtime.dispose();
    };
  }, []);

  useEffect(() => {
    runtimeRef.current?.setAccessibleLabel(accessibleLabel);
  }, [accessibleLabel]);

  useEffect(() => {
    runtimeRef.current?.setReadOnly(readOnly);
  }, [readOnly]);

  useEffect(() => {
    runtimeRef.current?.setScreenReaderMode(screenReaderMode);
  }, [screenReaderMode]);

  useImperativeHandle(
    ref,
    () => ({
      write(data) {
        const runtime = runtimeRef.current;
        if (!runtime) {
          return false;
        }
        runtime.write(data);
        return true;
      },
      writeOutput(chunk) {
        return runtimeRef.current?.writeOutput(chunk) ?? "unavailable";
      },
      markOutputGap(gap) {
        return runtimeRef.current?.markOutputGap(gap) ?? false;
      },
      markReplayComplete(outputSequence) {
        return (
          runtimeRef.current?.markReplayComplete(outputSequence) ?? false
        );
      },
      reset(cursor = 0) {
        const runtime = runtimeRef.current;
        if (!runtime) {
          return false;
        }
        runtime.reset(cursor);
        return true;
      },
      focus() {
        const runtime = runtimeRef.current;
        if (!runtime) {
          return false;
        }
        runtime.focus();
        return true;
      },
      getCursor: () => runtimeRef.current?.getCursor() ?? null,
    }),
    [ref],
  );

  const classes = className
    ? `terminal-surface ${className}`
    : "terminal-surface";

  return (
    <section
      className={classes}
      data-terminal-root="true"
      role="region"
      aria-label={accessibleLabel}
      aria-describedby={descriptionId}
    >
      <p id={descriptionId} className="visually-hidden">
        Interactive terminal. Terminal control keys are sent to the running
        process.
      </p>
      <div className="terminal-surface__viewport" ref={viewportRef} />
    </section>
  );
}
