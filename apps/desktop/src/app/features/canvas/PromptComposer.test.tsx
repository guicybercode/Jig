import { useState } from "react";
import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { PromptComposer } from "./PromptComposer";
import type { PromptComposerProps, PromptTerminalKey } from "./PromptComposer";

interface FixtureProps extends Omit<PromptComposerProps, "value" | "onChange"> {
  readonly initialValue?: string;
}

function Fixture({ initialValue = "", ...props }: FixtureProps) {
  const [value, setValue] = useState(initialValue);
  return <PromptComposer {...props} value={value} onChange={setValue} />;
}

function deferred() {
  let complete: () => void = () => {};
  const promise = new Promise<void>((resolve) => { complete = resolve; });
  return { promise, complete };
}

function setup(overrides: Partial<FixtureProps> = {}) {
  const onSend = vi.fn<PromptComposerProps["onSend"]>().mockResolvedValue(undefined);
  const onClose = vi.fn();
  const props = { nodeId: "terminal-a", title: "Codex API", onSend, onClose, ...overrides };
  const view = render(<Fixture {...props} />);
  return { ...view, onSend, onClose, user: userEvent.setup() };
}

describe("PromptComposer", () => {
  it("labels the target, focuses its draft and sends the exact text", async () => {
    const { user, onSend } = setup({ initialValue: "  Explain this function\nthen add tests  " });
    const editor = screen.getByRole("textbox", { name: "Prompt for Codex API" });
    expect(screen.getByRole("region", { name: "Prompt Composer" })).toBeVisible();
    expect(screen.getByText("Send to")).toBeVisible();
    expect(editor).toHaveFocus();

    await user.click(screen.getByRole("button", { name: "Send prompt" }));

    expect(onSend).toHaveBeenCalledExactlyOnceWith("  Explain this function\nthen add tests  ");
    expect(editor).toHaveValue("");
    expect(screen.getByRole("status")).toHaveTextContent("Prompt sent to Codex API.");
  });

  it("sends on Enter and inserts a newline with Shift+Enter", async () => {
    const { user, onSend } = setup();
    const editor = screen.getByRole("textbox");
    await user.type(editor, "First line");
    await user.keyboard("{Shift>}{Enter}{/Shift}");
    expect(editor).toHaveValue("First line\n");
    expect(onSend).not.toHaveBeenCalled();

    await user.type(editor, "Second line{Enter}");

    expect(onSend).toHaveBeenCalledExactlyOnceWith("First line\nSecond line");
    expect(editor).toHaveValue("");
  });

  it("ignores repeated Enter, IME confirmation and modified shortcuts", async () => {
    const { onSend, onClose, user } = setup({ initialValue: "Draft" });
    const editor = screen.getByRole("textbox");
    fireEvent.keyDown(editor, { key: "Enter", repeat: true });
    fireEvent.keyDown(editor, { key: "Enter", isComposing: true });
    fireEvent.keyDown(editor, { key: "Enter", keyCode: 229 });
    fireEvent.compositionStart(editor);
    fireEvent.keyDown(editor, { key: "Enter" });
    fireEvent.keyDown(editor, { key: "Escape" });
    fireEvent.compositionEnd(editor);
    fireEvent.keyDown(editor, { key: "Enter", ctrlKey: true });
    fireEvent.keyDown(editor, { key: "Enter", metaKey: true });
    fireEvent.keyDown(editor, { key: "Enter", altKey: true });
    expect(onSend).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
    expect(editor).toHaveValue("Draft");

    await user.keyboard("{Enter}");
    expect(onSend).toHaveBeenCalledExactlyOnceWith("Draft");
  });

  it.each(["", "  \n\t"])("does not submit an empty or whitespace-only draft %j", async (initialValue) => {
    const { user, onSend } = setup({ initialValue });
    expect(screen.getByRole("button", { name: "Send prompt" })).toBeDisabled();
    await user.keyboard("{Enter}");
    expect(onSend).not.toHaveBeenCalled();
    expect(screen.getByRole("textbox")).toHaveValue(initialValue);
  });

  it("explains an unavailable terminal while allowing draft edits", async () => {
    const onTerminalKey = vi.fn<NonNullable<PromptComposerProps["onTerminalKey"]>>().mockResolvedValue(undefined);
    const { user, onSend } = setup({ disabledReason: "Attach a running session to send input.", onTerminalKey });
    const editor = screen.getByRole("textbox");
    expect(editor).toHaveAccessibleDescription(expect.stringContaining("Attach a running session"));
    await user.keyboard("{Enter}{ArrowUp}");
    await user.type(editor, "Saved for later{Enter}");

    expect(editor).toHaveValue("Saved for later");
    expect(screen.getByRole("button", { name: "Send prompt" })).toBeDisabled();
    expect(onSend).not.toHaveBeenCalled();
    expect(onTerminalKey).not.toHaveBeenCalled();
  });

  it("retains the draft after a rejected send and offers an explicit retry", async () => {
    const onSend = vi.fn<PromptComposerProps["onSend"]>()
      .mockRejectedValueOnce(new Error("Transport failed"))
      .mockResolvedValueOnce(undefined);
    const { user } = setup({ initialValue: "Keep this request", onSend });
    await user.click(screen.getByRole("button", { name: "Send prompt" }));

    expect(screen.getByRole("alert")).toHaveTextContent("Your draft is still here");
    expect(screen.getByRole("textbox")).toHaveValue("Keep this request");
    await user.click(screen.getByRole("button", { name: "Try again" }));

    expect(onSend).toHaveBeenCalledTimes(2);
    expect(onSend).toHaveBeenLastCalledWith("Keep this request");
    expect(screen.getByRole("textbox")).toHaveValue("");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("does not duplicate pending sends or clear a draft edited while sending", async () => {
    const delivery = deferred();
    const onSend = vi.fn<PromptComposerProps["onSend"]>().mockReturnValue(delivery.promise);
    const { user } = setup({ initialValue: "Submitted", onSend });
    const editor = screen.getByRole("textbox");
    await user.keyboard("{Enter}{Enter}");
    expect(screen.getByRole("button", { name: "Sending…" })).toBeDisabled();
    expect(screen.getByRole("status")).toHaveTextContent("Sending prompt…");
    await user.clear(editor);
    await user.type(editor, "Next request");
    await act(async () => delivery.complete());

    expect(onSend).toHaveBeenCalledExactlyOnceWith("Submitted");
    expect(editor).toHaveValue("Next request");
    expect(screen.getByRole("button", { name: "Send prompt" })).toBeEnabled();
  });

  it("preserves a revised draft even when it equals the submitted text again", async () => {
    const delivery = deferred();
    const { user } = setup({ initialValue: "Repeat", onSend: () => delivery.promise });
    const editor = screen.getByRole("textbox");
    await user.keyboard("{Enter}");
    await user.clear(editor);
    await user.type(editor, "Repeat");
    await act(async () => delivery.complete());
    expect(editor).toHaveValue("Repeat");
  });

  it("ignores a previous terminal's pending completion after switching targets", async () => {
    const delivery = deferred();
    const onChange = vi.fn();
    const onClose = vi.fn();
    const { rerender } = render(<PromptComposer
      nodeId="terminal-a" title="Terminal A" value="A draft"
      onChange={onChange} onSend={() => delivery.promise} onClose={onClose}
    />);
    fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter" });
    rerender(<PromptComposer
      nodeId="terminal-b" title="Terminal B" value="B draft"
      onChange={onChange} onSend={vi.fn()} onClose={onClose}
    />);
    await act(async () => delivery.complete());

    expect(screen.getByRole("textbox", { name: "Prompt for Terminal B" })).toHaveValue("B draft");
    expect(screen.getByRole("textbox")).toHaveFocus();
    expect(onChange).not.toHaveBeenCalled();
    expect(screen.getByRole("status")).toBeEmptyDOMElement();
  });

  it("inserts named context as an explicit text snapshot without sending it", async () => {
    const { user, onSend, rerender } = setup({
      initialValue: "Review this plan.",
      contextItems: [{ id: "note-a", title: "Release plan", text: "Keep Linux and macOS support." }],
    });
    await user.click(screen.getByText(/Insert context/));
    expect(screen.getByText("Copies the source text into this draft as a snapshot.")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Insert context from Release plan" }));

    const expected = "Review this plan.\n\nContext snapshot: Release plan\nKeep Linux and macOS support.\n";
    expect(screen.getByRole("textbox")).toHaveValue(expected);
    expect(screen.getByRole("textbox")).toHaveFocus();
    expect(onSend).not.toHaveBeenCalled();
    rerender(<Fixture
      nodeId="terminal-a" title="Codex API" onSend={onSend} onClose={vi.fn()}
      contextItems={[{ id: "note-a", title: "Release plan", text: "Source changed later." }]}
    />);
    expect(screen.getByRole("textbox")).toHaveValue(expected);
  });

  it.each<PromptTerminalKey>(["Enter", "Tab", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"])(
    "forwards %s only when the editor is empty and the keystroke is intentional",
    async (key) => {
      const onTerminalKey = vi.fn<NonNullable<PromptComposerProps["onTerminalKey"]>>().mockResolvedValue(undefined);
      const { onSend, user } = setup({ onTerminalKey });
      const editor = screen.getByRole("textbox");
      fireEvent.keyDown(editor, { key, repeat: true });
      fireEvent.keyDown(editor, { key, isComposing: true });
      fireEvent.keyDown(editor, { key, ctrlKey: true });
      expect(onTerminalKey).not.toHaveBeenCalled();
      await act(async () => { fireEvent.keyDown(editor, { key }); });
      expect(onTerminalKey).toHaveBeenCalledExactlyOnceWith(key);
      expect(onSend).not.toHaveBeenCalled();
      await user.type(editor, "New draft");
      await act(async () => { fireEvent.keyDown(editor, { key }); });
      expect(onTerminalKey).toHaveBeenCalledTimes(1);
    },
  );

  it("allows Shift+Tab to leave the empty editor", async () => {
    const onTerminalKey = vi.fn<NonNullable<PromptComposerProps["onTerminalKey"]>>().mockResolvedValue(undefined);
    const { user } = setup({ onTerminalKey });
    await user.tab({ shift: true });
    expect(screen.getByRole("button", { name: "Close Prompt Composer" })).toHaveFocus();
    expect(onTerminalKey).not.toHaveBeenCalled();
  });

  it("keeps forward tab navigation available when terminal input is disabled", async () => {
    const onTerminalKey = vi.fn<NonNullable<PromptComposerProps["onTerminalKey"]>>().mockResolvedValue(undefined);
    const { user } = setup({
      onTerminalKey,
      disabledReason: "Attach a running session to send input.",
      contextItems: [{ id: "note-a", title: "Plan", text: "Useful context" }],
    });
    await user.tab();
    expect(screen.getByText(/Insert context/)).toHaveFocus();
    expect(onTerminalKey).not.toHaveBeenCalled();
  });

  it("offers a retry for a key that could not be delivered", async () => {
    const onTerminalKey = vi.fn<NonNullable<PromptComposerProps["onTerminalKey"]>>()
      .mockRejectedValueOnce(new Error("Disconnected"))
      .mockResolvedValueOnce(undefined);
    const { user, onSend } = setup({ onTerminalKey });
    await user.keyboard("{ArrowUp}");
    expect(screen.getByRole("alert")).toHaveTextContent("Could not deliver the key");
    await user.click(screen.getByRole("button", { name: "Retry key" }));
    expect(onTerminalKey).toHaveBeenCalledTimes(2);
    expect(onTerminalKey).toHaveBeenLastCalledWith("ArrowUp");
    expect(onSend).not.toHaveBeenCalled();
    expect(screen.getByRole("status")).toHaveTextContent("Key sent to Codex API.");
  });

  it("closes with Escape or the close button without discarding the draft", async () => {
    const { user, onClose, onSend } = setup({ initialValue: "Return to this later" });
    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("textbox")).toHaveValue("Return to this later");
    await user.click(screen.getByRole("button", { name: "Close Prompt Composer" }));
    expect(onClose).toHaveBeenCalledTimes(2);
    expect(onSend).not.toHaveBeenCalled();
  });

  it("restores focus to the trigger when closed from inside the composer", async () => {
    const user = userEvent.setup();
    function Host() {
      const [open, setOpen] = useState(false);
      return <>
        <button type="button" onClick={() => setOpen(true)}>Compose</button>
        {open ? <Fixture nodeId="a" title="Codex" onSend={vi.fn()} onClose={() => setOpen(false)} /> : null}
      </>;
    }
    render(<Host />);
    const trigger = screen.getByRole("button", { name: "Compose" });
    await user.click(trigger);
    expect(screen.getByRole("textbox")).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(trigger).toHaveFocus();
  });
});
