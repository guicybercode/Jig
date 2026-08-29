/** Raw data accepted by xterm without a UTF-8 round trip. */
export type TerminalOutputData = string | Uint8Array;

/** One daemon output chunk in its per-PTY monotonic sequence. */
export interface TerminalOutputChunk {
  readonly data: TerminalOutputData;
  readonly sequence: number;
  readonly replay: boolean;
}

/** Describes a range the daemon can no longer replay. */
export interface TerminalOutputGap {
  readonly requestedCursor: number;
  readonly firstAvailableSequence: number;
  readonly latestSequence: number;
}

/** Result of accepting one sequenced output chunk for xterm parsing. */
export type TerminalWriteDisposition = "queued" | "duplicate";

/** Imperative sequence guard used by a mounted terminal runtime. */
export interface TerminalOutputWriter {
  /** Writes unsequenced local data in call order. */
  write(data: TerminalOutputData): void;
  /** Writes a daemon chunk once, inserting an explicit marker for inferred gaps. */
  writeOutput(chunk: TerminalOutputChunk): TerminalWriteDisposition;
  /** Marks a daemon-reported unavailable range and prepares for retained replay. */
  markOutputGap(gap: TerminalOutputGap): boolean;
  /** Closes replay at the daemon's live boundary, marking missing chunks if needed. */
  markReplayComplete(outputSequence: number): boolean;
  /** Resets sequence tracking for a fresh PTY lifetime. */
  reset(cursor?: number): void;
  /** Returns the last output sequence already applied to xterm. */
  getCursor(): number;
  /** Ignores parse callbacks that arrive after the owning terminal is gone. */
  dispose(): void;
}

type WriteToTerminal = (
  data: TerminalOutputData,
  onParsed: () => void,
) => void;
type CursorApplied = (cursor: number) => void;

interface PendingCheckpoint {
  readonly sequence: number;
  parsed: boolean;
}

const GAP_COLOR = "\u001b[33m";
const RESET_COLOR = "\u001b[0m";
const CANCEL_INCOMPLETE_SEQUENCE = "\u0018";
const CLOSE_HYPERLINK = "\u001b]8;;\u001b\\";

/** Creates an imperative writer that keeps PTY chunks ordered and idempotent. */
export function createTerminalOutputWriter(
  writeToTerminal: WriteToTerminal,
  initialCursor = 0,
  onCursorApplied?: CursorApplied,
): TerminalOutputWriter {
  let appliedCursor = requireCursor(initialCursor, "initial cursor");
  let acceptedCursor = appliedCursor;
  let lastGapKey: string | null = null;
  let generation = 0;
  const pending: PendingCheckpoint[] = [];

  function write(data: TerminalOutputData): void {
    if (data.length === 0) {
      return;
    }
    writeToTerminal(data, () => undefined);
  }

  function writeOutput(
    chunk: TerminalOutputChunk,
  ): TerminalWriteDisposition {
    const sequence = requireOutputSequence(chunk.sequence, "output sequence");
    if (sequence <= acceptedCursor) {
      return "duplicate";
    }

    const expectedSequence = acceptedCursor + 1;
    if (sequence > expectedSequence) {
      queueGapMarker(expectedSequence, sequence - 1, "output unavailable");
    }

    queueCheckpoint(chunk.data, sequence);
    lastGapKey = null;
    return "queued";
  }

  function markOutputGap(gap: TerminalOutputGap): boolean {
    const requestedCursor = requireCursor(
      gap.requestedCursor,
      "requested cursor",
    );
    const firstAvailableSequence = requireOutputSequence(
      gap.firstAvailableSequence,
      "first available sequence",
    );
    const latestSequence = requireCursor(
      gap.latestSequence,
      "latest sequence",
    );
    if (latestSequence < firstAvailableSequence) {
      throw new RangeError(
        "latest sequence must be at least the first available sequence",
      );
    }
    if (latestSequence <= acceptedCursor) {
      return false;
    }

    const gapStart = Math.max(acceptedCursor + 1, requestedCursor + 1);
    const gapEnd = firstAvailableSequence - 1;
    const gapKey = `${gapStart}:${gapEnd}:${latestSequence}`;
    if (gapEnd >= gapStart && gapKey !== lastGapKey) {
      queueGapMarker(gapStart, gapEnd, "replay unavailable");
      lastGapKey = gapKey;
      return true;
    }
    return false;
  }

  function markReplayComplete(outputSequence: number): boolean {
    const liveSequence = requireCursor(outputSequence, "replay sequence");
    if (liveSequence <= acceptedCursor) {
      return false;
    }

    queueGapMarker(
      acceptedCursor + 1,
      liveSequence,
      "replay incomplete",
    );
    lastGapKey = null;
    return true;
  }

  function reset(nextCursor = 0): void {
    generation += 1;
    pending.length = 0;
    appliedCursor = requireCursor(nextCursor, "reset cursor");
    acceptedCursor = appliedCursor;
    lastGapKey = null;
  }

  function queueGapMarker(
    firstMissingSequence: number,
    lastMissingSequence: number,
    reason: string,
  ): void {
    const range =
      firstMissingSequence === lastMissingSequence
        ? `${firstMissingSequence}`
        : `${firstMissingSequence}-${lastMissingSequence}`;
    queueCheckpoint(
      `${CANCEL_INCOMPLETE_SEQUENCE}${RESET_COLOR}${CLOSE_HYPERLINK}\r\n` +
        `${GAP_COLOR}[CLI Master: ${reason}; output ${range}]${RESET_COLOR}\r\n`,
      lastMissingSequence,
    );
  }

  function queueCheckpoint(
    data: TerminalOutputData,
    sequence: number,
  ): void {
    const checkpoint: PendingCheckpoint = { sequence, parsed: false };
    const callbackGeneration = generation;
    const previousAcceptedCursor = acceptedCursor;
    pending.push(checkpoint);
    acceptedCursor = sequence;
    try {
      if (data.length === 0) {
        checkpoint.parsed = true;
        advanceAppliedCursor();
      } else {
        writeToTerminal(data, () => {
          if (callbackGeneration !== generation) {
            return;
          }
          checkpoint.parsed = true;
          advanceAppliedCursor();
        });
      }
    } catch (error) {
      acceptedCursor = previousAcceptedCursor;
      const checkpointIndex = pending.indexOf(checkpoint);
      if (checkpointIndex >= 0) {
        pending.splice(checkpointIndex, 1);
      }
      throw error;
    }
  }

  function advanceAppliedCursor(): void {
    let advanced = false;
    while (pending[0]?.parsed) {
      const checkpoint = pending.shift();
      if (checkpoint) {
        appliedCursor = checkpoint.sequence;
        advanced = true;
      }
    }
    if (advanced) {
      onCursorApplied?.(appliedCursor);
    }
  }

  function dispose(): void {
    generation += 1;
    pending.length = 0;
  }

  return {
    write,
    writeOutput,
    markOutputGap,
    markReplayComplete,
    reset,
    getCursor: () => appliedCursor,
    dispose,
  };
}

function requireCursor(value: number, field: string): number {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new RangeError(`${field} must be a non-negative safe integer`);
  }
  return value;
}

function requireOutputSequence(value: number, field: string): number {
  if (!Number.isSafeInteger(value) || value < 1) {
    throw new RangeError(`${field} must be a positive safe integer`);
  }
  return value;
}
