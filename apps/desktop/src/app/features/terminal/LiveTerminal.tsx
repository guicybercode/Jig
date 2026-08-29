import { useEffect, useRef, useState } from "react";

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
import type { TerminalInput } from "./terminal-runtime";

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
}

/** Connects one mounted xterm directly to one daemon-owned PTY session. */
export function LiveTerminal({
  session,
  className,
  autoFocus = false,
  subscribeTerminal,
  writeTerminal,
  resizeTerminal,
}: LiveTerminalProps) {
  const terminalRef = useRef<TerminalSurfaceHandle>(null);
  const writeQueueRef = useRef<Promise<void>>(Promise.resolve());
  const [terminalError, setTerminalError] = useState<ApiErrorData>();
  const live = isLiveStatus(session.status);

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
  }, [autoFocus, live, session.id, subscribeTerminal]);

  function handleInput(input: TerminalInput) {
    const bytes = terminalInputBytes(input);
    writeQueueRef.current = writeQueueRef.current
      .then(() => writeTerminal(session.id, bytes))
      .catch((error: unknown) => {
        setTerminalError(errorData(error));
      });
  }

  return (
    <div className={className ? `live-terminal ${className}` : "live-terminal"}>
      <TerminalSurface
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
