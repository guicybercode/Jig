import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import {
  RecordingIpcClient,
  type CreateSessionInput,
} from "../ipc/client";
import type { SessionView } from "../ipc/types";
import { AppShell } from "./AppShell";
import type { WorkspaceModel } from "./workspace/model";

const connected: WorkspaceModel = {
  daemonConnected: true,
  projects: [
    { id: "project-1", name: "core", path: "/tmp/core" },
    { id: "project-2", name: "desktop", path: "/tmp/desktop" },
  ],
  sessions: [
    {
      id: "session-1",
      projectId: "project-1",
      name: "Implement auth",
      status: "running",
      agentName: "Fake Agent",
    },
    {
      id: "session-2",
      projectId: "project-1",
      name: "Stopped notes",
      status: "exited",
      agentName: "Fake Agent",
    },
  ],
  selectedProjectId: "project-1",
  error: null,
  terminals: [{ sessionId: "session-1", name: "Implement auth" }],
};

describe("AppShell", () => {
  it("explains why session and project actions are unavailable", () => {
    render(<AppShell />);

    expect(screen.getByRole("button", { name: "New Session" })).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "New Session" }),
    ).toHaveAccessibleDescription(
      "Connect the local daemon and add a project first.",
    );
    expect(
      screen.getByRole("button", { name: "Add Project" }),
    ).toHaveAccessibleDescription("Connect the local daemon to add a project.");
  });

  it("reports an honest empty local workspace", () => {
    render(<AppShell />);

    expect(
      screen.getByRole("heading", { name: "No project selected", level: 1 }),
    ).toBeVisible();
    expect(
      screen.getByRole("heading", { name: "Add a repository to begin" }),
    ).toBeVisible();
    expect(screen.getByRole("status")).toHaveTextContent("Daemon unavailable");
    expect(
      screen.getByRole("navigation", { name: "Workspace navigation" }),
    ).toBeVisible();
  });

  it("places the workspace skip link first in keyboard order", async () => {
    const user = userEvent.setup();
    render(<AppShell />);

    await user.tab();

    const skipLink = screen.getByRole("link", { name: "Skip to workspace" });
    expect(skipLink).toHaveFocus();
    expect(skipLink).toHaveAttribute("href", "#workspace");
    expect(screen.getByRole("main")).toHaveAttribute("id", "workspace");
  });

  it("lists projects and sessions when the daemon is connected", () => {
    render(<AppShell initial={connected} />);

    expect(screen.getByRole("button", { name: "core" })).toBeVisible();
    expect(screen.getByRole("button", { name: /Implement auth/ })).toBeVisible();
    expect(screen.getByText("Running")).toBeVisible();
    expect(screen.getByRole("status")).toHaveTextContent("Daemon connected");
    expect(screen.getByRole("button", { name: "New Session" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Add Project" })).toBeEnabled();
    expect(
      screen.getByRole("button", { name: "New Session" }),
    ).not.toHaveAccessibleDescription();
    expect(
      screen.getByRole("button", { name: "Add Project" }),
    ).not.toHaveAccessibleDescription();
  });

  it("creates a session through the dialog without a vendor CLI", async () => {
    const user = userEvent.setup();
    const ipc = new RecordingIpcClient();
    render(<AppShell initial={connected} ipc={ipc} />);

    await user.click(screen.getByRole("button", { name: "New Session" }));
    const dialog = screen.getByRole("dialog", { name: "Create session" });
    expect(dialog).toBeVisible();
    await user.clear(screen.getByLabelText("Session name"));
    await user.type(screen.getByLabelText("Session name"), "Fix parser");
    await user.selectOptions(screen.getByLabelText("Agent"), "custom");
    await user.click(screen.getByRole("button", { name: "Create session" }));

    expect(ipc.createdSessions).toEqual([
      {
        projectId: "project-1",
        name: "Fix parser",
        agentId: "custom",
        isolateWorktree: true,
      },
    ]);
    expect(screen.queryByRole("dialog", { name: "Create session" })).toBeNull();
  });

  it("traps focus, closes on Escape, and restores the session trigger", async () => {
    const user = userEvent.setup();
    render(<AppShell initial={connected} />);

    const trigger = screen.getByRole("button", { name: "New Session" });
    await user.click(trigger);

    const nameInput = screen.getByLabelText("Session name");
    const submit = screen.getByRole("button", { name: "Create session" });
    expect(nameInput).toHaveFocus();

    await user.tab({ shift: true });
    expect(submit).toHaveFocus();
    await user.tab();
    expect(nameInput).toHaveFocus();

    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Create session" })).toBeNull();
    expect(trigger).toHaveFocus();
  });

  it("keeps the create dialog busy until the IPC request resolves", async () => {
    const user = userEvent.setup();
    const ipc = new DeferredIpcClient();
    render(<AppShell initial={connected} ipc={ipc} />);

    const trigger = screen.getByRole("button", { name: "New Session" });
    await user.click(trigger);
    await user.click(screen.getByRole("button", { name: "Create session" }));

    const dialog = screen.getByRole("dialog", { name: "Create session" });
    const form = within(dialog).getByRole("form");
    expect(dialog).toBeVisible();
    expect(form).toHaveAttribute("aria-busy", "true");
    expect(screen.getByRole("button", { name: "Creating session…" })).toBeDisabled();
    expect(within(dialog).getByRole("status")).toHaveTextContent(
      "Creating session…",
    );
    await user.tab();
    expect(dialog).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(dialog).toBeVisible();

    await act(async () => {
      ipc.completeCreate();
      await Promise.resolve();
    });

    await waitFor(() =>
      expect(
        screen.queryByRole("dialog", { name: "Create session" }),
      ).toBeNull(),
    );
    expect(trigger).toHaveFocus();
  });

  it("keeps the create dialog open and explains an IPC failure", async () => {
    const user = userEvent.setup();
    const ipc = new RejectingIpcClient();
    render(<AppShell initial={connected} ipc={ipc} />);

    await user.click(screen.getByRole("button", { name: "New Session" }));
    await user.click(screen.getByRole("button", { name: "Create session" }));

    const alert = await screen.findByRole("alert");
    const dialog = screen.getByRole("dialog", { name: "Create session" });
    expect(alert).toHaveTextContent(
      "Could not create the session. Check the local daemon and try again.",
    );
    expect(dialog).toBeVisible();
    expect(within(dialog).getByRole("form")).not.toHaveAttribute("aria-busy");
    expect(screen.getByRole("button", { name: "Create session" })).toBeEnabled();
  });

  it("unsubscribes from a terminal without stopping the session", () => {
    const ipc = new RecordingIpcClient();
    const { unmount } = render(<AppShell initial={connected} ipc={ipc} />);

    expect(ipc.calls).toContain("subscribe:session-1");
    unmount();
    expect(ipc.calls).toContain("unsubscribe:session-1");
    expect(ipc.calls.some((call) => call.startsWith("stop:"))).toBe(false);
  });

  it("lays out subscribed terminals in a grid", () => {
    render(<AppShell initial={connected} />);

    expect(
      screen.getByRole("region", { name: "Session terminals" }),
    ).toBeVisible();
    expect(
      screen.getByRole("article", { name: "Terminal Implement auth" }),
    ).toBeVisible();
  });

  it("filters and runs command palette actions", async () => {
    const user = userEvent.setup();
    render(<AppShell initial={connected} />);

    await user.click(screen.getByRole("button", { name: "Command palette" }));
    const palette = screen.getByRole("dialog", { name: "Command palette" });
    expect(screen.getByLabelText("Search commands")).toHaveFocus();
    await user.type(screen.getByLabelText("Search commands"), "worktree");
    expect(within(palette).queryByRole("listbox")).toBeNull();
    expect(within(palette).getByRole("list", { name: "Commands" })).toBeVisible();
    expect(within(palette).getByRole("status")).toHaveTextContent(
      "1 command available.",
    );
    expect(
      within(palette).getByRole("button", { name: "Remove Worktree" }),
    ).toBeVisible();
    await user.click(
      within(palette).getByRole("button", { name: "Remove Worktree" }),
    );
    expect(
      screen.getByRole("dialog", { name: "Remove worktree" }),
    ).toBeVisible();
  });

  it("navigates command results with arrows and restores focus on Escape", async () => {
    const user = userEvent.setup();
    render(<AppShell initial={connected} />);

    const trigger = screen.getByRole("button", { name: "Command palette" });
    await user.click(trigger);

    const palette = screen.getByRole("dialog", { name: "Command palette" });
    const search = screen.getByLabelText("Search commands");
    expect(search).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    expect(
      within(palette).getByRole("button", { name: "Add Project" }),
    ).toHaveFocus();
    await user.keyboard("{End}");
    expect(
      within(palette).getByRole("button", { name: "Remove Worktree" }),
    ).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    expect(
      within(palette).getByRole("button", { name: "Add Project" }),
    ).toHaveFocus();

    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Command palette" })).toBeNull();
    expect(trigger).toHaveFocus();
  });

  it("renders daemon errors with a suggested action", () => {
    render(
      <AppShell
        initial={{
          ...connected,
          error: {
            code: "AGENT_EXECUTABLE_NOT_FOUND",
            message: "Could not start the fake agent.",
            action: "Build crates/fake-agent and try again.",
          },
        }}
      />,
    );

    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("AGENT_EXECUTABLE_NOT_FOUND");
    expect(alert).toHaveTextContent("Could not start the fake agent.");
    expect(alert).toHaveTextContent("Build crates/fake-agent and try again.");
  });

  it("keeps destructive worktree removal behind an explicit confirmation", async () => {
    const user = userEvent.setup();
    render(<AppShell initial={connected} />);

    await user.click(screen.getByRole("button", { name: "Command palette" }));
    await user.click(screen.getByRole("button", { name: "Remove Worktree" }));
    const confirm = screen.getByRole("dialog", { name: "Remove worktree" });
    const cancel = screen.getByRole("button", { name: "Cancel" });
    const remove = screen.getByRole("button", { name: "Remove worktree" });
    expect(confirm).toHaveTextContent("uncommitted changes");
    expect(confirm).toHaveTextContent("cannot be undone");
    expect(cancel).toHaveFocus();

    await user.tab();
    expect(remove).toHaveFocus();
    await user.tab();
    expect(cancel).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(
      screen.queryByRole("dialog", { name: "Remove worktree" }),
    ).toBeNull();
    expect(screen.getByRole("button", { name: "Command palette" })).toHaveFocus();
  });

  it("disables project creation while the daemon is disconnected", () => {
    render(
      <AppShell
        initial={{
          ...connected,
          daemonConnected: false,
        }}
      />,
    );

    expect(screen.getByRole("button", { name: "Add Project" })).toBeDisabled();
    expect(screen.getByRole("status")).toHaveTextContent("Daemon unavailable");
    expect(screen.getByRole("button", { name: "New Session" })).toBeDisabled();
  });
});

class DeferredIpcClient extends RecordingIpcClient {
  private pendingCreate:
    | {
        readonly input: CreateSessionInput;
        readonly resolve: (session: SessionView) => void;
      }
    | undefined;

  override createSession(input: CreateSessionInput): Promise<SessionView> {
    this.calls.push(`createSession:${input.name}`);
    this.createdSessions.push({ ...input });

    return new Promise((resolve) => {
      this.pendingCreate = { input, resolve };
    });
  }

  /** Resolves the outstanding create request with a starting session. */
  completeCreate(): void {
    const pendingCreate = this.pendingCreate;
    if (!pendingCreate) {
      throw new Error("No create request is pending.");
    }

    pendingCreate.resolve({
      id: "session-created",
      projectId: pendingCreate.input.projectId,
      name: pendingCreate.input.name,
      status: "starting",
      agentName: pendingCreate.input.agentId,
    });
    this.pendingCreate = undefined;
  }
}

class RejectingIpcClient extends RecordingIpcClient {
  override async createSession(input: CreateSessionInput): Promise<SessionView> {
    await super.createSession(input);
    throw new Error("Daemon unavailable");
  }
}
