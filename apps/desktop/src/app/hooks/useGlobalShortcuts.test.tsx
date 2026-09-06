import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { useGlobalShortcuts } from "./useGlobalShortcuts";

describe("useGlobalShortcuts", () => {
  it("leaves primary shortcuts inside an embedded keyboard scope", () => {
    const onNewSession = vi.fn();
    render(<ShortcutHarness onNewSession={onNewSession} />);

    fireEvent.keyDown(screen.getByLabelText("Browser address"), {
      key: "t",
      ctrlKey: true,
    });
    expect(onNewSession).not.toHaveBeenCalled();

    fireEvent.keyDown(screen.getByRole("button", { name: "Canvas control" }), {
      key: "t",
      ctrlKey: true,
    });
    expect(onNewSession).toHaveBeenCalledOnce();
  });

  it("continues to leave terminal shortcuts untouched", () => {
    const onNewSession = vi.fn();
    render(<ShortcutHarness onNewSession={onNewSession} />);

    fireEvent.keyDown(screen.getByLabelText("Terminal input"), {
      key: "t",
      ctrlKey: true,
    });

    expect(onNewSession).not.toHaveBeenCalled();
  });
});

function ShortcutHarness({
  onNewSession,
}: {
  readonly onNewSession: () => void;
}) {
  useGlobalShortcuts({ platform: "linux", onNewSession });
  return (
    <>
      <button type="button">Canvas control</button>
      <div data-shortcut-scope="true">
        <input aria-label="Browser address" />
      </div>
      <div data-terminal-root="true">
        <textarea aria-label="Terminal input" />
      </div>
    </>
  );
}
