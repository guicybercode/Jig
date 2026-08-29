import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { AddProjectDialog } from "./ProjectDialogs";

describe("AddProjectDialog", () => {
  it("selects a project through the visual folder browser", async () => {
    const user = userEvent.setup();
    const onAdd = vi.fn().mockResolvedValue({
      id: "project-1",
      name: "cli-master",
      path: "/Users/test/code/cli-master",
      createdAtMs: 1,
      lastOpenedAtMs: 1,
    });
    const onBrowseDirectory = vi
      .fn()
      .mockResolvedValue("/Users/test/code/cli-master");

    render(
      <AddProjectDialog
        open
        onClose={vi.fn()}
        onAdd={onAdd}
        onBrowseDirectory={onBrowseDirectory}
      />,
    );

    const dialog = screen.getByRole("dialog", { name: "Add Project" });
    await user.click(
      within(dialog).getByRole("button", { name: /Choose a project folder/ }),
    );

    expect(onBrowseDirectory).toHaveBeenCalledOnce();
    expect(within(dialog).getByText("cli-master")).toBeVisible();
    expect(within(dialog).getByText("/Users/test/code")).toBeVisible();

    await user.click(
      within(dialog).getByRole("button", { name: "Add Project" }),
    );

    expect(onAdd).toHaveBeenCalledWith({
      path: "/Users/test/code/cli-master",
      name: undefined,
    });
  });
});
