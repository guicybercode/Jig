import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { Project, Session } from "../../../ipc/types";
import {
  CANVAS_DOCUMENT_UPDATED_EVENT,
  createInitialCanvasDocument,
} from "../canvas/canvas-state";
import { CanvasSidebar } from "./CanvasSidebar";

const PROJECT: Project = {
  id: "project-1",
  name: "CLI Master",
  path: "/workspace/cli-master",
  createdAtMs: 1,
  lastOpenedAtMs: 2,
};

const SESSION: Session = {
  id: "session-1",
  projectId: PROJECT.id,
  name: "Review",
  agentId: "agent-1",
  cwd: PROJECT.path,
  status: "exited",
  createdAtMs: 1,
  updatedAtMs: 2,
};

describe("CanvasSidebar", () => {
  it("shows a minimal default workspace when no project exists", () => {
    renderSidebar([]);

    act(() => {
      window.dispatchEvent(
        new CustomEvent(CANVAS_DOCUMENT_UPDATED_EVENT, {
          detail: createInitialCanvasDocument(),
        }),
      );
    });

    expect(screen.getByRole("searchbox", { name: "Filter workspaces" })).toBeVisible();
    expect(screen.getByText("My Workspace")).toBeVisible();
    expect(screen.getByLabelText("2 canvas terminals")).toHaveTextContent("2");
    expect(screen.queryByText("Sessions")).not.toBeInTheDocument();
  });

  it("keeps the canvas terminal count synchronized", () => {
    renderSidebar([]);

    act(() => {
      window.dispatchEvent(
        new CustomEvent(CANVAS_DOCUMENT_UPDATED_EVENT, {
          detail: { ...createInitialCanvasDocument(), nodes: [] },
        }),
      );
    });

    expect(screen.getByLabelText("0 canvas terminals")).toHaveTextContent("0");
  });

  it("filters and selects project workspaces", async () => {
    const user = userEvent.setup();
    const onSelectProject = vi.fn();
    renderSidebar([PROJECT], onSelectProject);

    await user.type(
      screen.getByRole("searchbox", { name: "Filter workspaces" }),
      "master",
    );
    await user.click(screen.getByRole("button", { name: /^CLI Master/ }));

    expect(onSelectProject).toHaveBeenCalledWith(PROJECT.id);
  });

  it("reports daemon sessions instead of global canvas terminal cards", () => {
    renderSidebar(
      [PROJECT],
      vi.fn(),
      vi.fn(),
      "canvas",
      vi.fn(),
      vi.fn(),
      vi.fn(),
      [SESSION],
    );

    expect(screen.getByLabelText("1 sessions")).toHaveTextContent("1");
  });

  it("offers a control to hide the workspace sidebar", async () => {
    const user = userEvent.setup();
    const onHide = vi.fn();
    renderSidebar([], vi.fn(), onHide);

    await user.click(
      screen.getByRole("button", { name: "Hide workspace sidebar" }),
    );

    expect(onHide).toHaveBeenCalledOnce();
  });

  it("keeps project rename and remove controls visible and connected", async () => {
    const user = userEvent.setup();
    const onRenameProject = vi.fn();
    const onRemoveProject = vi.fn();
    renderSidebar(
      [PROJECT],
      vi.fn(),
      vi.fn(),
      "canvas",
      vi.fn(),
      onRenameProject,
      onRemoveProject,
    );

    const renameButton = screen.getByRole("button", {
      name: `Rename ${PROJECT.name}`,
    });
    const removeButton = screen.getByRole("button", {
      name: `Remove ${PROJECT.name} from workspaces`,
    });
    expect(renameButton).toBeVisible();
    expect(removeButton).toBeVisible();

    await user.click(renameButton);
    await user.click(removeButton);

    expect(onRenameProject).toHaveBeenCalledWith(PROJECT.id);
    expect(onRemoveProject).toHaveBeenCalledWith(PROJECT.id);
  });

  it("returns to the canvas from the settings view", async () => {
    const user = userEvent.setup();
    const onOpenCanvas = vi.fn();
    renderSidebar([], vi.fn(), vi.fn(), "settings", onOpenCanvas);

    expect(screen.getByRole("button", { name: "Settings" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    await user.click(screen.getByRole("button", { name: /My Workspace/ }));

    expect(onOpenCanvas).toHaveBeenCalledOnce();
  });
});

function renderSidebar(
  projects: readonly Project[],
  onSelectProject = vi.fn(),
  onHide = vi.fn(),
  activeView: "canvas" | "settings" | "diagnostics" = "canvas",
  onOpenCanvas = vi.fn(),
  onRenameProject = vi.fn(),
  onRemoveProject = vi.fn(),
  sessions: readonly Session[] = [],
) {
  return render(
    <CanvasSidebar
      projects={projects}
      sessions={sessions}
      activeView={activeView}
      canManageProjects
      onSelectProject={onSelectProject}
      onOpenCanvas={onOpenCanvas}
      onHide={onHide}
      onAddProject={vi.fn()}
      onRenameProject={onRenameProject}
      onRemoveProject={onRemoveProject}
      onOpenSettings={vi.fn()}
      onOpenDiagnostics={vi.fn()}
    />,
  );
}
