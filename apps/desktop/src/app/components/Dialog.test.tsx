import { useRef, useState } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import { Dialog } from "./Dialog";

function DialogHarness({ startOpen = false }: { readonly startOpen?: boolean }) {
  const [open, setOpen] = useState(startOpen);
  const nameRef = useRef<HTMLInputElement>(null);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>
        Open dialog
      </button>
      <Dialog
        title="New session"
        open={open}
        initialFocusRef={nameRef}
        onClose={() => setOpen(false)}
      >
        <label htmlFor="dialog-name">Session name</label>
        <input ref={nameRef} id="dialog-name" />
        <button type="button">Start session</button>
        <button type="button" onClick={() => setOpen(false)}>
          Cancel
        </button>
      </Dialog>
    </>
  );
}

describe("Dialog", () => {
  it("cycles Tab inside the dialog and restores focus on Escape", async () => {
    const user = userEvent.setup();
    render(<DialogHarness />);

    const opener = screen.getByRole("button", { name: "Open dialog" });
    await user.click(opener);

    const dialog = screen.getByRole("dialog", { name: "New session" });
    expect(dialog).toBeVisible();
    expect(screen.getByLabelText("Session name")).toHaveFocus();

    await user.tab({ shift: true });
    expect(
      screen.getByRole("button", { name: "Close New session" }),
    ).toHaveFocus();
    await user.tab();
    expect(screen.getByLabelText("Session name")).toHaveFocus();

    await user.tab();
    expect(screen.getByRole("button", { name: "Start session" })).toHaveFocus();
    await user.tab();
    expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();
    await user.tab();
    expect(
      screen.getByRole("button", { name: "Close New session" }),
    ).toHaveFocus();
    await user.tab();
    expect(screen.getByLabelText("Session name")).toHaveFocus();

    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(opener).toHaveFocus();
  });
});
