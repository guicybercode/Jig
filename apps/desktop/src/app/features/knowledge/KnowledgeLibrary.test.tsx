import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { IpcError } from "../../../ipc/client";
import { createMockIpcClient } from "../../../test/mockIpc";
import { KnowledgePanel } from "./KnowledgePanel";
import type { KnowledgePage, KnowledgeRecord } from "./knowledge-types";

const project = { id: "project-a", name: "Project A" };

/** Content fixtures intentionally keep exact whitespace in the reusable body. */
function entry(overrides: Partial<KnowledgeRecord> = {}): KnowledgeRecord {
  return {
    id: "entry-a", kind: "prompt", projectId: null, title: "Review changes",
    body: "Review the diff.\nKeep the public API stable.\n", revision: 3,
    createdAtMs: 100, updatedAtMs: 200, ...overrides,
  };
}

/** Controllable responses exercise real user-visible async races. */
function deferred<T>() {
  let settle: (value: T) => void = () => { throw new Error("Promise not initialized"); };
  const promise = new Promise<T>((resolve) => { settle = resolve; });
  return { promise, resolve: settle };
}

describe("KnowledgePanel", () => {
  it("saves project context and only inserts exact text after an explicit click", async () => {
    const user = userEvent.setup();
    const onInsert = vi.fn();
    const saved = entry({ kind: "context", projectId: project.id, title: "Architecture", body: "Keep this context.\n\n" });
    const client = createMockIpcClient({ handlers: { listKnowledge: async () => ({ entries: [], nextCursor: null }) } });
    client.saveKnowledge.mockResolvedValue(saved);
    render(<KnowledgePanel client={client} currentProject={project} onInsert={onInsert} />);
    await waitFor(() => expect(client.listKnowledge).toHaveBeenCalledWith({ projectId: project.id }));

    await user.selectOptions(screen.getByLabelText("Type"), "context");
    await user.type(screen.getByLabelText("Title"), "  Architecture  ");
    fireEvent.change(screen.getByLabelText("Content"), { target: { value: saved.body } });
    await user.click(screen.getByRole("button", { name: "Save locally" }));
    expect(client.saveKnowledge).toHaveBeenCalledWith({ kind: "context", projectId: project.id, title: "Architecture", body: saved.body });
    expect(await screen.findByText("Saved locally.")).toBeVisible();
    expect(onInsert).not.toHaveBeenCalled();
    expect(client.writeTerminal).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: "Insert into draft" }));
    expect(onInsert).toHaveBeenCalledWith({ sourceId: saved.id, sourceRevision: saved.revision, kind: "context", title: saved.title, body: saved.body });
    expect(screen.getByText("Inserted into the session draft.")).toBeVisible();
    expect(client.writeTerminal).not.toHaveBeenCalled();
  });

  it("inserts unsaved edits with the revision they were based on, even after a newer list response", async () => {
    const user = userEvent.setup();
    const original = entry();
    const onInsert = vi.fn();
    const client = createMockIpcClient();
    client.listKnowledge.mockResolvedValueOnce({ entries: [original], nextCursor: null });
    client.listKnowledge.mockResolvedValue({ entries: [entry({ revision: 4, body: "Changed elsewhere" })], nextCursor: null });
    render(<KnowledgePanel client={client} onInsert={onInsert} />);
    await user.click(await screen.findByRole("button", { name: original.title }));
    fireEvent.change(screen.getByLabelText("Title"), { target: { value: "Edited review" } });
    fireEvent.change(screen.getByLabelText("Content"), { target: { value: "Unsaved content\n" } });
    await user.click(screen.getByRole("button", { name: "Refresh" }));
    await waitFor(() => expect(client.listKnowledge).toHaveBeenCalledTimes(2));
    await user.click(screen.getByRole("button", { name: "Insert into draft" }));
    expect(onInsert).toHaveBeenCalledWith({
      sourceId: original.id, sourceRevision: 3, kind: "prompt",
      title: "Edited review", body: "Unsaved content\n",
    });
    expect(client.saveKnowledge).not.toHaveBeenCalled();
    expect(client.writeTerminal).not.toHaveBeenCalled();
    expect(client.startSession).not.toHaveBeenCalled();
  });

  it("inserts a never-saved draft with neither a source ID nor a source revision", async () => {
    const user = userEvent.setup();
    const onInsert = vi.fn();
    const client = createMockIpcClient({ handlers: { listKnowledge: async () => ({ entries: [], nextCursor: null }) } });
    render(<KnowledgePanel client={client} onInsert={onInsert} />);
    await user.type(screen.getByLabelText("Title"), "New draft");
    await user.type(screen.getByLabelText("Content"), "New text");
    await user.click(screen.getByRole("button", { name: "Insert into draft" }));
    expect(onInsert).toHaveBeenCalledWith({
      sourceId: null, sourceRevision: null, kind: "prompt", title: "New draft", body: "New text",
    });
    expect(client.saveKnowledge).not.toHaveBeenCalled();
    expect(client.writeTerminal).not.toHaveBeenCalled();
    expect(client.startSession).not.toHaveBeenCalled();
  });

  it("preserves item and new-project drafts when selecting items, refreshing, or changing project", async () => {
    const user = userEvent.setup();
    const first = entry();
    const second = entry({ id: "entry-b", title: "Project context", kind: "context", projectId: project.id });
    const client = createMockIpcClient({ handlers: { listKnowledge: async ({ projectId }) => ({ entries: projectId ? [first, second] : [first], nextCursor: null }) } });
    const onInsert = vi.fn();
    const view = render(<KnowledgePanel client={client} currentProject={project} onInsert={onInsert} />);
    await user.click(await screen.findByRole("button", { name: /Review changes/ }));
    fireEvent.change(screen.getByLabelText("Content"), { target: { value: "Unsaved review" } });
    await user.click(screen.getByRole("button", { name: /Project context/ }));
    await user.click(screen.getByRole("button", { name: /Review changes/ }));
    expect(screen.getByLabelText("Content")).toHaveValue("Unsaved review");
    await user.click(screen.getByRole("button", { name: "Refresh" }));
    await waitFor(() => expect(client.listKnowledge).toHaveBeenCalledTimes(2));
    expect(screen.getByLabelText("Content")).toHaveValue("Unsaved review");

    await user.click(screen.getByRole("button", { name: "New item" }));
    await user.type(screen.getByLabelText("Title"), "Project draft");
    await user.selectOptions(screen.getByLabelText("Scope"), "");
    view.rerender(<KnowledgePanel client={client} onInsert={onInsert} />);
    await waitFor(() => expect(client.listKnowledge).toHaveBeenLastCalledWith({ projectId: null }));
    await user.type(screen.getByLabelText("Title"), "Global draft");
    view.rerender(<KnowledgePanel client={client} currentProject={project} onInsert={onInsert} />);
    expect(screen.getByLabelText("Title")).toHaveValue("Project draft");
    expect(screen.getByLabelText("Scope")).toHaveValue("");
    await user.click(await screen.findByRole("button", { name: /Review changes/ }));
    expect(screen.getByLabelText("Content")).toHaveValue("Unsaved review");
    expect(client.saveKnowledge).not.toHaveBeenCalled();
  });

  it("retains conflict edits and saves a copy without overwriting the newer revision", async () => {
    const user = userEvent.setup();
    const original = entry();
    const client = createMockIpcClient({ handlers: { listKnowledge: async () => ({ entries: [original], nextCursor: null }) } });
    client.saveKnowledge.mockRejectedValueOnce(new IpcError({ code: "revision_conflict", message: "Revision changed" }));
    client.saveKnowledge.mockResolvedValueOnce(entry({ id: "entry-copy", body: "My retained draft", revision: 1 }));
    render(<KnowledgePanel client={client} onInsert={vi.fn()} />);
    await user.click(await screen.findByRole("button", { name: /Review changes/ }));
    fireEvent.change(screen.getByLabelText("Content"), { target: { value: "My retained draft" } });
    await user.click(screen.getByRole("button", { name: "Save locally" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Your edits are preserved");
    expect(screen.getByLabelText("Content")).toHaveValue("My retained draft");
    expect(client.saveKnowledge).toHaveBeenNthCalledWith(1, { id: original.id, expectedRevision: 3, kind: "prompt", projectId: null, title: original.title, body: "My retained draft" });
    await user.click(screen.getByRole("button", { name: "Save as copy" }));
    expect(client.saveKnowledge).toHaveBeenNthCalledWith(2, { kind: "prompt", projectId: null, title: original.title, body: "My retained draft" });
    expect(await screen.findByText("Copy saved locally.")).toBeVisible();
  });

  it("requires confirmation before revision-bound deletion and keeps cancelled edits", async () => {
    const user = userEvent.setup();
    const original = entry();
    const client = createMockIpcClient({ handlers: { listKnowledge: async () => ({ entries: [original], nextCursor: null }), deleteKnowledge: async () => undefined } });
    render(<KnowledgePanel client={client} onInsert={vi.fn()} />);
    await user.click(await screen.findByRole("button", { name: /Review changes/ }));
    fireEvent.change(screen.getByLabelText("Content"), { target: { value: "Keep my edits" } });
    await user.click(screen.getByRole("button", { name: "Delete item" }));
    expect(client.deleteKnowledge).not.toHaveBeenCalled();
    const dialog = screen.getByRole("dialog", { name: "Delete saved content" });
    expect(within(dialog).getByRole("button", { name: "Keep item" })).toHaveFocus();
    await user.click(within(dialog).getByRole("button", { name: "Keep item" }));
    expect(screen.getByLabelText("Content")).toHaveValue("Keep my edits");
    await user.click(screen.getByRole("button", { name: "Delete item" }));
    await user.click(screen.getByRole("button", { name: "Confirm delete" }));
    expect(client.deleteKnowledge).toHaveBeenCalledWith({ id: original.id, expectedRevision: original.revision });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.getByLabelText("Title")).toHaveValue("");
  });

  it("loads additional pages and searches all saved title/body content", async () => {
    const user = userEvent.setup();
    const first = entry();
    const second = entry({ id: "entry-b", title: "Deployment notes", kind: "context" });
    const client = createMockIpcClient({ handlers: { listKnowledge: async ({ cursor, query }) => query ? { entries: [second], nextCursor: null } : cursor ? { entries: [second], nextCursor: null } : { entries: [first], nextCursor: first.id } } });
    render(<KnowledgePanel client={client} currentProject={project} onInsert={vi.fn()} />);
    await user.click(await screen.findByRole("button", { name: "Load more" }));
    expect(client.listKnowledge).toHaveBeenLastCalledWith({ projectId: project.id, cursor: first.id });
    expect(await screen.findByRole("button", { name: /Deployment notes/ })).toBeVisible();
    expect(screen.getByRole("button", { name: /Review changes/ })).toBeVisible();
    await user.type(screen.getByRole("searchbox"), "deployment");
    await waitFor(() => expect(client.listKnowledge).toHaveBeenLastCalledWith({ projectId: project.id, query: "deployment" }));
    expect(await screen.findByRole("button", { name: /Deployment notes/ })).toBeVisible();
    expect(screen.queryByRole("button", { name: /Review changes/ })).not.toBeInTheDocument();
  });

  it("keeps the confirmed delete target when the host switches projects", async () => {
    const user = userEvent.setup();
    const original = entry({ projectId: project.id });
    const client = createMockIpcClient({ handlers: {
      listKnowledge: async ({ projectId }) => ({ entries: projectId ? [original] : [], nextCursor: null }),
      deleteKnowledge: async () => undefined,
    } });
    const onInsert = vi.fn();
    const view = render(<KnowledgePanel client={client} currentProject={project} onInsert={onInsert} />);
    await user.click(await screen.findByRole("button", { name: "Review changes" }));
    await user.click(screen.getByRole("button", { name: "Delete item" }));
    view.rerender(<KnowledgePanel client={client} onInsert={onInsert} />);
    await user.click(screen.getByRole("button", { name: "Confirm delete" }));
    expect(client.deleteKnowledge).toHaveBeenCalledWith({ id: original.id, expectedRevision: original.revision });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("does not replace a successful save with an older refresh response", async () => {
    const user = userEvent.setup();
    const oldResponse = deferred<KnowledgePage>();
    const original = entry();
    const updated = entry({ title: "Updated title", revision: 4 });
    const client = createMockIpcClient();
    client.listKnowledge.mockResolvedValueOnce({ entries: [original], nextCursor: null });
    client.listKnowledge.mockReturnValueOnce(oldResponse.promise);
    client.listKnowledge.mockResolvedValue({ entries: [updated], nextCursor: null });
    client.saveKnowledge.mockResolvedValue(updated);
    render(<KnowledgePanel client={client} onInsert={vi.fn()} />);
    await user.click(await screen.findByRole("button", { name: "Review changes" }));
    fireEvent.change(screen.getByLabelText("Title"), { target: { value: "Updated title" } });
    await user.click(screen.getByRole("button", { name: "Refresh" }));
    await waitFor(() => expect(client.listKnowledge).toHaveBeenCalledTimes(2));
    await user.click(screen.getByRole("button", { name: "Save locally" }));
    expect(await screen.findByRole("button", { name: "Updated title" })).toBeVisible();
    await act(async () => oldResponse.resolve({ entries: [original], nextCursor: null }));
    expect(screen.queryByRole("button", { name: "Review changes" })).not.toBeInTheDocument();
    expect(screen.getByLabelText("Title")).toHaveValue("Updated title");
  });

  it("shows deletion failures inside the confirmation and retains the edited content", async () => {
    const user = userEvent.setup();
    const original = entry();
    const client = createMockIpcClient({ handlers: { listKnowledge: async () => ({ entries: [original], nextCursor: null }) } });
    client.deleteKnowledge.mockRejectedValue(new IpcError({ code: "revision_conflict", message: "This item has a newer revision." }));
    render(<KnowledgePanel client={client} onInsert={vi.fn()} />);
    await user.click(await screen.findByRole("button", { name: "Review changes" }));
    fireEvent.change(screen.getByLabelText("Content"), { target: { value: "My draft survives" } });
    await user.click(screen.getByRole("button", { name: "Delete item" }));
    await user.click(screen.getByRole("button", { name: "Confirm delete" }));
    const dialog = screen.getByRole("dialog", { name: "Delete saved content" });
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("This item has a newer revision.");
    await user.click(within(dialog).getByRole("button", { name: "Keep item" }));
    expect(screen.getByLabelText("Content")).toHaveValue("My draft survives");
  });

  it("ignores late search responses without replacing a newer result or editor draft", async () => {
    const user = userEvent.setup();
    const oldResponse = deferred<KnowledgePage>();
    const current = entry({ id: "entry-current", title: "Current result" });
    const client = createMockIpcClient({ handlers: { listKnowledge: async ({ query }) => query === "old" ? oldResponse.promise : { entries: query ? [current] : [], nextCursor: null } } });
    render(<KnowledgePanel client={client} onInsert={vi.fn()} />);
    await user.type(screen.getByLabelText("Title"), "Unfinished note");
    await user.type(screen.getByRole("searchbox"), "old");
    await waitFor(() => expect(client.listKnowledge).toHaveBeenCalledWith({ projectId: null, query: "old" }));
    await user.clear(screen.getByRole("searchbox"));
    await user.type(screen.getByRole("searchbox"), "new");
    expect(await screen.findByRole("button", { name: /Current result/ })).toBeVisible();
    await act(async () => oldResponse.resolve({ entries: [entry({ title: "Outdated result" })], nextCursor: null }));
    expect(screen.queryByRole("button", { name: /Outdated result/ })).not.toBeInTheDocument();
    expect(screen.getByLabelText("Title")).toHaveValue("Unfinished note");
  });

  it("validates UTF-8 limits, focuses invalid fields, and matches Rust whitespace rules", async () => {
    const user = userEvent.setup();
    const client = createMockIpcClient({ handlers: { listKnowledge: async () => ({ entries: [], nextCursor: null }) } });
    client.saveKnowledge.mockResolvedValue(entry({ title: "\uFEFF", body: "\uFEFF" }));
    render(<KnowledgePanel client={client} onInsert={vi.fn()} />);
    await user.click(screen.getByRole("button", { name: "Save locally" }));
    expect(screen.getByLabelText("Title")).toHaveFocus();
    expect(screen.getByText("Enter a title.")).toBeVisible();
    fireEvent.change(screen.getByLabelText("Title"), { target: { value: "é".repeat(129) } });
    fireEvent.change(screen.getByLabelText("Content"), { target: { value: "Valid body" } });
    await user.click(screen.getByRole("button", { name: "Save locally" }));
    expect(screen.getByText("Use a title of at most 256 UTF-8 bytes.")).toBeVisible();
    fireEvent.change(screen.getByLabelText("Title"), { target: { value: "Valid title" } });
    fireEvent.change(screen.getByLabelText("Content"), { target: { value: "é".repeat(32_769) } });
    await user.click(screen.getByRole("button", { name: "Save locally" }));
    expect(screen.getByLabelText("Content")).toHaveFocus();
    expect(screen.getByText(/Content must be at most 64 KiB/)).toBeVisible();
    expect(client.saveKnowledge).not.toHaveBeenCalled();
    fireEvent.change(screen.getByLabelText("Title"), { target: { value: "\uFEFF" } });
    fireEvent.change(screen.getByLabelText("Content"), { target: { value: "\uFEFF" } });
    await user.click(screen.getByRole("button", { name: "Save locally" }));
    expect(client.saveKnowledge).toHaveBeenCalledWith({ kind: "prompt", projectId: null, title: "\uFEFF", body: "\uFEFF" });
  });

  it("blocks duplicate saves and explains unavailable insertion", async () => {
    const user = userEvent.setup();
    const pending = deferred<KnowledgeRecord>();
    const client = createMockIpcClient({ handlers: { listKnowledge: async () => ({ entries: [], nextCursor: null }) } });
    client.saveKnowledge.mockReturnValue(pending.promise);
    render(<KnowledgePanel client={client} onInsert={vi.fn()} insertDisabledReason="Select a session to insert this content." />);
    await user.type(screen.getByLabelText("Title"), "Review changes");
    await user.type(screen.getByLabelText("Content"), "Content");
    expect(screen.getByRole("button", { name: "Insert into draft" })).toBeDisabled();
    expect(screen.getByText("Select a session to insert this content.")).toBeVisible();
    await user.dblClick(screen.getByRole("button", { name: "Save locally" }));
    expect(client.saveKnowledge).toHaveBeenCalledOnce();
    expect(screen.getByRole("button", { name: "Saving changes…" })).toBeDisabled();
    expect(screen.getByLabelText("Content")).toBeDisabled();
    await act(async () => pending.resolve(entry()));
    expect(await screen.findByText("Saved locally.")).toBeVisible();
  });

  it("offers a retry after list failure while retaining a new draft", async () => {
    const user = userEvent.setup();
    const client = createMockIpcClient({ handlers: { listKnowledge: async () => ({ entries: [], nextCursor: null }) } });
    client.listKnowledge.mockRejectedValueOnce(new IpcError({ code: "storage_failed", message: "Could not load saved content." }));
    client.listKnowledge.mockResolvedValue({ entries: [entry()], nextCursor: null });
    render(<KnowledgePanel client={client} onInsert={vi.fn()} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not load saved content.");
    await user.type(screen.getByLabelText("Title"), "Keep this draft");
    await user.click(screen.getByRole("button", { name: "Retry loading" }));
    expect(await screen.findByRole("button", { name: /Review changes/ })).toBeVisible();
    expect(screen.getByLabelText("Title")).toHaveValue("Keep this draft");
  });
});
