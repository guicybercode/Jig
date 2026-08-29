import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { Project } from "../../../ipc/types";
import { CanvasSidebar } from "./CanvasSidebar";

const PROJECT: Project = {
  id: "project-1",
  name: "CLI Master",
  path: "/workspace/cli-master",
  createdAtMs: 1,
  lastOpenedAtMs: 2,
};

describe("CanvasSidebar", () => {
  it("shows a minimal default workspace when no project exists", () => {
    renderSidebar([]);

    expect(screen.getByRole("searchbox", { name: "Filter workspaces" })).toBeVisible();
    expect(screen.getByText("My Workspace")).toBeVisible();
    expect(screen.queryByText("Sessions")).not.toBeInTheDocument();
  });

  it("filters and selects project workspaces", async () => {
    const user = userEvent.setup();
    const onSelectProject = vi.fn();
    renderSidebar([PROJECT], onSelectProject);

    await user.type(
      screen.getByRole("searchbox", { name: "Filter workspaces" }),
      "master",
    );
    await user.click(screen.getByRole("button", { name: /CLI Master/ }));

    expect(onSelectProject).toHaveBeenCalledWith(PROJECT.id);
  });
});

function renderSidebar(
  projects: readonly Project[],
  onSelectProject = vi.fn(),
) {
  return render(
    <CanvasSidebar
      projects={projects}
      sessions={[]}
      canManageProjects
      onSelectProject={onSelectProject}
      onAddProject={vi.fn()}
      onOpenSettings={vi.fn()}
      onOpenDiagnostics={vi.fn()}
    />,
  );
}
