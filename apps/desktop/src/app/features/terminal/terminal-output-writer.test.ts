import { describe, expect, it, vi } from "vitest";

import {
  createTerminalOutputWriter,
  type TerminalOutputData,
} from "./terminal-output-writer";

interface PendingWrite {
  readonly data: TerminalOutputData;
  readonly onParsed: () => void;
}

describe("createTerminalOutputWriter", () => {
  it("advances only across the highest contiguous parsed checkpoint", () => {
    const pending: PendingWrite[] = [];
    const onCursorApplied = vi.fn();
    const writer = createTerminalOutputWriter(
      (data, onParsed) => pending.push({ data, onParsed }),
      0,
      onCursorApplied,
    );

    expect(
      writer.writeOutput({ data: "one", sequence: 1, replay: true }),
    ).toBe("queued");
    expect(
      writer.writeOutput({ data: "duplicate", sequence: 1, replay: false }),
    ).toBe("duplicate");
    expect(
      writer.writeOutput({ data: "two", sequence: 2, replay: true }),
    ).toBe("queued");
    expect(writer.getCursor()).toBe(0);

    requirePending(pending, 1).onParsed();
    expect(writer.getCursor()).toBe(0);
    requirePending(pending, 0).onParsed();

    expect(writer.getCursor()).toBe(2);
    expect(onCursorApplied).toHaveBeenLastCalledWith(2);
  });

  it("preserves fragmented bytes and applies inferred/replay gaps in order", () => {
    const pending: PendingWrite[] = [];
    const writer = createTerminalOutputWriter((data, onParsed) => {
      pending.push({ data, onParsed });
    });
    const utf8Prefix = new Uint8Array([0xe2]);
    const utf8Suffix = new Uint8Array([0x82, 0xac]);

    expect(
      writer.writeOutput({ data: utf8Prefix, sequence: 1, replay: true }),
    ).toBe("queued");
    expect(
      writer.writeOutput({ data: utf8Suffix, sequence: 2, replay: true }),
    ).toBe("queued");
    expect(
      writer.writeOutput({ data: "after gap", sequence: 4, replay: false }),
    ).toBe("queued");

    expect(requirePending(pending, 0).data).toBe(utf8Prefix);
    expect(requirePending(pending, 1).data).toBe(utf8Suffix);
    expect(requirePending(pending, 2).data).toContain(
      "output unavailable; output 3",
    );
    expect(requirePending(pending, 2).data).toMatch(
      /^\u0018\u001b\[0m/,
    );
    expect(requirePending(pending, 3).data).toBe("after gap");

    flushPending(pending);
    expect(writer.getCursor()).toBe(4);
    expect(
      writer.markOutputGap({
        requestedCursor: 4,
        firstAvailableSequence: 7,
        latestSequence: 8,
      }),
    ).toBe(true);
    expect(
      writer.writeOutput({ data: "retained", sequence: 7, replay: true }),
    ).toBe("queued");
    expect(writer.markReplayComplete(8)).toBe(true);
    expect(writer.getCursor()).toBe(4);

    expect(requirePending(pending, 0).data).toContain(
      "replay unavailable; output 5-6",
    );
    expect(requirePending(pending, 1).data).toBe("retained");
    expect(requirePending(pending, 2).data).toContain(
      "replay incomplete; output 8",
    );
    flushPending(pending);
    expect(writer.getCursor()).toBe(8);
  });

  it("ignores callbacks from a reset or disposed terminal lifetime", () => {
    const pending: PendingWrite[] = [];
    const onCursorApplied = vi.fn();
    const writer = createTerminalOutputWriter(
      (data, onParsed) => pending.push({ data, onParsed }),
      0,
      onCursorApplied,
    );

    writer.writeOutput({ data: "old", sequence: 1, replay: false });
    const oldCallback = requirePending(pending, 0).onParsed;
    writer.reset(12);
    oldCallback();
    expect(writer.getCursor()).toBe(12);
    expect(onCursorApplied).not.toHaveBeenCalled();

    writer.writeOutput({ data: "new", sequence: 13, replay: false });
    const disposedCallback = requirePending(pending, 1).onParsed;
    writer.dispose();
    disposedCallback();
    expect(writer.getCursor()).toBe(12);
    expect(onCursorApplied).not.toHaveBeenCalled();
  });

  it("does not accept a sequence when xterm rejects the write synchronously", () => {
    let shouldThrow = true;
    const writer = createTerminalOutputWriter((_data, onParsed) => {
      if (shouldThrow) {
        throw new Error("xterm unavailable");
      }
      onParsed();
    });

    expect(() =>
      writer.writeOutput({ data: "retry", sequence: 1, replay: false }),
    ).toThrow("xterm unavailable");
    shouldThrow = false;
    expect(
      writer.writeOutput({ data: "retry", sequence: 1, replay: false }),
    ).toBe("queued");
    expect(writer.getCursor()).toBe(1);
  });
});

function flushPending(pending: PendingWrite[]): void {
  for (const write of pending.splice(0)) {
    write.onParsed();
  }
}

function requirePending(
  pending: readonly PendingWrite[],
  index: number,
): PendingWrite {
  const write = pending[index];
  if (!write) {
    throw new Error(`Expected pending write ${index}.`);
  }
  return write;
}
