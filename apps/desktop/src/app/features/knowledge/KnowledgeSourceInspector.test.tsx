import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import { IpcError } from "../../../ipc/client";
import type { KnowledgeDiscoverResponse, KnowledgeReadResponse, KnowledgeSourceAvailability, KnowledgeSourceEntry } from "../../../ipc/domain";
import { createMockIpcClient } from "../../../test/mockIpc";
import { KnowledgeSourceInspector } from "./KnowledgeSourceInspector";

const project = { id: "project-a", name: "Project A" };

/** Source metadata is display-only; read requests carry the two opaque IDs. */
function source(overrides: Partial<KnowledgeSourceEntry> = {}): KnowledgeSourceEntry {
  return {
    entryId: "source-a", kind: "rule", provider: "codex", scope: "project",
    sourcePath: "/repo/AGENTS.md", name: "AGENTS.md", scopeDirectory: ".",
    precedenceHint: "Project instructions follow global instructions. Native loading depends on the CLI.",
    viaSymlink: false, availability: "available", ...overrides,
  };
}

/** Builds one bounded metadata inventory with no implicitly read content. */
function inventory(entries: readonly KnowledgeSourceEntry[], overrides: Partial<KnowledgeDiscoverResponse> = {}): KnowledgeDiscoverResponse {
  return { scanId: "scan-a", entries, truncated: false, issues: [], ...overrides };
}

/** Allows old operations to settle after user navigation or a reconnect. */
function deferred<T>() {
  let settle: (value: T) => void = () => { throw new Error("Promise not initialized"); };
  const promise = new Promise<T>((resolve) => { settle = resolve; });
  return { promise, resolve: settle };
}

describe("KnowledgeSourceInspector", () => {
  it("discovers global sources without a project and reads literal content only on selection", async () => {
    const user = userEvent.setup();
    const entry = source({ scope: "global", sourcePath: "/home/example/.codex/AGENTS.md", scopeDirectory: "" });
    const content = "# Instructions\n<script>window.unsafe = true</script>\n![remote](https://example.com/image.png)\n";
    const client = createMockIpcClient({ handlers: {
      discoverKnowledge: async () => inventory([entry]),
      readKnowledge: async () => ({ entry, content }),
    } });
    render(<KnowledgeSourceInspector client={client} />);
    const button = await screen.findByRole("button", { name: entry.name });
    expect(client.discoverKnowledge).toHaveBeenCalledWith({ projectId: null });
    expect(client.readKnowledge).not.toHaveBeenCalled();
    expect(screen.getByText(/Native CLI loading remains unverified/)).toBeVisible();
    await user.click(button);
    expect(client.readKnowledge).toHaveBeenCalledWith({ scanId: "scan-a", entryId: entry.entryId });
    expect(await screen.findByLabelText("Source content")).toHaveTextContent("<script>window.unsafe = true</script>");
    expect(screen.getByLabelText("Source content").textContent).toBe(content);
    expect(screen.getByText(entry.precedenceHint)).toBeVisible();
    expect(screen.queryByRole("img")).not.toBeInTheDocument();
    expect(document.querySelector("script")).not.toBeInTheDocument();
    expect(client.writeTerminal).not.toHaveBeenCalled();
    expect(client.startSession).not.toHaveBeenCalled();
    expect(client.openPath).not.toHaveBeenCalled();
  });

  const unavailable: readonly KnowledgeSourceAvailability[] = ["too_large", "symlink", "non_regular", "unreadable"];
  it.each(unavailable)("keeps %s sources inspectable without requesting their body", async (availability) => {
    const user = userEvent.setup();
    const entry = source({ availability });
    const client = createMockIpcClient({ handlers: { discoverKnowledge: async () => inventory([entry]) } });
    render(<KnowledgeSourceInspector client={client} currentProject={project} />);
    await user.click(await screen.findByRole("button", { name: entry.name }));
    expect(screen.getByText(entry.precedenceHint)).toBeVisible();
    expect(screen.getByText("Source path")).toBeVisible();
    expect(screen.queryByLabelText("Source content")).not.toBeInTheDocument();
    expect(client.readKnowledge).not.toHaveBeenCalled();
    expect(client.discoverKnowledge).toHaveBeenCalledWith({ projectId: project.id });
  });

  it("shows truncation, unsupported scope issues and their locations alongside available results", async () => {
    const client = createMockIpcClient({ handlers: { discoverKnowledge: async () => inventory([source()], {
      truncated: true,
      issues: [{ code: "nested_scope_unsupported", sourcePath: "/repo/nested", message: "Nested project scopes are not included in this inventory." }],
    }) } });
    render(<KnowledgeSourceInspector client={client} />);
    expect(await screen.findByText(/Inventory incomplete/)).toBeVisible();
    expect(screen.getByText("1 discovery issue")).toBeVisible();
    expect(screen.getByText("Nested project scopes are not included in this inventory.")).toBeVisible();
    expect(screen.getByText("/repo/nested")).toBeVisible();
    expect(screen.getByRole("button", { name: "AGENTS.md" })).toBeVisible();
  });

  it("searches metadata across providers and distinguishes a successful empty source", async () => {
    const user = userEvent.setup();
    const first = source();
    const skill = source({ entryId: "source-b", kind: "skill", provider: "claude", scope: "admin", name: "review", sourcePath: "/etc/skills/review/SKILL.md", scopeDirectory: "", viaSymlink: true });
    const client = createMockIpcClient({ handlers: {
      discoverKnowledge: async () => inventory([first, skill]),
      readKnowledge: async () => ({ entry: skill, content: "" }),
    } });
    render(<KnowledgeSourceInspector client={client} />);
    await screen.findByRole("button", { name: first.name });
    await user.type(screen.getByRole("searchbox"), "claude");
    expect(screen.queryByRole("button", { name: first.name })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: skill.name }));
    expect(await screen.findByText("This source is empty.")).toBeVisible();
    expect(screen.getByText("Resolved directory link")).toBeVisible();
    expect(screen.getByText("Administrator")).toBeVisible();
  });

  it("retries failed discovery without fabricating an empty successful inventory", async () => {
    const user = userEvent.setup();
    const client = createMockIpcClient();
    client.discoverKnowledge.mockRejectedValueOnce(new IpcError({ code: "knowledge_discovery_unavailable", message: "Source discovery is unavailable." }));
    client.discoverKnowledge.mockResolvedValue(inventory([source()]));
    render(<KnowledgeSourceInspector client={client} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Source discovery is unavailable.");
    expect(screen.queryByText("No sources found at the supported locations.")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Retry discovery" }));
    expect(await screen.findByRole("button", { name: "AGENTS.md" })).toBeVisible();
  });

  it("ignores older scans across project changes and reconnection on the same client", async () => {
    const pendingProject = deferred<KnowledgeDiscoverResponse>();
    const pendingGlobal = deferred<KnowledgeDiscoverResponse>();
    const fresh = source({ name: "Fresh inventory" });
    const client = createMockIpcClient();
    client.discoverKnowledge.mockReturnValueOnce(pendingProject.promise);
    client.discoverKnowledge.mockReturnValueOnce(pendingGlobal.promise);
    client.discoverKnowledge.mockResolvedValue(inventory([fresh], { scanId: "scan-fresh" }));
    const view = render(<KnowledgeSourceInspector client={client} currentProject={project} connectionKey={1} />);
    await waitFor(() => expect(client.discoverKnowledge).toHaveBeenCalledTimes(1));
    view.rerender(<KnowledgeSourceInspector client={client} connectionKey={1} />);
    await waitFor(() => expect(client.discoverKnowledge).toHaveBeenCalledTimes(2));
    view.rerender(<KnowledgeSourceInspector client={client} connectionKey={2} />);
    expect(await screen.findByRole("button", { name: fresh.name })).toBeVisible();
    await act(async () => {
      pendingProject.resolve(inventory([source({ name: "Old project inventory" })]));
      pendingGlobal.resolve(inventory([source({ name: "Old global inventory" })]));
    });
    expect(screen.queryByRole("button", { name: /Old .* inventory/ })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: fresh.name })).toBeVisible();
  });

  it("ignores a slow preview after the user selects another source", async () => {
    const user = userEvent.setup();
    const first = source();
    const second = source({ entryId: "source-b", name: "Second source", sourcePath: "/repo/second/AGENTS.md" });
    const pendingRead = deferred<KnowledgeReadResponse>();
    const client = createMockIpcClient({ handlers: { discoverKnowledge: async () => inventory([first, second]) } });
    client.readKnowledge.mockReturnValueOnce(pendingRead.promise);
    client.readKnowledge.mockResolvedValue({ entry: second, content: "Current source text" });
    render(<KnowledgeSourceInspector client={client} />);
    await user.click(await screen.findByRole("button", { name: first.name }));
    expect(screen.getByText("Reading source…")).toBeVisible();
    await user.click(screen.getByRole("button", { name: second.name }));
    expect(await screen.findByLabelText("Source content")).toHaveTextContent("Current source text");
    await act(async () => pendingRead.resolve({ entry: first, content: "Outdated source text" }));
    expect(screen.getByLabelText("Source content")).toHaveTextContent("Current source text");
    expect(screen.queryByText("Outdated source text")).not.toBeInTheDocument();
  });

  it("invalidates a pending preview on reconnect and uses a fresh scan capability", async () => {
    const user = userEvent.setup();
    const entry = source();
    const pendingRead = deferred<KnowledgeReadResponse>();
    const client = createMockIpcClient();
    client.discoverKnowledge.mockResolvedValueOnce(inventory([entry]));
    client.discoverKnowledge.mockResolvedValue(inventory([entry], { scanId: "scan-reconnected" }));
    client.readKnowledge.mockReturnValueOnce(pendingRead.promise);
    client.readKnowledge.mockResolvedValue({ entry, content: "Revalidated source" });
    const view = render(<KnowledgeSourceInspector client={client} connectionKey="daemon-1" />);
    await user.click(await screen.findByRole("button", { name: entry.name }));
    view.rerender(<KnowledgeSourceInspector client={client} connectionKey="daemon-2" />);
    await screen.findByRole("button", { name: entry.name });
    await act(async () => pendingRead.resolve({ entry, content: "Old daemon preview" }));
    expect(screen.queryByLabelText("Source content")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: entry.name }));
    expect(await screen.findByLabelText("Source content")).toHaveTextContent("Revalidated source");
    expect(client.readKnowledge).toHaveBeenLastCalledWith({ scanId: "scan-reconnected", entryId: entry.entryId });
  });

  it.each(["knowledge_scan_expired", "knowledge_source_changed"])("clears stale content and offers a rescan after %s", async (code) => {
    const user = userEvent.setup();
    const first = source();
    const second = source({ entryId: "source-b", name: "Changed source" });
    const client = createMockIpcClient({ handlers: { discoverKnowledge: async () => inventory([first, second]) } });
    client.readKnowledge.mockResolvedValueOnce({ entry: first, content: "Previous preview" });
    client.readKnowledge.mockRejectedValueOnce(new IpcError({ code, message: "This source must be discovered again." }));
    render(<KnowledgeSourceInspector client={client} />);
    await user.click(await screen.findByRole("button", { name: first.name }));
    await screen.findByLabelText("Source content");
    await user.click(screen.getByRole("button", { name: second.name }));
    expect(await screen.findByRole("alert")).toHaveTextContent("This source must be discovered again.");
    expect(screen.queryByLabelText("Source content")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Rescan sources" }));
    await waitFor(() => expect(client.discoverKnowledge).toHaveBeenCalledTimes(2));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("rejects a body whose returned provenance differs from the selected descriptor", async () => {
    const user = userEvent.setup();
    const entry = source();
    const client = createMockIpcClient({ handlers: {
      discoverKnowledge: async () => inventory([entry]),
      readKnowledge: async () => ({ entry: { ...entry, sourcePath: "/another/source.md", provider: "cursor" }, content: "Wrong source body" }),
    } });
    render(<KnowledgeSourceInspector client={client} />);
    await user.click(await screen.findByRole("button", { name: entry.name }));
    expect(await screen.findByRole("alert")).toHaveTextContent("invalid response");
    expect(screen.queryByText("Wrong source body")).not.toBeInTheDocument();
    expect(screen.queryByText("/another/source.md")).not.toBeInTheDocument();
  });

  it("provides native keyboard navigation to source selection and the read-only preview", async () => {
    const user = userEvent.setup();
    const entry = source();
    const client = createMockIpcClient({ handlers: {
      discoverKnowledge: async () => inventory([entry]),
      readKnowledge: async () => ({ entry, content: "Keyboard-readable source" }),
    } });
    render(<KnowledgeSourceInspector client={client} />);
    const list = await screen.findByRole("list", { name: "Discovered rules and skills" });
    within(list).getByRole("button", { name: entry.name }).focus();
    await user.keyboard("{Enter}");
    const preview = await screen.findByLabelText("Source content");
    await user.tab();
    expect(preview).toHaveFocus();
  });
});
