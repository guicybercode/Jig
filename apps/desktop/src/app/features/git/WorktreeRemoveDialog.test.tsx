import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { WorktreeRemoveDialog } from "./WorktreeRemoveDialog";

describe("WorktreeRemoveDialog", () => {
  it("blocks dirty removal until the caller confirms allowDirty", async () => {
    const user = userEvent.setup();
    const onConfirm = vi.fn();
    render(
      <WorktreeRemoveDialog
        preview={{
          path: "/tmp/data/worktrees/project/topic",
          branch: "agent/topic",
          dirty: true,
          inUse: false,
        }}
        onCancel={() => undefined}
        onConfirm={onConfirm}
      />,
    );

    expect(screen.getByText("/tmp/data/worktrees/project/topic")).toBeVisible();
    expect(screen.getByText("agent/topic")).toBeVisible();
    expect(
      screen.getByRole("button", { name: "Remove worktree" }),
    ).toBeDisabled();

    await user.click(
      screen.getByRole("checkbox", {
        name: /uncommitted changes/i,
      }),
    );
    await user.click(screen.getByRole("button", { name: "Remove worktree" }));
    expect(onConfirm).toHaveBeenCalledWith(true);
  });

  it("never allows force removal while a session is using the worktree", () => {
    render(
      <WorktreeRemoveDialog
        preview={{
          path: "/tmp/data/worktrees/project/topic",
          branch: "agent/topic",
          dirty: false,
          inUse: true,
        }}
        onCancel={() => undefined}
        onConfirm={() => undefined}
      />,
    );

    expect(
      screen.getByRole("button", { name: "Remove worktree" }),
    ).toBeDisabled();
    expect(screen.getByRole("alert")).toHaveTextContent(
      "A session is still using this worktree",
    );
  });
});
