import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { CanvasElementSearch } from "./CanvasElementSearch";
import type { CanvasNode } from "./canvas-state";

const NODES: readonly CanvasNode[] = [
  { id: "note", kind: "note", title: "Release checklist", text: "Validate Linux and macOS packaging", x: 0, y: 0 },
  { id: "terminal", kind: "terminal", title: "Build", preset: "custom", executable: "/opt/bin/checker", workingDirectory: "/workspace/release", x: 100, y: 100, width: 432, height: 256 },
];

describe("CanvasElementSearch", () => {
  it("matches titles, note content, executables and directories with all query terms", async () => {
    const user = userEvent.setup();
    render(<CanvasElementSearch nodes={NODES} agents={[]} sessions={new Map()} onFocusNode={vi.fn()} onClose={vi.fn()} />);
    const input = screen.getByRole("searchbox", { name: "Search canvas items" });
    expect(input).toHaveFocus();
    for (const query of ["release checklist", "LINUX packaging"]) {
      await user.clear(input);
      await user.type(input, query);
      const results = screen.getByRole("list", { name: "Canvas search results" });
      expect(within(results).getAllByRole("button")).toHaveLength(1);
      expect(within(results).getByRole("button", { name: /Release checklist/ })).toBeVisible();
    }
    for (const query of ["checker", "/workspace/release"]) {
      await user.clear(input);
      await user.type(input, query);
      expect(screen.getByRole("button", { name: /Build/ })).toBeVisible();
      expect(screen.queryByRole("button", { name: /Release checklist/ })).not.toBeInTheDocument();
    }
    await user.clear(input);
    await user.type(input, "checker packaging");
    expect(screen.getByText(/No matching items/)).toBeVisible();
  });

  it("supports arrow navigation, returning to search, and focusing the chosen item", async () => {
    const user = userEvent.setup();
    const onFocusNode = vi.fn();
    const onClose = vi.fn();
    render(<CanvasElementSearch nodes={NODES} agents={[]} sessions={new Map()} onFocusNode={onFocusNode} onClose={onClose} />);
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("button", { name: /Release checklist/ })).toHaveFocus();
    await user.keyboard("{ArrowUp}");
    expect(screen.getByRole("searchbox")).toHaveFocus();
    await user.keyboard("{ArrowDown}{ArrowDown}{Enter}");
    expect(onFocusNode).toHaveBeenCalledWith(NODES[1]);
    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("searches the saved agent display name and executable without exposing its environment", async () => {
    const user = userEvent.setup();
    const node = NODES[1];
    if (!node || node.kind !== "terminal") throw new Error("Missing terminal fixture");
    render(<CanvasElementSearch
      nodes={[{ ...node, agentId: "agent" }]}
      agents={[{ id: "agent", source: "custom", displayName: "Security reviewer", enabled: true, command: { executable: "/bin/security-agent", args: [], env: { SECRET: "private-value" } } }]}
      sessions={new Map()}
      onFocusNode={vi.fn()}
      onClose={vi.fn()}
    />);
    const input = screen.getByRole("searchbox");
    await user.type(input, "security reviewer");
    expect(screen.getByRole("button", { name: /Build/ })).toBeVisible();
    await user.clear(input);
    await user.type(input, "security-agent");
    expect(screen.getByRole("button", { name: /Build/ })).toBeVisible();
    await user.clear(input);
    await user.type(input, "private-value");
    expect(screen.queryByRole("button", { name: /Build/ })).not.toBeInTheDocument();
  });
});
