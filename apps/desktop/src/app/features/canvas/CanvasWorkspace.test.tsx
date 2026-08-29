import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { CANVAS_STORAGE_KEY } from "./canvas-state";
import { CanvasWorkspace } from "./CanvasWorkspace";

describe("CanvasWorkspace", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("renders the first-launch terminal and note composition", () => {
    const { container } = renderCanvas();

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
    expect(screen.queryByText(/Daemon (connected|offline)/)).not.toBeInTheDocument();
    expect(
      container.querySelectorAll("[data-connection-id]"),
    ).toHaveLength(2);
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
    expect(
      document.querySelectorAll("[data-connection-id]"),
    ).toHaveLength(3);

    await user.click(
      screen.getByRole("button", { name: "Reset canvas layout" }),
    );
    expect(screen.getAllByRole("textbox", { name: /Notes content/ })).toHaveLength(
      1,
    );
  });

  it("configures a Codex terminal from the terminal tool", async () => {
    const user = userEvent.setup();
    renderCanvas();

    await user.click(screen.getByRole("button", { name: "Add terminal card" }));
    const dialog = screen.getByRole("dialog", { name: "New Terminal" });
    await user.click(within(dialog).getByRole("radio", { name: "Codex" }));
    expect(within(dialog).getByLabelText("Terminal name")).toHaveValue("Codex");
    expect(within(dialog).getByLabelText("Command")).toHaveValue("codex");
    await user.click(
      within(dialog).getByRole("button", { name: "Create terminal" }),
    );

    expect(
      screen.getByRole("article", { name: "Codex, terminal canvas item" }),
    ).toBeVisible();
    await waitFor(() => {
      const persisted = JSON.parse(
        localStorage.getItem(CANVAS_STORAGE_KEY) ?? "{}",
      ) as {
        nodes?: readonly { readonly title?: string; readonly preset?: string }[];
      };
      expect(persisted.nodes).toContainEqual(
        expect.objectContaining({ title: "Codex", preset: "codex" }),
      );
    });
  });

  it("moves a selected node with keyboard and pointer alternatives", async () => {
    const user = userEvent.setup();
    const { container } = renderCanvas();
    const terminal = screen.getByRole("article", {
      name: "Terminal 1, terminal canvas item",
    });

    terminal.focus();
    await user.keyboard("{ArrowRight}{Alt>}{ArrowUp}{/Alt}");

    await waitFor(() => {
      expect(readNodePosition("terminal-primary")).toEqual({ x: 178, y: 209 });
    });

    const header = terminal.querySelector<HTMLElement>(".canvas-node__header");
    expect(header).not.toBeNull();
    fireEvent.pointerDown(header!, {
      pointerId: 7,
      clientX: 100,
      clientY: 100,
    });
    fireEvent.pointerMove(header!, {
      pointerId: 7,
      clientX: 132,
      clientY: 124,
    });
    fireEvent.pointerUp(header!, {
      pointerId: 7,
      clientX: 132,
      clientY: 124,
    });

    await waitFor(() => {
      expect(readNodePosition("terminal-primary")).toEqual({ x: 210, y: 233 });
    });
    expect(container.querySelector(".canvas-node--selected")).toBe(terminal);
  });

  it("resizes a terminal by keyboard and pointer", async () => {
    const user = userEvent.setup();
    renderCanvas();
    const terminal = screen.getByRole("article", {
      name: "Terminal 1, terminal canvas item",
    });
    await user.click(terminal);
    const handle = within(terminal).getByRole("button", {
      name: "Resize Terminal 1",
    });

    handle.focus();
    await user.keyboard("{ArrowRight}{Alt>}{ArrowDown}{/Alt}");
    await waitFor(() => {
      expect(readTerminalSize("terminal-primary")).toEqual({
        width: 448,
        height: 257,
      });
    });

    fireEvent.pointerDown(handle, {
      pointerId: 11,
      clientX: 100,
      clientY: 100,
    });
    fireEvent.pointerMove(handle, {
      pointerId: 11,
      clientX: 164,
      clientY: 132,
    });
    fireEvent.pointerUp(handle, {
      pointerId: 11,
      clientX: 164,
      clientY: 132,
    });

    await waitFor(() => {
      expect(readTerminalSize("terminal-primary")).toEqual({
        width: 512,
        height: 289,
      });
    });
    expect(terminal).toHaveStyle({ width: "512px", height: "289px" });
  });

  it("shows canvas items and fits them from the reference controls", async () => {
    const user = userEvent.setup();
    const { container } = renderCanvas();
    const viewport = container.querySelector<HTMLElement>(".canvas-viewport");
    expect(viewport).not.toBeNull();
    const scrollTo = vi.fn();
    Object.defineProperties(viewport!, {
      clientWidth: { configurable: true, value: 1000 },
      clientHeight: { configurable: true, value: 700 },
      scrollTo: { configurable: true, value: scrollTo },
    });

    await user.click(screen.getByRole("button", { name: "Show canvas items" }));
    const panel = screen.getByRole("region", { name: "Canvas items" });
    expect(within(panel).getByRole("button", { name: /Terminal 1/ })).toBeVisible();
    expect(within(panel).getByRole("button", { name: /Notes/ })).toBeVisible();

    await user.click(screen.getByRole("button", { name: "Fit canvas to items" }));
    expect(scrollTo).toHaveBeenCalledOnce();
  });

  it("pans the canvas up, down, left, and right", () => {
    const { container } = renderCanvas();
    const viewport = container.querySelector<HTMLElement>(".canvas-viewport");
    expect(viewport).not.toBeNull();
    viewport!.scrollLeft = 2_000;
    viewport!.scrollTop = 1_500;
    viewport!.focus();

    fireEvent.keyDown(viewport!, { key: "ArrowLeft" });
    fireEvent.keyDown(viewport!, { key: "ArrowUp" });
    expect(viewport!.scrollLeft).toBe(1_920);
    expect(viewport!.scrollTop).toBe(1_420);
    fireEvent.keyDown(viewport!, { key: "ArrowRight" });
    fireEvent.keyDown(viewport!, { key: "ArrowDown" });
    expect(viewport!.scrollLeft).toBe(2_000);
    expect(viewport!.scrollTop).toBe(1_500);

    fireEvent.pointerDown(viewport!, {
      button: 0,
      pointerId: 19,
      clientX: 100,
      clientY: 100,
    });
    fireEvent.pointerMove(viewport!, {
      pointerId: 19,
      clientX: 140,
      clientY: 160,
    });
    fireEvent.pointerUp(viewport!, {
      pointerId: 19,
      clientX: 140,
      clientY: 160,
    });
    expect(viewport!.scrollLeft).toBe(1_960);
    expect(viewport!.scrollTop).toBe(1_440);
  });

  it("removes a selected item's connection from the inspector", async () => {
    const user = userEvent.setup();
    const { container } = renderCanvas();
    const terminal = screen.getByRole("article", {
      name: "Terminal 1, terminal canvas item",
    });

    await user.click(terminal);
    const inspector = screen.getByRole("region", {
      name: "Connections for Terminal 1",
    });
    await user.click(
      within(inspector).getByRole("button", {
        name: "Remove connection to Notes",
      }),
    );

    await waitFor(() => {
      const persisted = JSON.parse(
        localStorage.getItem(CANVAS_STORAGE_KEY) ?? "{}",
      ) as { connections?: readonly unknown[] };
      expect(persisted.connections).toHaveLength(1);
    });
    expect(
      container.querySelectorAll("[data-connection-id]"),
    ).toHaveLength(1);
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

function readNodePosition(nodeId: string) {
  const persisted = JSON.parse(
    localStorage.getItem(CANVAS_STORAGE_KEY) ?? "{}",
  ) as {
    nodes?: readonly { readonly id?: string; readonly x?: number; readonly y?: number }[];
  };
  const node = persisted.nodes?.find((candidate) => candidate.id === nodeId);
  return { x: node?.x, y: node?.y };
}

function readTerminalSize(nodeId: string) {
  const persisted = JSON.parse(
    localStorage.getItem(CANVAS_STORAGE_KEY) ?? "{}",
  ) as {
    nodes?: readonly {
      readonly id?: string;
      readonly width?: number;
      readonly height?: number;
    }[];
  };
  const node = persisted.nodes?.find((candidate) => candidate.id === nodeId);
  return { width: node?.width, height: node?.height };
}
