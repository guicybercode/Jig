import type { SessionId, SessionOutputEvent, SessionOutputGapEvent } from "../../ipc";
import type { TerminalSurfaceHandle } from "../features/terminal";
import { base64ToBytes } from "./terminal-codec";

type PendingOutput =
  | { readonly kind: "output"; readonly event: SessionOutputEvent }
  | {
      readonly kind: "gap";
      readonly event: SessionOutputGapEvent;
    }
  | { readonly kind: "replay"; readonly outputSequence: number };

/** Routes PTY events to mounted xterm handles without touching React state. */
export type TerminalRegistry = {
  attach(sessionId: SessionId, handle: TerminalSurfaceHandle): void;
  detach(sessionId: SessionId, handle?: TerminalSurfaceHandle): void;
  writeOutput(event: SessionOutputEvent): void;
  markOutputGap(event: SessionOutputGapEvent): void;
  markReplayComplete(sessionId: SessionId, outputSequence: number): void;
  reset(sessionId: SessionId, cursor?: number): void;
};

function applyPending(
  handle: TerminalSurfaceHandle,
  pending: PendingOutput,
): void {
  switch (pending.kind) {
    case "output":
      handle.writeOutput({
        data: base64ToBytes(pending.event.base64),
        sequence: pending.event.outputSequence,
        replay: pending.event.replay,
      });
      return;
    case "gap":
      handle.markOutputGap({
        requestedCursor: pending.event.requestedCursor,
        firstAvailableSequence: pending.event.firstAvailableSequence,
        latestSequence: pending.event.latestSequence,
      });
      return;
    case "replay":
      handle.markReplayComplete(pending.outputSequence);
  }
}

/** Creates a session-id map of terminal handles and a short pre-mount buffer. */
export function createTerminalRegistry(): TerminalRegistry {
  const handles = new Map<SessionId, TerminalSurfaceHandle>();
  const buffers = new Map<SessionId, PendingOutput[]>();

  function enqueue(sessionId: SessionId, pending: PendingOutput): void {
    const existing = buffers.get(sessionId) ?? [];
    existing.push(pending);
    buffers.set(sessionId, existing.slice(-64));
  }

  return {
    attach(sessionId, handle) {
      handles.set(sessionId, handle);
      const pending = buffers.get(sessionId);
      if (!pending || pending.length === 0) {
        return;
      }
      buffers.delete(sessionId);
      for (const item of pending) {
        applyPending(handle, item);
      }
    },
    detach(sessionId, handle) {
      const current = handles.get(sessionId);
      if (handle && current !== handle) {
        return;
      }
      handles.delete(sessionId);
    },
    writeOutput(event) {
      const handle = handles.get(event.sessionId);
      const pending: PendingOutput = { kind: "output", event };
      if (handle) {
        applyPending(handle, pending);
        return;
      }
      enqueue(event.sessionId, pending);
    },
    markOutputGap(event) {
      const handle = handles.get(event.sessionId);
      const pending: PendingOutput = { kind: "gap", event };
      if (handle) {
        applyPending(handle, pending);
        return;
      }
      enqueue(event.sessionId, pending);
    },
    markReplayComplete(sessionId, outputSequence) {
      const handle = handles.get(sessionId);
      const pending: PendingOutput = { kind: "replay", outputSequence };
      if (handle) {
        applyPending(handle, pending);
        return;
      }
      enqueue(sessionId, pending);
    },
    reset(sessionId, cursor = 0) {
      buffers.delete(sessionId);
      handles.get(sessionId)?.reset(cursor);
    },
  };
}
