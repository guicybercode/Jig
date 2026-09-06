import { createRef, useImperativeHandle, type ComponentProps } from "react";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { IpcError } from "../../../ipc/client";
import type { Session } from "../../../ipc/types";
import { LiveTerminal, type LiveTerminalInputHandle, type LiveTerminalTransport } from "./LiveTerminal";
import type { TerminalSurfaceProps } from "./TerminalSurface";
import type { TerminalInputModes } from "./terminal-runtime";

const surface = vi.hoisted((): { modes: TerminalInputModes | null } => ({
  modes: { bracketedPasteMode: false, applicationCursorKeysMode: false },
}));

vi.mock("./TerminalSurface", () => ({
  TerminalSurface({ ref, accessibleLabel, readOnly, onInput }: TerminalSurfaceProps) {
    useImperativeHandle(ref, () => ({
      write: () => true,
      writeOutput: () => "queued",
      markOutputGap: () => true,
      markReplayComplete: () => true,
      reset: () => true,
      focus: () => true,
      getCursor: () => 0,
      getInputModes: () => surface.modes,
    }));
    return (
      <section aria-label={accessibleLabel}>
        <button type="button" disabled={readOnly} onClick={() => onInput?.({ kind: "text", data: "keyboard" })}>
          Type into terminal
        </button>
        <button type="button" disabled={readOnly} onClick={() => onInput?.({ kind: "binary", data: "\u0000\u00ff" })}>
          Send binary input
        </button>
      </section>
    );
  },
}));

const SESSION: Session = {
  id: "0198f000-0000-7000-8000-000000000001",
  projectId: "0198f000-0000-7000-8000-000000000002",
  agentId: "0198f000-0000-7000-8000-000000000003",
  name: "Review agent",
  cwd: "/workspace/project",
  status: "running",
  ptyId: "pty-first",
  createdAtMs: 1,
  updatedAtMs: 1,
};

describe("LiveTerminal input handle", () => {
  beforeEach(() => {
    surface.modes = { bracketedPasteMode: false, applicationCursorKeysMode: false };
  });

  it("serializes keyboard, composed, and binary writes and reads modes at delivery", async () => {
    const pending = deferred();
    const writeTerminal = vi.fn<LiveTerminalTransport["writeTerminal"]>(async () => undefined).mockImplementationOnce(() => pending.promise);
    const { inputRef } = renderTerminal({ writeTerminal });
    const handle = requireHandle(inputRef.current);
    const encode = vi.fn((modes: TerminalInputModes) => Uint8Array.of(
      Number(modes.bracketedPasteMode), Number(modes.applicationCursorKeysMode),
    ));

    fireEvent.click(screen.getByRole("button", { name: "Type into terminal" }));
    await waitFor(() => expect(writeTerminal).toHaveBeenCalledOnce());
    const composed = handle.writeInput(encode);
    fireEvent.click(screen.getByRole("button", { name: "Send binary input" }));
    expect(encode).not.toHaveBeenCalled();
    expect(writeTerminal).toHaveBeenCalledOnce();
    surface.modes = { bracketedPasteMode: true, applicationCursorKeysMode: true };

    await act(async () => {
      pending.resolve();
      await composed;
    });
    await waitFor(() => expect(writeTerminal).toHaveBeenCalledTimes(3));
    expect(encode).toHaveBeenCalledWith(surface.modes);
    expect(writeTerminal.mock.calls).toEqual([
      [SESSION.id, new TextEncoder().encode("keyboard")],
      [SESSION.id, Uint8Array.of(1, 1)],
      [SESSION.id, Uint8Array.of(0, 255)],
    ]);
  });

  it("rejects composer failures while preserving keyboard error feedback and queue recovery", async () => {
    const failure = new IpcError({ code: "write_failed", message: "Input temporarily unavailable" });
    const writeTerminal = vi.fn<LiveTerminalTransport["writeTerminal"]>(async () => undefined)
      .mockRejectedValueOnce(failure)
      .mockRejectedValueOnce(failure);
    const { inputRef } = renderTerminal({ writeTerminal });
    const handle = requireHandle(inputRef.current);

    await expect(handle.writeInput(() => Uint8Array.of(1))).rejects.toBe(failure);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Type into terminal" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Input temporarily unavailable");
    await expect(handle.writeInput(() => Uint8Array.of(2))).resolves.toBeUndefined();
    expect(writeTerminal).toHaveBeenLastCalledWith(SESSION.id, Uint8Array.of(2));
  });

  it.each([
    { description: "switching session", next: { ...SESSION, id: "0198f000-0000-7000-8000-000000000004" } },
    { description: "replacing the PTY", next: { ...SESSION, ptyId: "pty-restarted" } },
  ])("discards queued input after $description and gives the new target its own queue", async ({ next }) => {
    const pending = deferred();
    const writeTerminal = vi.fn<LiveTerminalTransport["writeTerminal"]>(async () => undefined).mockImplementationOnce(() => pending.promise);
    const { inputRef, props, rerender } = renderTerminal({ writeTerminal });
    const handle = requireHandle(inputRef.current);
    const first = handle.writeInput(() => Uint8Array.of(1));
    await waitFor(() => expect(writeTerminal).toHaveBeenCalledOnce());
    const encodeStale = vi.fn(() => Uint8Array.of(2));
    const stale = handle.writeInput(encodeStale);
    const staleRejection = expect(stale).rejects.toThrow("session changed");
    fireEvent.click(screen.getByRole("button", { name: "Type into terminal" }));

    rerender(<LiveTerminal {...props} session={next} />);
    await expect(handle.writeInput(encodeStale)).rejects.toThrow("no longer available");
    await expect(requireHandle(inputRef.current).writeInput(() => Uint8Array.of(3))).resolves.toBeUndefined();
    expect(writeTerminal).toHaveBeenLastCalledWith(next.id, Uint8Array.of(3));
    await act(async () => {
      pending.resolve();
      await first;
      await staleRejection;
    });
    expect(encodeStale).not.toHaveBeenCalled();
    expect(writeTerminal).toHaveBeenCalledTimes(2);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("invalidates retained and queued input when the terminal is unmounted", async () => {
    const pending = deferred();
    const writeTerminal = vi.fn<LiveTerminalTransport["writeTerminal"]>(async () => undefined).mockImplementationOnce(() => pending.promise);
    const { inputRef, unmount } = renderTerminal({ writeTerminal });
    const handle = requireHandle(inputRef.current);
    const first = handle.writeInput(() => Uint8Array.of(1));
    await waitFor(() => expect(writeTerminal).toHaveBeenCalledOnce());
    const encodeStale = vi.fn(() => Uint8Array.of(2));
    const queued = expect(handle.writeInput(encodeStale)).rejects.toThrow("session changed");

    unmount();
    expect(inputRef.current).toBeNull();
    await expect(handle.writeInput(encodeStale)).rejects.toThrow("no longer available");
    pending.resolve();
    await first;
    await queued;
    expect(encodeStale).not.toHaveBeenCalled();
    expect(writeTerminal).toHaveBeenCalledOnce();
  });

  it("rejects input when the session stops or its surface cannot report modes", async () => {
    const writeTerminal = vi.fn<LiveTerminalTransport["writeTerminal"]>(async () => undefined);
    const { inputRef, props, rerender } = renderTerminal({ writeTerminal });
    surface.modes = null;
    const encode = vi.fn(() => Uint8Array.of(1));
    await expect(requireHandle(inputRef.current).writeInput(encode)).rejects.toThrow("not ready");
    surface.modes = { bracketedPasteMode: false, applicationCursorKeysMode: false };
    rerender(<LiveTerminal {...props} session={{ ...SESSION, status: "exited" }} />);
    await expect(requireHandle(inputRef.current).writeInput(encode)).rejects.toThrow("no longer available");
    expect(encode).not.toHaveBeenCalled();
    expect(writeTerminal).not.toHaveBeenCalled();
  });
});

function renderTerminal(overrides: Partial<ComponentProps<typeof LiveTerminal>> = {}) {
  const inputRef = createRef<LiveTerminalInputHandle>();
  const props: ComponentProps<typeof LiveTerminal> = {
    session: SESSION,
    inputRef,
    subscribeTerminal: vi.fn(async () => vi.fn()),
    writeTerminal: vi.fn(async () => undefined),
    resizeTerminal: vi.fn(async () => undefined),
    ...overrides,
  };
  return { ...render(<LiveTerminal {...props} />), inputRef, props };
}

function requireHandle(handle: LiveTerminalInputHandle | null): LiveTerminalInputHandle {
  if (!handle) throw new Error("Expected a mounted terminal input handle.");
  return handle;
}

function deferred() {
  let resolve: () => void = () => undefined;
  const promise = new Promise<void>((complete) => { resolve = complete; });
  return { promise, resolve };
}
