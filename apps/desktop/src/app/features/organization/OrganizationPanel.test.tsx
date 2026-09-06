import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { IpcError } from "../../../ipc/client";
import type { OrganizationEntry, OrganizationGetResponse, OrganizationTarget } from "../../../ipc/domain";
import { organizationTargetKey } from "../../../ipc/organization-schema";
import type { Session } from "../../../ipc/types";
import { createMockIpcClient } from "../../../test/mockIpc";
import { OrganizationPanel } from "./OrganizationPanel";

const project = { id: "project-a", name: "Project A" };
const session: Pick<Session, "id" | "name" | "status"> = { id: "session-a", name: "Session A", status: "running" };

function defaults(target: OrganizationTarget): OrganizationEntry {
  return { target, pinned: false, archived: false, workflow: target.kind === "project" ? null : "backlog", revision: 0, updatedAtMs: null };
}

/** Mirrors compare-and-save metadata semantics without any process operations. */
function organizationClient() {
  const entries = new Map<string, OrganizationEntry>();
  const client = createMockIpcClient({ handlers: {
    getOrganization: async ({ targets }) => ({ entries: targets.map((target) => entries.get(organizationTargetKey(target)) ?? defaults(target)) }),
    saveOrganization: async (input) => {
      const key = organizationTargetKey(input.target);
      const current = entries.get(key) ?? defaults(input.target);
      if (input.expectedRevision !== current.revision) throw new IpcError({ code: "organization_conflict", message: "Organization changed in another window." });
      const saved = { target: input.target, pinned: input.pinned, archived: input.archived, workflow: input.workflow, revision: current.revision + 1, updatedAtMs: 100 };
      entries.set(key, saved);
      return saved;
    },
  } });
  return { client, entries };
}

function deferred<T>() {
  let settle: (value: T) => void = () => { throw new Error("Promise not initialized"); };
  const promise = new Promise<T>((resolve) => { settle = resolve; });
  return { promise, resolve: settle };
}

describe("OrganizationPanel", () => {
  it("skips empty selection and batches existing project/session defaults without writes", async () => {
    const { client } = organizationClient();
    const view = render(<OrganizationPanel client={client} />);
    expect(screen.getByText("Select a project or session to organize it.")).toBeVisible();
    expect(client.getOrganization).not.toHaveBeenCalled();
    view.rerender(<OrganizationPanel client={client} currentProject={project} currentSession={session} />);
    await waitFor(() => expect(screen.getAllByRole("checkbox", { name: "Pinned" })[0]).toBeEnabled());
    expect(client.getOrganization).toHaveBeenCalledWith({ targets: [{ kind: "project", id: project.id }, { kind: "session", id: session.id }] });
    expect(screen.getByRole("combobox", { name: "Workflow" })).toHaveValue("backlog");
    expect(client.saveOrganization).not.toHaveBeenCalled();
  });

  it("explicitly saves an archived active session and workflow while showing its unchanged process status", async () => {
    const user = userEvent.setup();
    const { client } = organizationClient();
    const onChanged = vi.fn();
    render(<OrganizationPanel client={client} currentSession={session} onChanged={onChanged} />);
    await waitFor(() => expect(screen.getByRole("checkbox", { name: "Archived" })).toBeEnabled());
    await user.click(screen.getByRole("checkbox", { name: "Pinned" }));
    await user.click(screen.getByRole("checkbox", { name: "Archived" }));
    await user.selectOptions(screen.getByRole("combobox", { name: "Workflow" }), "done");
    expect(screen.getByText(/this session continues running/)).toBeVisible();
    expect(screen.getByLabelText("Session status: Running")).toBeVisible();
    expect(client.saveOrganization).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Save organization" }));
    expect(await screen.findByText("Organization saved locally.")).toBeVisible();
    expect(client.saveOrganization).toHaveBeenCalledWith({ target: { kind: "session", id: session.id }, expectedRevision: 0, pinned: true, archived: true, workflow: "done" });
    expect(onChanged).toHaveBeenCalledWith(expect.objectContaining({ revision: 1, workflow: "done", archived: true }));
    expect(screen.getByLabelText("Session status: Running")).toBeVisible();
    expect(client.stopSession).not.toHaveBeenCalled();
    expect(client.startSession).not.toHaveBeenCalled();
    expect(client.writeTerminal).not.toHaveBeenCalled();
  });

  it("saves project flags with null workflow and preserves unsaved choices across project selection", async () => {
    const user = userEvent.setup();
    const { client } = organizationClient();
    const view = render(<OrganizationPanel client={client} currentProject={project} />);
    await waitFor(() => expect(screen.getByRole("checkbox", { name: "Pinned" })).toBeEnabled());
    await user.click(screen.getByRole("checkbox", { name: "Pinned" }));
    view.rerender(<OrganizationPanel client={client} currentProject={{ id: "project-b", name: "Project B" }} />);
    await waitFor(() => expect(screen.getByRole("checkbox", { name: "Pinned" })).toBeEnabled());
    expect(screen.getByRole("checkbox", { name: "Pinned" })).not.toBeChecked();
    view.rerender(<OrganizationPanel client={client} currentProject={project} />);
    await waitFor(() => expect(screen.getByRole("checkbox", { name: "Pinned" })).toBeEnabled());
    expect(screen.getByRole("checkbox", { name: "Pinned" })).toBeChecked();
    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Save organization" }));
    expect(client.saveOrganization).toHaveBeenCalledWith({ target: { kind: "project", id: project.id }, expectedRevision: 0, pinned: true, archived: false, workflow: null });
  });

  it("preserves workflow drafts across sessions and discards only the selected entity's choices", async () => {
    const user = userEvent.setup();
    const { client } = organizationClient();
    const view = render(<OrganizationPanel client={client} currentSession={session} />);
    await waitFor(() => expect(screen.getByRole("combobox")).toBeEnabled());
    await user.selectOptions(screen.getByRole("combobox"), "in_review");
    view.rerender(<OrganizationPanel client={client} currentSession={{ ...session, id: "session-b", name: "Session B" }} />);
    await waitFor(() => expect(screen.getByRole("combobox")).toBeEnabled());
    await user.click(screen.getByRole("checkbox", { name: "Archived" }));
    view.rerender(<OrganizationPanel client={client} currentSession={session} />);
    await waitFor(() => expect(screen.getByRole("combobox")).toBeEnabled());
    expect(screen.getByRole("combobox")).toHaveValue("in_review");
    await user.click(screen.getByRole("button", { name: "Discard changes" }));
    expect(screen.getByRole("combobox")).toHaveValue("backlog");
    expect(client.saveOrganization).not.toHaveBeenCalled();
  });

  it("retains conflicting desired flags and requires explicit revision refresh before retrying", async () => {
    const user = userEvent.setup();
    const { client, entries } = organizationClient();
    const target: OrganizationTarget = { kind: "session", id: session.id };
    render(<OrganizationPanel client={client} currentSession={session} />);
    await waitFor(() => expect(screen.getByRole("checkbox", { name: "Pinned" })).toBeEnabled());
    await user.click(screen.getByRole("checkbox", { name: "Pinned" }));
    await user.selectOptions(screen.getByRole("combobox"), "blocked");
    entries.set(organizationTargetKey(target), { ...defaults(target), archived: true, revision: 1, updatedAtMs: 100 });
    await user.click(screen.getByRole("button", { name: "Save organization" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Your choices are preserved");
    expect(screen.getByRole("checkbox", { name: "Pinned" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Archived" })).not.toBeChecked();
    expect(screen.getByRole("combobox")).toHaveValue("blocked");
    expect(screen.getByRole("button", { name: "Save organization" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Refresh revision" }));
    expect(await screen.findByText(/Latest revision loaded/)).toBeVisible();
    expect(screen.getByText("Pinned: No. Archived: Yes. Workflow: Backlog.")).toBeVisible();
    await waitFor(() => expect(screen.getByRole("button", { name: "Save organization" })).toBeEnabled());
    await user.click(screen.getByRole("button", { name: "Save organization" }));
    expect(client.saveOrganization).toHaveBeenLastCalledWith({ target, expectedRevision: 1, pinned: true, archived: false, workflow: "blocked" });
    expect(await screen.findByText("Organization saved locally.")).toBeVisible();
  });

  it("ignores old batch responses when project selection or connection changes", async () => {
    const first = deferred<OrganizationGetResponse>();
    const second = deferred<OrganizationGetResponse>();
    const { client } = organizationClient();
    client.getOrganization.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const view = render(<OrganizationPanel client={client} currentProject={project} connectionKey={1} />);
    await waitFor(() => expect(client.getOrganization).toHaveBeenCalledTimes(1));
    view.rerender(<OrganizationPanel client={client} currentProject={{ id: "project-b", name: "Project B" }} connectionKey={1} />);
    await waitFor(() => expect(client.getOrganization).toHaveBeenCalledTimes(2));
    view.rerender(<OrganizationPanel client={client} currentProject={{ id: "project-b", name: "Project B" }} connectionKey={2} />);
    await waitFor(() => expect(screen.getByRole("checkbox", { name: "Pinned" })).toBeEnabled());
    await act(async () => {
      first.resolve({ entries: [{ ...defaults({ kind: "project", id: project.id }), pinned: true, revision: 1, updatedAtMs: 100 }] });
      second.resolve({ entries: [{ ...defaults({ kind: "project", id: "project-b" }), pinned: true, revision: 1, updatedAtMs: 100 }] });
    });
    expect(screen.getByRole("checkbox", { name: "Pinned" })).not.toBeChecked();
  });

  it("ignores a stale save response after reconnect and preserves the desired flags for revalidation", async () => {
    const user = userEvent.setup();
    const pendingSave = deferred<OrganizationEntry>();
    const { client } = organizationClient();
    const onChanged = vi.fn();
    client.saveOrganization.mockReturnValueOnce(pendingSave.promise);
    const view = render(<OrganizationPanel client={client} currentProject={project} connectionKey={1} onChanged={onChanged} />);
    await waitFor(() => expect(screen.getByRole("checkbox", { name: "Pinned" })).toBeEnabled());
    await user.click(screen.getByRole("checkbox", { name: "Pinned" }));
    await user.click(screen.getByRole("button", { name: "Save organization" }));
    view.rerender(<OrganizationPanel client={client} currentProject={project} connectionKey={2} onChanged={onChanged} />);
    await waitFor(() => expect(screen.getByRole("checkbox", { name: "Pinned" })).toBeEnabled());
    await act(async () => pendingSave.resolve({ ...defaults({ kind: "project", id: project.id }), pinned: true, revision: 1, updatedAtMs: 100 }));
    expect(screen.getByRole("checkbox", { name: "Pinned" })).toBeChecked();
    expect(screen.queryByText("Organization saved locally.")).not.toBeInTheDocument();
    expect(onChanged).not.toHaveBeenCalled();
  });

  it("rejects a batch for another target and leaves controls unavailable", async () => {
    const { client } = organizationClient();
    client.getOrganization.mockResolvedValue({ entries: [defaults({ kind: "project", id: "another-project" })] });
    render(<OrganizationPanel client={client} currentProject={project} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("invalid response");
    expect(screen.getByRole("checkbox", { name: "Pinned" })).toBeDisabled();
  });

  it("prevents duplicate saves while a request is pending", async () => {
    const user = userEvent.setup();
    const pendingSave = deferred<OrganizationEntry>();
    const { client } = organizationClient();
    client.saveOrganization.mockReturnValueOnce(pendingSave.promise);
    render(<OrganizationPanel client={client} currentProject={project} />);
    const card = screen.getByRole("form", { name: project.name });
    await waitFor(() => expect(within(card).getByRole("checkbox", { name: "Pinned" })).toBeEnabled());
    await user.click(within(card).getByRole("checkbox", { name: "Pinned" }));
    await user.dblClick(within(card).getByRole("button", { name: "Save organization" }));
    expect(client.saveOrganization).toHaveBeenCalledOnce();
    expect(within(card).getByRole("checkbox", { name: "Pinned" })).toBeDisabled();
    await act(async () => pendingSave.resolve({ ...defaults({ kind: "project", id: project.id }), pinned: true, revision: 1, updatedAtMs: 100 }));
  });
});
