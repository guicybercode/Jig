import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { CANVAS_STORAGE_KEY } from "./canvas-state";
import { CanvasWorkspace } from "./CanvasWorkspace";

describe("CanvasWorkspace", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("renders the first-launch terminal and note composition", () => {
    renderCanvas();

    expect(
      screen.getByRole("heading", { name: "My Workspace", level: 1 }),
    ).toBeVisible();
    expect(
      screen.getAllByRole("region", { name: /Terminal surface/ }),
    ).toHaveLength(2);
    expect(screen.getByRole("textbox", { name: "Notes content" })).toHaveValue(
      "Write a note for this workspace…",
    );
    expect(screen.getByText("Canvas saved locally")).toBeVisible();
  });

  it("adds notes, creates a two-click connection, and resets the layout", async () => {
    const user = userEvent.setup();
    renderCanvas();

    await user.click(screen.getByRole("button", { name: "Add note" }));
    expect(screen.getAllByRole("textbox", { name: /Notes content/ })).toHaveLength(
      2,
    );

    const secondTerminal = screen.getByRole("article", {
      name: "Terminal 2, terminal canvas item",
    });
    await user.click(
      within(secondTerminal).getByRole("button", {
        name: "Start connection from Terminal 2",
      }),
    );
    const addedNote = screen.getAllByRole("article", {
      name: "Notes, note canvas item",
    })[1];
    expect(addedNote).toBeDefined();
    await user.click(
      within(addedNote as HTMLElement).getByRole("button", {
        name: "Connect to Notes",
      }),
    );

    await waitFor(() => {
      const persisted = JSON.parse(
        localStorage.getItem(CANVAS_STORAGE_KEY) ?? "{}",
      ) as { connections?: readonly unknown[] };
      expect(persisted.connections).toHaveLength(3);
    });

    await user.click(
      screen.getByRole("button", { name: "Reset canvas layout" }),
    );
    expect(screen.getAllByRole("textbox", { name: /Notes content/ })).toHaveLength(
      1,
    );
  });
});

function renderCanvas() {
  return render(
    <CanvasWorkspace
      isConnected
      projects={[]}
      sessions={[]}
      onAddProject={vi.fn()}
      onNewSession={vi.fn()}
      onSelectSession={vi.fn()}
    />,
  );
}
