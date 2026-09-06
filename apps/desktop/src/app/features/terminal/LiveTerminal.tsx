import {
  useCallback,
  useEffect,
  useImperativeHandle,
  useLayoutEffect,
  useRef,
  useState,
  type Ref,
} from "react";

import type {
  IpcEventErrorHandler,
  IpcEventHandler,
  TerminalResizeInput,
  TerminalSubscriptionInput,
  Unsubscribe,
} from "../../../ipc/client";
import { requireBoolean, requireNumber, requireRecord, requireString } from "../../../ipc/schema";
import type { ApiErrorData, EventEnvelope, Session } from "../../../ipc/types";
import { errorData, isLiveStatus } from "../../utils";
import { TerminalSurface, type TerminalSurfaceHandle } from "./TerminalSurface";
import type { TerminalInput, TerminalInputModes } from "./terminal-runtime";

/** Shares the terminal input queue without retaining PTY bytes in React state. */
export interface LiveTerminalInputHandle {
  /** Encodes against the modes at delivery and rejects if the target changed. */
  writeInput(encode: (modes: TerminalInputModes) => Uint8Array): Promise<void>;
}

export interface LiveTerminalTransport {
  readonly subscribeTerminal: (
    input: TerminalSubscriptionInput,
    handler: IpcEventHandler,
    onError: IpcEventErrorHandler,
  ) => Promise<Unsubscribe>;
  readonly writeTerminal: (sessionId: string, bytes: Uint8Array) => Promise<void>;
  readonly resizeTerminal: (input: TerminalResizeInput) => Promise<void>;
}

interface LiveTerminalProps extends LiveTerminalTransport {
  readonly session: Session;
  readonly className?: string;
  readonly autoFocus?: boolean;
  /** Imperative input access for a composer associated with this exact session. */
  readonly inputRef?: Ref<LiveTerminalInputHandle>;
}

interface TerminalInputTarget {
  readonly sessionId: string;
  readonly ptyId: string | undefined;
  readonly live: boolean;
  writeQueue: Promise<void>;
}

/** Connects one mounted xterm directly to one daemon-owned PTY session. */
export function LiveTerminal({
  session,
  className,
  autoFocus = false,
  inputRef,
  subscribeTerminal,
  writeTerminal,
  resizeTerminal,
}: LiveTerminalProps) {
  const terminalRef = useRef<TerminalSurfaceHandle>(null);
  const inputTargetRef = useRef<TerminalInputTarget | null>(null);
  const writeTerminalRef = useRef(writeTerminal);
  const [terminalError, setTerminalError] = useState<ApiErrorData>();
  const live = isLiveStatus(session.status);

  useLayoutEffect(() => {
    writeTerminalRef.current = writeTerminal;
  }, [writeTerminal]);

  useLayoutEffect(() => {
    const target: TerminalInputTarget = {
      sessionId: session.id,
      ptyId: session.ptyId,
      live,
      writeQueue: Promise.resolve(),
    };
    inputTargetRef.current = target;
    return () => {
      if (inputTargetRef.current === target) inputTargetRef.current = null;
    };
  }, [live, session.id, session.ptyId]);

  const enqueueInput = useCallback((
    target: TerminalInputTarget | null,
    encode: (modes: TerminalInputModes) => Uint8Array,
  ): Promise<void> => {
    if (!target || inputTargetRef.current !== target || !target.live) {
      return Promise.reject(new Error("The terminal is no longer available for input."));
    }
    const write = target.writeQueue.then(() => {
      if (inputTargetRef.current !== target || !target.live) {
        throw new Error("The terminal session changed before input could be sent.");
      }
      const modes = terminalRef.current?.getInputModes();
      if (!modes) throw new Error("The terminal is not ready for input.");
      return writeTerminalRef.current(target.sessionId, encode(modes));
    });
    // A failed input must reject its caller without poisoning subsequent writes.
    target.writeQueue = write.catch(() => undefined);
    return write;
  }, []);

  useImperativeHandle(inputRef, () => {
    const target = inputTargetRef.current;
    return { writeInput: (encode) => enqueueInput(target, encode) };
  }, [enqueueInput, live, session.id, session.ptyId]);

  useEffect(() => {
    let active = true;
    let unsubscribe: Unsubscribe | undefined;
    if (!live) {
      return undefined;
    }
    const cursor = terminalRef.current?.getCursor() ?? 0;
    void subscribeTerminal(
      { sessionId: session.id, cursor },
      (event) => {
        if (active) {
          applyTerminalEvent(terminalRef.current, event);
        }
      },
      (error) => {
        if (active) {
          setTerminalError(errorData(error));
        }
      },
    )
      .then((nextUnsubscribe) => {
        if (!active) {
          nextUnsubscribe();
          return;
        }
        unsubscribe = nextUnsubscribe;
        setTerminalError(undefined);
        if (autoFocus) {
          terminalRef.current?.focus();
        }
      })
      .catch((error: unknown) => {
        if (active) {
          setTerminalError(errorData(error));
        }
      });
    return () => {
      active = false;
      unsubscribe?.();
    };
  }, [autoFocus, live, session.id, session.ptyId, subscribeTerminal]);

  function handleInput(input: TerminalInput) {
    const target = inputTargetRef.current;
    if (target?.sessionId !== session.id || target?.ptyId !== session.ptyId) return;
    void enqueueInput(target, () => terminalInputBytes(input)).catch((error: unknown) => {
      if (inputTargetRef.current === target) {
        setTerminalError(errorData(error));
      }
    });
  }

  return (
    <div className={className ? `live-terminal ${className}` : "live-terminal"}>
      <TerminalSurface
        key={`${session.id}:${session.ptyId ?? "pending"}`}
        ref={terminalRef}
        accessibleLabel={`Interactive terminal for ${session.name}`}
        readOnly={!live}
        onInput={handleInput}
        onResize={({ cols, rows }) => {
          if (!live) {
            return;
          }
          void resizeTerminal({
            sessionId: session.id,
            columns: cols,
            rows,
          }).catch((error: unknown) => setTerminalError(errorData(error)));
        }}
      />
      {terminalError ? (
        <div className="live-terminal__error" role="alert">
          <strong>{terminalError.message}</strong>
          {terminalError.action ? <span>{terminalError.action}</span> : null}
        </div>
      ) : null}
    </div>
  );
}

function applyTerminalEvent(
  terminal: TerminalSurfaceHandle | null,
  event: EventEnvelope,
) {
  if (!terminal) {
    return;
  }
  const payload = requireRecord(event.payload, `${event.event} payload`);
  switch (event.event) {
    case "session.output":
      terminal.writeOutput({
        data: decodeBase64(requireString(payload.base64, "output.base64")),
        sequence: requireNumber(payload.outputSequence, "output.outputSequence"),
        replay: requireBoolean(payload.replay, "output.replay"),
      });
      break;
    case "session.output_gap":
      terminal.markOutputGap({
        requestedCursor: requireNumber(payload.requestedCursor, "gap.requestedCursor"),
        firstAvailableSequence: requireNumber(
          payload.firstAvailableSequence,
          "gap.firstAvailableSequence",
        ),
        latestSequence: requireNumber(payload.latestSequence, "gap.latestSequence"),
      });
      break;
    case "session.replay_complete":
      terminal.markReplayComplete(
        requireNumber(payload.outputSequence, "replay.outputSequence"),
      );
      break;
  }
}

function terminalInputBytes(input: TerminalInput): Uint8Array {
  if (input.kind === "text") {
    return new TextEncoder().encode(input.data);
  }
  return Uint8Array.from(input.data, (character) => character.charCodeAt(0) & 0xff);
}

function decodeBase64(encoded: string): Uint8Array {
  const binary = globalThis.atob(encoded);
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}
