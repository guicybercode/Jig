import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { NewCanvasTerminalDialog } from "./NewCanvasTerminalDialog";

describe("NewCanvasTerminalDialog", () => {
  it("creates a Gemini draft using the selected project directory", async () => {
    const user = userEvent.setup();
    const onCreate = vi.fn();
    render(<NewCanvasTerminalDialog
      defaultWorkingDirectory="/projects/demo"
      onClose={vi.fn()}
      onCreate={onCreate}
    />);

    await user.click(screen.getByRole("radio", { name: "Gemini" }));
    expect(screen.getByLabelText("Command")).toHaveValue("gemini");
    await user.click(screen.getByRole("button", { name: "Create terminal" }));

    expect(onCreate).toHaveBeenCalledExactlyOnceWith({
      title: "Gemini",
      preset: "gemini",
      isolation: "current",
      executable: "gemini",
      workingDirectory: "/projects/demo",
    });
  });

  it("passes an explicit isolated working copy choice without launching a process", async () => {
    const user = userEvent.setup();
    const onCreate = vi.fn();
    render(<NewCanvasTerminalDialog defaultWorkingDirectory="/projects/demo/tools" onClose={vi.fn()} onCreate={onCreate} />);
    await user.selectOptions(screen.getByRole("combobox", { name: "Working copy" }), "new_worktree");
    expect(screen.getByText(/Requires a Git repository with a commit/)).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Create terminal" }));
    expect(onCreate).toHaveBeenCalledExactlyOnceWith({
      title: "Shell", preset: "shell", isolation: "new_worktree", executable: undefined,
      workingDirectory: "/projects/demo/tools",
    });
  });
});
