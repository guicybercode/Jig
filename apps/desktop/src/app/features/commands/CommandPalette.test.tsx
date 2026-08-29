import { useState } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import {
  CommandPalette,
  type CommandPaletteCommand,
} from "./CommandPalette";

interface CommandPaletteHarnessProps {
  readonly commands: readonly CommandPaletteCommand[];
}

function CommandPaletteHarness({ commands }: CommandPaletteHarnessProps) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>
        Open commands
      </button>
      <CommandPalette
        open={open}
        commands={commands}
        onClose={() => setOpen(false)}
      />
    </>
  );
}

describe("CommandPalette", () => {
  it("skips unavailable results and restores focus after keyboard selection", async () => {
    const user = userEvent.setup();
    const runSession = vi.fn();
    const reconnect = vi.fn();
    const commands: readonly CommandPaletteCommand[] = [
      { id: "session.create", label: "New Session", onSelect: runSession },
      {
        id: "agent.create",
        label: "Add Custom Agent",
        onSelect: vi.fn(),
        disabled: true,
        disabledReason: "Connect the local daemon first.",
      },
      { id: "daemon.reconnect", label: "Reconnect", onSelect: reconnect },
    ];
    render(<CommandPaletteHarness commands={commands} />);

    const opener = screen.getByRole("button", { name: "Open commands" });
    await user.click(opener);

    const search = screen.getByRole("searchbox", { name: "Search commands" });
    expect(search).toHaveFocus();
    expect(screen.getByRole("status")).toHaveTextContent(
      "3 matching commands; 2 available.",
    );
    expect(
      screen.getByRole("button", { name: "Add Custom Agent" }),
    ).toHaveAccessibleDescription("Connect the local daemon first.");

    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("button", { name: "New Session" })).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("button", { name: "Reconnect" })).toHaveFocus();
    await user.keyboard("{Enter}");

    expect(reconnect).toHaveBeenCalledOnce();
    expect(runSession).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(opener).toHaveFocus();
  });

  it("filters commands and explains an empty result", async () => {
    const user = userEvent.setup();
    render(
      <CommandPaletteHarness
        commands={[
          { id: "session.create", label: "New Session", onSelect: vi.fn() },
          { id: "daemon.reconnect", label: "Reconnect", onSelect: vi.fn() },
        ]}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Open commands" }));
    const search = screen.getByRole("searchbox", { name: "Search commands" });
    await user.type(search, "missing");

    expect(screen.getByRole("status")).toHaveTextContent(
      "0 matching commands; 0 available.",
    );
    expect(screen.getByText("No matching commands.")).toBeVisible();
    await user.keyboard("{ArrowDown}");
    expect(search).toHaveFocus();
  });
});
