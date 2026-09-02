import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "../App";
import { IpcError } from "../ipc/client";
import type {
  AgentDetection,
  AgentRecord,
  BootstrapResult,
  Project,
  Session,
  StateSnapshot,
  Worktree,
} from "../ipc/types";
import {
  createMockIpcClient,
  EMPTY_BOOTSTRAP,
  type MockIpcClient,
} from "../test/mockIpc";
import { CANVAS_STORAGE_KEY } from "./features/canvas/canvas-state";

vi.mock("./features/terminal/LiveTerminal", () => ({
  LiveTerminal: ({ session }: { readonly session: Session }) => (
    <div
      aria-label={"Live terminal for " + session.name}
      data-terminal-root="true"
      data-testid={"live-terminal-" + session.id}
      tabIndex={0}
    />
  ),
}));

const TEST_TIME = 1_725_000_000_000;
const viewportScrollTo = vi.fn();

describe("AppShell canvas workflows", () => {
  beforeEach(() => {
    localStorage.removeItem(CANVAS_STORAGE_KEY);
    localStorage.removeItem("cli-master.canvas.sidebar-collapsed");
    viewportScrollTo.mockReset();
    Object.defineProperty(HTMLElement.prototype, "scrollTo", {
      configurable: true,
      writable: true,
      value: viewportScrollTo,
    });
  });

  it("starts a terminal inside the canvas without changing workspace", async () => {
    const project = createProject();
    const shellAgent: AgentRecord = {
      ...createAgent(),
      id: "agent-shell",
      displayName: "Shell",
      command: { executable: "/bin/zsh", args: ["-l"], env: {} },
    };
    const createdSession = createSession({
      id: "canvas-terminal-session",
      name: "Terminal 1",
      agentId: shellAgent.id,
      status: "unknown",
    });
    const client = createMockIpcClient({
      bootstrap: createBootstrap({
        projects: [project],
        agents: [shellAgent],
      }),
      handlers: {
        createSession: async () => createdSession,
        startSession: async () => ({
          ...createdSession,
          status: "running",
          pid: 123,
        }),
      },
    });
    const user = userEvent.setup();
    render(<App client={client} />);
    const terminal = await sessionNode("Terminal 1");

    await user.click(
      within(terminal).getByRole("button", { name: "Start terminal" }),
    );

    expect(
      await screen.findByTestId("live-terminal-canvas-terminal-session"),
    ).toBeVisible();
    expect(screen.getByRole("main")).toHaveClass("canvas-workspace");
    expect(
      screen.getByRole("heading", { name: project.name, level: 1 }),
    ).toBeVisible();
    expect(client.createSession).toHaveBeenCalledOnce();
    expect(client.startSession).toHaveBeenCalledWith({
      sessionId: createdSession.id,
    });
  });

  it("keeps persistent canvas chrome around settings", async () => {
    const client = createMockIpcClient({ bootstrap: EMPTY_BOOTSTRAP });
    const user = await renderApp(client);

    expectPersistentChrome();
    await user.click(screen.getByRole("button", { name: "Settings" }));

    expect(
      screen.getByRole("heading", { name: "Settings", level: 1 }),
    ).toBeVisible();
    expectPersistentChrome();
    expect(screen.getByRole("button", { name: "Settings" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(document.querySelector(".project-rail")).not.toBeInTheDocument();
    expect(document.querySelector(".session-pane")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Back to canvas" }));
    expect(screen.getByRole("main")).toHaveClass("canvas-workspace");
  });

  it("suppresses the webview reload and inspector context menu", async () => {
    const client = createMockIpcClient({ bootstrap: EMPTY_BOOTSTRAP });
    await renderApp(client);
    const contextMenu = new MouseEvent("contextmenu", {
      bubbles: true,
      cancelable: true,
    });

    document.body.dispatchEvent(contextMenu);

    expect(contextMenu.defaultPrevented).toBe(true);
  });

  it("keeps persistent canvas chrome around diagnostics", async () => {
    const client = createMockIpcClient({
      bootstrap: EMPTY_BOOTSTRAP,
      handlers: {
        getDiagnostics: async () => ({
          daemonVersion: "0.1.0-test",
          protocolVersion: 1,
          schemaVersion: 1,
          daemonInstanceId: "daemon-test",
          dataPath: "/data/cli-master",
          runtimePath: "/run/cli-master",
          logPath: "/data/cli-master/logs",
          effectivePath: ["/usr/bin"],
          recentIssues: [],
        }),
      },
    });
    const user = await renderApp(client);

    await user.click(screen.getByRole("button", { name: "Open diagnostics" }));

    expect(await screen.findByText("/data/cli-master")).toBeVisible();
    expectPersistentChrome();
    expect(
      screen.getByRole("button", { name: "Open diagnostics" }),
    ).toHaveAttribute("aria-current", "page");
    expect(document.querySelector(".project-rail")).not.toBeInTheDocument();
    expect(document.querySelector(".session-pane")).not.toBeInTheDocument();
  });

  it("hides and restores the persistent CanvasSidebar", async () => {
    const client = createMockIpcClient({ bootstrap: EMPTY_BOOTSTRAP });
    const user = userEvent.setup();
    const { container } = render(<App client={client} />);

    await user.click(
      await screen.findByRole("button", { name: "Hide workspace sidebar" }),
    );

    expect(container.querySelector(".app-shell")).toHaveClass(
      "app-shell--canvas-sidebar-collapsed",
    );
    expect(localStorage.getItem("cli-master.canvas.sidebar-collapsed")).toBe(
      "true",
    );
    expect(
      screen.getByRole("complementary", {
        name: "Canvas workspaces",
        hidden: true,
      }),
    ).toBeInTheDocument();

    await user.click(
      screen.getByRole("button", { name: "Show workspace sidebar" }),
    );

    expect(container.querySelector(".app-shell")).not.toHaveClass(
      "app-shell--canvas-sidebar-collapsed",
    );
    expect(localStorage.getItem("cli-master.canvas.sidebar-collapsed")).toBe(
      "false",
    );
    expect(
      screen.getByRole("complementary", { name: "Canvas workspaces" }),
    ).toBeVisible();
    expect(
      screen.getByRole("button", { name: "Hide navigation" }),
    ).toHaveAttribute("aria-expanded", "true");
  });

  it("adds a daemon-validated project and selects it", async () => {
    const addedProject = createProject({
      name: "CLI Master",
      path: "/Users/test/code/cli-master-link",
      repositoryRoot: "/Users/test/code/cli-master",
      currentBranch: "feature/session-ui",
    });
    const client = createMockIpcClient({
      handlers: { addProject: async () => addedProject },
    });
    const user = await renderApp(client);

    await user.click(screen.getByRole("button", { name: "Add Project" }));
    const dialog = await screen.findByRole("dialog", { name: "Add Project" });
    await user.click(
      within(dialog).getByRole("button", { name: "Enter a path manually" }),
    );
    await user.type(
      within(dialog).getByRole("textbox", { name: /Directory/ }),
      "  /Users/test/code/cli-master-link  ",
    );
    await user.type(
      within(dialog).getByRole("textbox", { name: /Display name/ }),
      "  CLI Master  ",
    );
    await user.click(
      within(dialog).getByRole("button", { name: "Add Project" }),
    );

    await waitFor(() => expect(client.addProject).toHaveBeenCalledOnce());
    expect(client.addProject).toHaveBeenCalledWith({
      path: "/Users/test/code/cli-master-link",
      name: "CLI Master",
    });
    expect(
      await screen.findByRole("heading", {
        name: "CLI Master",
        level: 1,
      }),
    ).toBeVisible();
    expect(
      within(screen.getByRole("navigation", { name: "Workspaces" })).getByRole(
        "button",
        { name: /^CLI Master/ },
      ),
    ).toHaveAttribute("aria-current", "page");
    expect(
      screen.queryByRole("dialog", { name: "Add Project" }),
    ).not.toBeInTheDocument();
  });

  it("filters canvas nodes when switching projects", async () => {
    const firstProject = createProject({
      id: "project-one",
      name: "First Repository",
      lastOpenedAtMs: TEST_TIME + 20,
    });
    const secondProject = createProject({
      id: "project-two",
      name: "Second Repository",
      path: "/repos/second",
      repositoryRoot: "/repos/second",
      currentBranch: "feature/two",
      lastOpenedAtMs: TEST_TIME + 10,
    });
    const firstSession = createSession({
      id: "session-one",
      projectId: firstProject.id,
      name: "First project session",
    });
    const secondSession = createSession({
      id: "session-two",
      projectId: secondProject.id,
      name: "Second project session",
      cwd: "/repos/second",
      branch: "feature/two",
    });
    const client = createMockIpcClient({
      bootstrap: createBootstrap({
        projects: [firstProject, secondProject],
        sessions: [firstSession, secondSession],
      }),
    });
    const user = await renderApp(client);

    const firstTerminal = await sessionNode(firstSession.name);
    expect(firstTerminal).toBeVisible();
    await user.click(firstTerminal);
    expect(firstTerminal).toHaveAttribute("data-selected", "true");
    expect(
      screen.queryByRole("article", {
        name: secondSession.name + ", terminal canvas item",
      }),
    ).not.toBeInTheDocument();

    const projects = screen.getByRole("navigation", { name: "Workspaces" });
    await user.click(
      within(projects).getByRole("button", {
        name: /^Second Repository/,
      }),
    );

    expect(await sessionNode(secondSession.name)).toBeVisible();
    await waitFor(() => {
      expect(
        screen.queryByRole("article", {
          name: firstSession.name + ", terminal canvas item",
        }),
      ).not.toBeInTheDocument();
    });
    expect(
      screen.getByRole("heading", {
        name: "Second Repository",
        level: 1,
      }),
    ).toBeVisible();
    expect(
      within(projects).getByRole("button", {
        name: /^Second Repository/,
      }),
    ).toHaveAttribute("aria-current", "page");

    await user.click(
      within(projects).getByRole("button", {
        name: /^First Repository/,
      }),
    );
    const returnedFirstTerminal = await sessionNode(firstSession.name);
    expect(returnedFirstTerminal).not.toHaveAttribute("data-selected");
    const palette = await openCommandPalette(user);
    expect(
      within(palette).getByRole("option", { name: /Rename Session/ }),
    ).toHaveAttribute("aria-disabled", "true");
  });

  it("opens New Session and blocks invalid required and relative paths", async () => {
    const project = createProject();
    const client = createMockIpcClient({
      bootstrap: createBootstrap({
        projects: [project],
        agents: [createAgent()],
      }),
    });
    const user = await renderApp(client);

    await openNewSession(user);
    const dialog = await screen.findByRole("dialog", { name: "New Session" });
    await user.click(
      within(dialog).getByRole("button", { name: "Review Session" }),
    );

    expect(within(dialog).getByRole("alert")).toHaveTextContent(
      "Enter a session name.",
    );
    expect(
      within(dialog).getByRole("textbox", { name: /Session name/ }),
    ).toHaveFocus();

    await user.type(
      within(dialog).getByRole("textbox", { name: /Session name/ }),
      "Test repository flow",
    );
    const directory = within(dialog).getByRole("textbox", {
      name: /Subdirectory/,
    });
    await user.type(directory, "../outside");
    await user.click(
      within(dialog).getByRole("button", { name: "Review Session" }),
    );

    expect(within(dialog).getByRole("alert")).toHaveTextContent(
      "Use only non-empty child directories; . and .. are not allowed.",
    );
    expect(directory).toHaveFocus();
    expect(client.createSession).not.toHaveBeenCalled();
  });

  it("shows detected executable failures and prevents selecting that agent", async () => {
    const project = createProject();
    const agent = createAgent();
    const client = createMockIpcClient({
      bootstrap: createBootstrap(
        { projects: [project], agents: [agent] },
        [
          {
            agentId: agent.id,
            available: false,
            errorCode: "executable_not_found",
          },
        ],
      ),
    });
    const user = await renderApp(client);

    await openNewSession(user);
    const dialog = await screen.findByRole("dialog", { name: "New Session" });

    expect(within(dialog).getByText(/Executable not found: Codex/)).toBeVisible();
    expect(
      within(dialog).getByRole("option", { name: "Codex — unavailable" }),
    ).toBeDisabled();
    expect(
      within(dialog).getByRole("combobox", { name: /Agent/ }),
    ).toHaveValue("__custom_agent__");
  });

  it("creates one canvas node for a synchronously resolved session", async () => {
    const project = createProject();
    const createdSession = createSession({ name: "Fast session" });
    const client = createMockIpcClient({
      bootstrap: createBootstrap({
        projects: [project],
        agents: [createAgent()],
      }),
      handlers: {
        createSession: () => Promise.resolve(createdSession),
      },
    });
    const user = await renderApp(client);
    const createButton = await reachSessionReview(user, "Fast session");

    await act(async () => {
      createButton.click();
      createButton.click();
      await Promise.resolve();
    });

    expect(client.createSession).toHaveBeenCalledOnce();
    expect(client.createSession).toHaveBeenCalledWith({
      projectId: project.id,
      name: "Fast session",
      agentId: "agent-codex",
      isolation: "current",
      relativeDirectory: undefined,
    });
    const node = await sessionNode("Fast session");
    expect(node).toHaveAttribute("data-canvas-session-id", createdSession.id);
  });

  it("keeps deferred session creation locked and adds its node once", async () => {
    const project = createProject();
    const createdSession = createSession({ name: "Deferred session" });
    const creation = deferred<Session>();
    const client = createMockIpcClient({
      bootstrap: createBootstrap({
        projects: [project],
        agents: [createAgent()],
      }),
      handlers: { createSession: () => creation.promise },
    });
    const user = await renderApp(client);
    const createButton = await reachSessionReview(user, "Deferred session");

    act(() => {
      createButton.click();
      createButton.click();
    });

    expect(client.createSession).toHaveBeenCalledOnce();
    expect(
      await screen.findByRole("button", { name: "Creating…" }),
    ).toBeDisabled();

    await act(async () => {
      creation.resolve(createdSession);
      await creation.promise;
    });

    expect(await sessionNode("Deferred session")).toHaveAttribute(
      "data-canvas-session-id",
      createdSession.id,
    );
    expect(
      screen.getAllByRole("article", {
        name: "Deferred session, terminal canvas item",
      }),
    ).toHaveLength(1);
  });

  it("renders every daemon session status on its canvas node", async () => {
    const project = createProject();
    const statuses = [
      "starting",
      "running",
      "idle",
      "exited",
      "failed",
      "unknown",
    ] as const;
    const sessions = statuses.map((status, index) =>
      createSession({
        id: "session-" + status,
        name: capitalize(status) + " session",
        status,
        updatedAtMs: TEST_TIME + index,
        exitCode: status === "exited" ? 17 : undefined,
        errorCode:
          status === "failed" ? "executable_not_found" : undefined,
      }),
    );
    const client = createMockIpcClient({
      bootstrap: createBootstrap({
        projects: [project],
        agents: [createAgent()],
        sessions,
      }),
    });

    await renderApp(client);

    for (const status of statuses) {
      const node = await sessionNode(capitalize(status) + " session");
      expect(
        within(node).getByLabelText(
          "Session status: " + capitalize(status),
        ),
      ).toBeVisible();
    }
  });

  it("lists the canvas command set, excludes retired views, and executes actions", async () => {
    const project = createProject();
    const session = createSession({ status: "exited" });
    const worktree = createWorktree();
    const startedSession = { ...session, status: "running" as const };
    const client = createMockIpcClient({
      bootstrap: createBootstrap({
        projects: [project],
        agents: [createAgent()],
        sessions: [session],
        worktrees: [worktree],
      }),
      handlers: {
        startSession: async () => startedSession,
      },
    });
    const user = await renderApp(client);
    await user.click(await sessionNode(session.name));

    const dialog = await openCommandPalette(user);
    const expectedCommands = [
      "Add Project",
      "Switch Project",
      "New Session",
      "Rename Project",
      "Remove Project",
      "Start Session",
      "Stop Session",
      "Restart Session",
      "Rename Session",
      "Show Git Status",
      "Open Session Path",
      "Delete Session",
      "Remove Session Worktree",
      "Open Canvas",
      "Open Settings",
      "Open Diagnostics",
      "Switch Project: Test Repository",
      "Focus Session: Test session",
    ];

    for (const label of expectedCommands) {
      expect(within(dialog).getByText(label, { exact: true })).toBeVisible();
    }
    expect(
      within(dialog).queryByText("Open Session", { exact: true }),
    ).not.toBeInTheDocument();
    expect(
      within(dialog).queryByText("Open Grid", { exact: true }),
    ).not.toBeInTheDocument();

    await selectPaletteCommand(user, dialog, "Start Session");
    await waitFor(() => {
      expect(client.startSession).toHaveBeenCalledWith({
        sessionId: session.id,
      });
    });

    const settingsPalette = await openCommandPalette(user);
    await selectPaletteCommand(user, settingsPalette, "Open Settings");
    expect(
      screen.getByRole("heading", { name: "Settings", level: 1 }),
    ).toBeVisible();
  });

  it("clears stale palette session actions when a non-session node is selected", async () => {
    const project = createProject();
    const session = createSession({ status: "exited" });
    const client = createMockIpcClient({
      bootstrap: createBootstrap({
        projects: [project],
        agents: [createAgent()],
        sessions: [session],
      }),
    });
    const user = await renderApp(client);
    await user.click(await sessionNode(session.name));
    await user.click(
      screen.getByRole("article", { name: "Notes, note canvas item" }),
    );

    const palette = await openCommandPalette(user);
    const restartOption = within(palette).getByRole("option", {
      name: /Restart Session/,
    });
    expect(restartOption).toHaveAttribute("aria-disabled", "true");
    await user.click(restartOption);
    expect(client.restartSession).not.toHaveBeenCalled();
  });

  it("uses Linux shortcuts without reviving grid view and focuses entries 1 through 9", async () => {
    const project = createProject();
    const sessions = Array.from({ length: 9 }, (_, index) =>
      createSession({
        id: "linux-session-" + (index + 1),
        name: "Linux session " + (index + 1),
        status: "exited",
        updatedAtMs: TEST_TIME + index,
      }),
    );
    const client = createMockIpcClient({
      platform: "linux",
      bootstrap: createBootstrap({
        projects: [project],
        agents: [createAgent()],
        sessions,
      }),
    });
    const user = await renderApp(client);

    await user.keyboard("{Control>}k{/Control}");
    expect(
      await screen.findByRole("dialog", { name: "Command palette" }),
    ).toBeVisible();
    await user.keyboard("{Escape}");

    await user.keyboard("{Control>}t{/Control}");
    expect(
      await screen.findByRole("dialog", { name: "New Session" }),
    ).toBeVisible();
    await user.keyboard("{Escape}");

    const canvas = screen.getByRole("main");
    await user.keyboard("{Control>}{Shift>}g{/Shift}{/Control}");
    expect(screen.getByRole("main")).toBe(canvas);
    expect(
      screen.queryByRole("heading", { name: "Session Grid" }),
    ).not.toBeInTheDocument();

    for (let shortcut = 1; shortcut <= 9; shortcut += 1) {
      viewportScrollTo.mockClear();
      await user.keyboard(`{Control>}${shortcut}{/Control}`);
      const target = await sessionNode(`Linux session ${10 - shortcut}`);
      await waitFor(() => {
        expect(target).toHaveFocus();
        expect(target).toHaveAttribute("data-selected", "true");
        expect(viewportScrollTo).toHaveBeenCalled();
      });
    }

    const newest = await sessionNode("Linux session 9");
    expect(newest).not.toHaveAttribute("data-selected");
  });

  it("uses macOS Meta shortcuts and numbered canvas focus", async () => {
    const project = createProject();
    const olderSession = createSession({
      id: "mac-older-session",
      name: "Mac older session",
      status: "exited",
      updatedAtMs: TEST_TIME + 1,
    });
    const newestSession = createSession({
      id: "mac-newest-session",
      name: "Mac newest session",
      status: "exited",
      updatedAtMs: TEST_TIME + 2,
    });
    const client = createMockIpcClient({
      platform: "macos",
      bootstrap: createBootstrap({
        projects: [project],
        agents: [createAgent()],
        sessions: [olderSession, newestSession],
      }),
    });
    const user = await renderApp(client);

    await user.keyboard("{Meta>}k{/Meta}");
    expect(
      await screen.findByRole("dialog", { name: "Command palette" }),
    ).toBeVisible();
    await user.keyboard("{Escape}");

    await user.keyboard("{Meta>}t{/Meta}");
    expect(
      await screen.findByRole("dialog", { name: "New Session" }),
    ).toBeVisible();
    await user.keyboard("{Escape}");

    const canvas = screen.getByRole("main");
    await user.keyboard("{Meta>}{Shift>}g{/Shift}{/Meta}");
    expect(screen.getByRole("main")).toBe(canvas);

    viewportScrollTo.mockClear();
    await user.keyboard("{Meta>}1{/Meta}");
    const newest = await sessionNode(newestSession.name);
    await waitFor(() => {
      expect(newest).toHaveFocus();
      expect(newest).toHaveAttribute("data-selected", "true");
      expect(viewportScrollTo).toHaveBeenCalled();
    });
  });

  it("treats compact navigation as an inert focus-trapped drawer", async () => {
    const compactMedia = installCompactMatchMedia();
    const visibleRect = new DOMRect(0, 0, 1, 1);
    const visibleRects = Object.assign([visibleRect], {
      item: (index: number) => (index === 0 ? visibleRect : null),
    });
    const clientRectSpy = vi
      .spyOn(HTMLElement.prototype, "getClientRects")
      .mockReturnValue(visibleRects);
    try {
      const client = createMockIpcClient();
      const user = userEvent.setup();
      render(<App client={client} />);
      await screen.findByRole("heading", {
        name: "My Workspace",
        hidden: true,
      });
      const opener = screen.getByRole("button", { name: "Open navigation" });
      const workspace = screen.getByRole("main", { hidden: true });
      const toolbar = screen.getByRole("toolbar", {
        name: "Workspace actions",
      });
      const statusBar = document.querySelector(".status-bar");
      const skipLink = document.querySelector(".skip-link");

      await user.click(opener);

      await waitFor(() => {
        expect(workspace).toHaveAttribute("inert");
        expect(workspace).toHaveAttribute("aria-hidden", "true");
        expect(toolbar).toHaveAttribute("inert");
        expect(statusBar).toHaveAttribute("inert");
        expect(skipLink).toHaveAttribute("inert");
      });
      const navigation = screen.getByRole("dialog", {
        name: "Workspace navigation",
      });
      expect(navigation).toHaveAttribute("data-open", "true");
      const closeButton = screen.getByRole("button", {
        name: "Close navigation",
      });
      const lastButton = within(navigation).getByRole("button", {
        name: "Open diagnostics",
      });
      expect(closeButton).toHaveFocus();

      closeButton.focus();
      await user.keyboard("{Shift>}{Tab}{/Shift}");
      expect(lastButton).toHaveFocus();

      lastButton.focus();
      await user.keyboard("{Tab}");
      expect(closeButton).toHaveFocus();

      await user.keyboard("{Escape}");
      await waitFor(() => {
        expect(workspace).not.toHaveAttribute("inert");
        expect(workspace).not.toHaveAttribute("aria-hidden");
        expect(toolbar).not.toHaveAttribute("inert");
        expect(statusBar).not.toHaveAttribute("inert");
        expect(skipLink).not.toHaveAttribute("inert");
        expect(opener).toHaveFocus();
      });
      expect(opener).toHaveAccessibleName("Open navigation");

      await user.click(opener);
      const reopenedNavigation = screen.getByRole("dialog", {
        name: "Workspace navigation",
      });
      await user.click(
        within(reopenedNavigation).getByRole("button", {
          name: "Add workspace project",
        }),
      );
      const addProjectDialog = await screen.findByRole("dialog", {
        name: "Add Project",
      });
      await waitFor(() => {
        expect(navigation).toHaveAttribute("data-open", "false");
        expect(workspace).not.toHaveAttribute("inert");
        const focusedElement = document.activeElement;
        expect(focusedElement).toBeInstanceOf(HTMLElement);
        if (!(focusedElement instanceof HTMLElement)) {
          throw new Error("Expected the Add Project dialog to own focus.");
        }
        expect(addProjectDialog).toContainElement(focusedElement);
      });
      await user.click(
        within(addProjectDialog).getByRole("button", {
          name: "Close Add Project",
        }),
      );
      await waitFor(() => expect(opener).toHaveFocus());

      await user.click(opener);
      await waitFor(() => expect(workspace).toHaveAttribute("inert"));
      act(() => compactMedia.exitCompact());
      await waitFor(() => {
        expect(workspace).not.toHaveAttribute("inert");
        expect(navigation).toHaveAttribute("data-open", "false");
        expect(opener).toHaveAccessibleName("Hide navigation");
      });
    } finally {
      clientRectSpy.mockRestore();
      compactMedia.restore();
    }
  });

  it("does not capture application shortcuts inside the terminal root", async () => {
    const project = createProject();
    const firstSession = createSession({
      id: "first-session",
      name: "First session",
      status: "running",
      updatedAtMs: TEST_TIME + 2,
    });
    const terminalOwnedSession = createSession({
      id: "terminal-session",
      name: "Terminal-owned session",
      status: "running",
      updatedAtMs: TEST_TIME + 1,
    });
    const client = createMockIpcClient({
      platform: "linux",
      bootstrap: createBootstrap({
        projects: [project],
        agents: [createAgent()],
        sessions: [firstSession, terminalOwnedSession],
      }),
    });
    const user = await renderApp(client);
    const ownedNode = await sessionNode(terminalOwnedSession.name);
    const terminal = screen.getByTestId(
      "live-terminal-" + terminalOwnedSession.id,
    );

    await user.click(terminal);
    expect(terminal).toHaveFocus();
    expect(ownedNode).toHaveAttribute("data-selected", "true");

    viewportScrollTo.mockClear();
    await user.keyboard("{Control>}k{/Control}");
    await user.keyboard("{Control>}t{/Control}");
    await user.keyboard("{Control>}1{/Control}");
    await user.keyboard("{Control>}{Shift>}g{/Shift}{/Control}");

    expect(
      screen.queryByRole("dialog", { name: "Command palette" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("dialog", { name: "New Session" }),
    ).not.toBeInTheDocument();
    expect(terminal).toHaveFocus();
    expect(ownedNode).toHaveAttribute("data-selected", "true");
    expect(await sessionNode(firstSession.name)).not.toHaveAttribute(
      "data-selected",
    );
    expect(viewportScrollTo).not.toHaveBeenCalled();
  });

  it("shows an actionable IPC error without dismissing the failed form", async () => {
    const client = createMockIpcClient({
      handlers: {
        addProject: async () => {
          throw new IpcError({
            code: "repository_not_found",
            message: "No Git repository was found at that directory.",
            action: "Choose the repository root or initialize Git there first.",
          });
        },
      },
    });
    const user = await renderApp(client);
    await user.click(screen.getByRole("button", { name: "Add Project" }));
    const dialog = await screen.findByRole("dialog", { name: "Add Project" });
    await user.click(
      within(dialog).getByRole("button", { name: "Enter a path manually" }),
    );
    await user.type(
      within(dialog).getByRole("textbox", { name: /Directory/ }),
      "/tmp/not-a-repository",
    );
    await user.click(
      within(dialog).getByRole("button", { name: "Add Project" }),
    );

    const error = await within(dialog).findByRole("alert");
    expect(error).toHaveTextContent(
      "No Git repository was found at that directory.",
    );
    expect(error).toHaveTextContent(
      "Choose the repository root or initialize Git there first.",
    );
    expect(screen.getAllByRole("alert")).toHaveLength(1);
    expect(
      screen.queryByRole("button", { name: "Dismiss operation error" }),
    ).not.toBeInTheDocument();
    expect(dialog).toBeVisible();
  });

  it("opens rename and remove project overlays from CanvasSidebar", async () => {
    const project = createProject();
    const renamedProject = { ...project, name: "Renamed Repository" };
    const client = createMockIpcClient({
      bootstrap: createBootstrap({ projects: [project] }),
      handlers: {
        renameProject: async () => renamedProject,
        removeProject: async () => undefined,
      },
    });
    const user = await renderApp(client);

    await user.click(
      screen.getByRole("button", { name: "Rename Test Repository" }),
    );
    const renameDialog = await screen.findByRole("dialog", {
      name: "Rename Project",
    });
    const nameInput = within(renameDialog).getByRole("textbox", {
      name: "Display name",
    });
    await user.clear(nameInput);
    await user.type(nameInput, "Renamed Repository");
    await user.click(
      within(renameDialog).getByRole("button", { name: "Save Name" }),
    );

    await waitFor(() => {
      expect(client.renameProject).toHaveBeenCalledWith({
        projectId: project.id,
        name: "Renamed Repository",
      });
    });
    expect(
      await screen.findByRole("button", { name: /^Renamed Repository/ }),
    ).toBeVisible();

    await user.click(
      screen.getByRole("button", {
        name: "Remove Renamed Repository from workspaces",
      }),
    );
    const removeDialog = await screen.findByRole("dialog", {
      name: "Remove Project",
    });
    expect(removeDialog).toHaveTextContent("Files stay on disk.");
    await user.click(
      within(removeDialog).getByRole("button", { name: "Remove from App" }),
    );

    await waitFor(() => {
      expect(client.removeProject).toHaveBeenCalledWith(project.id);
      expect(
        screen.queryByRole("button", { name: /^Renamed Repository/ }),
      ).not.toBeInTheDocument();
    });
    expect(
      screen.getByRole("heading", { name: "My Workspace", level: 1 }),
    ).toBeVisible();
  });

  it("starts, restarts, opens, renames, and reads Git status from a canvas node", async () => {
    const project = createProject();
    const session = createSession({
      name: "Automation",
      status: "exited",
      worktreeId: "worktree-one",
      worktreePath: "/repos/project/.worktrees/automation",
    });
    const worktree = createWorktree({
      path: session.worktreePath,
    });
    const client = createMockIpcClient({
      bootstrap: createBootstrap({
        projects: [project],
        agents: [createAgent()],
        sessions: [session],
        worktrees: [worktree],
      }),
      handlers: {
        startSession: async () => ({
          ...session,
          status: "running",
          pid: 101,
        }),
        restartSession: async () => ({
          ...session,
          status: "running",
          pid: 102,
        }),
        renameSession: async () => ({
          ...session,
          name: "Renamed automation",
          status: "running",
        }),
        openPath: async () => undefined,
        getGitStatus: async () => ({
          branch: "agent/automation",
          files: [
            {
              path: "src/app.ts",
              kind: "modified",
              staged: false,
              unstaged: true,
            },
          ],
          counts: { modified: 1, added: 0, deleted: 0, untracked: 0 },
          hasStaged: false,
          hasTrackedChanges: true,
          hasUntracked: false,
          isDirty: true,
        }),
      },
    });
    const user = await renderApp(client);

    let actions = await openSessionActions(user, session.name);
    await user.click(
      within(actions).getByRole("button", { name: "Start session" }),
    );
    await waitFor(() => {
      expect(client.startSession).toHaveBeenCalledWith({
        sessionId: session.id,
      });
    });
    expect(
      within(await sessionNode(session.name)).getByLabelText(
        "Session status: Running",
      ),
    ).toBeVisible();

    actions = await openSessionActions(user, session.name);
    await user.click(
      within(actions).getByRole("button", { name: "Restart session" }),
    );
    await waitFor(() => {
      expect(client.restartSession).toHaveBeenCalledWith({
        sessionId: session.id,
      });
    });

    actions = await openSessionActions(user, session.name);
    await user.click(
      within(actions).getByRole("button", {
        name: "Open working directory",
      }),
    );
    await waitFor(() => {
      expect(client.openPath).toHaveBeenCalledWith(worktree.path);
    });

    actions = await openSessionActions(user, session.name);
    await user.click(
      within(actions).getByRole("button", { name: "Rename session" }),
    );
    const renameDialog = await screen.findByRole("dialog", {
      name: "Rename Session",
    });
    const renameInput = within(renameDialog).getByRole("textbox", {
      name: "Session name",
    });
    await user.clear(renameInput);
    await user.type(renameInput, "Renamed automation");
    await user.click(
      within(renameDialog).getByRole("button", { name: "Save Name" }),
    );
    await waitFor(() => {
      expect(client.renameSession).toHaveBeenCalledWith({
        sessionId: session.id,
        name: "Renamed automation",
      });
    });
    expect(await sessionNode("Renamed automation")).toBeVisible();

    actions = await openSessionActions(user, "Renamed automation");
    await user.click(
      within(actions).getByRole("button", { name: "Git status" }),
    );
    const gitDialog = await screen.findByRole("dialog", { name: "Git Status" });
    expect(await within(gitDialog).findByText("Worktree dirty")).toBeVisible();
    expect(within(gitDialog).getByText("src/app.ts")).toBeVisible();
    expect(client.getGitStatus).toHaveBeenCalledWith({
      kind: "session",
      sessionId: session.id,
    });
    await user.click(within(gitDialog).getByRole("button", { name: "Close" }));
  });

  it("disables unsafe actions when a managed worktree is missing", async () => {
    const session = createSession({
      name: "Missing worktree",
      status: "exited",
      worktreeId: "missing-worktree",
      worktreePath: "/repos/project/.worktrees/missing",
    });
    const client = createMockIpcClient({
      bootstrap: createBootstrap({
        projects: [createProject()],
        agents: [createAgent()],
        sessions: [session],
      }),
    });
    const user = await renderApp(client);

    const actions = await openSessionActions(user, session.name);

    expect(
      within(actions).getByRole("button", { name: "Start session" }),
    ).toHaveAttribute("aria-disabled", "true");
    expect(
      within(actions).getByRole("button", { name: "Restart session" }),
    ).toHaveAttribute("aria-disabled", "true");
    expect(
      within(actions).getByRole("button", { name: "Git status" }),
    ).toHaveAttribute("aria-disabled", "true");
    expect(
      within(actions).getByRole("button", {
        name: "Open working directory",
      }),
    ).toHaveAttribute("aria-disabled", "true");
    expect(
      within(actions).getByRole("button", { name: "Remove worktree" }),
    ).toHaveAttribute("aria-disabled", "true");
    expect(actions).toHaveTextContent(
      "The managed worktree is no longer available.",
    );
    expect(
      within(actions).getByRole("button", {
        name: "Delete session metadata",
      }),
    ).toBeEnabled();
  });

  it("reports a disconnect, disables node actions, and reconnects cleanly", async () => {
    const project = createProject();
    const session = createSession({
      name: "Offline metadata",
      status: "running",
    });
    const client = createMockIpcClient({
      bootstrap: createBootstrap({
        projects: [project],
        agents: [createAgent()],
        sessions: [session],
      }),
    });
    const user = await renderApp(client);

    act(() => {
      client.emit("daemon.shutting_down", {
        reasonCode: "restart_requested",
        activeSessionCount: 0,
      });
    });

    const banner = await screen.findByRole("alert");
    expect(banner).toHaveTextContent("Daemon disconnected.");
    expect(banner).toHaveTextContent("Existing metadata may be stale.");
    const actions = await openSessionActions(user, session.name);
    expect(
      within(actions).getByRole("button", { name: "Restart session" }),
    ).toHaveAttribute("aria-disabled", "true");
    expect(
      within(actions).getByRole("button", { name: "Stop process" }),
    ).toHaveAttribute("aria-disabled", "true");
    await user.click(
      within(banner).getByRole("button", { name: "Reconnect" }),
    );

    await waitFor(() => {
      expect(client.initialize).toHaveBeenCalledTimes(2);
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    });
    expect(client.subscribe).toHaveBeenCalledTimes(2);
    expect(client.listenerCount()).toBe(1);
    expect(await sessionNode(session.name)).toBeVisible();
  });

  it("keeps worktree removal blocked for every daemon safety reason", async () => {
    const project = createProject();
    const session = createSession({
      status: "exited",
      worktreeId: "blocked-worktree",
      worktreePath: "/repos/project/.worktrees/blocked",
    });
    const worktree = createWorktree({
      id: "blocked-worktree",
      sessionId: session.id,
      path: session.worktreePath,
      isDirty: true,
    });
    const client = createMockIpcClient({
      bootstrap: createBootstrap({
        projects: [project],
        agents: [createAgent()],
        sessions: [session],
        worktrees: [worktree],
      }),
      handlers: {
        prepareWorktreeRemoval: async () => ({
          status: "blocked",
          worktreeId: worktree.id,
          isDirty: true,
          blockers: [
            "staged_changes",
            "tracked_changes",
            "untracked_files",
            "ignored_files",
            "assume_unchanged",
            "skip_worktree",
            "locked",
            "running",
            "in_use",
          ],
        }),
      },
    });
    const user = await renderApp(client);

    const actions = await openSessionActions(user, session.name);
    await user.click(
      within(actions).getByRole("button", { name: "Remove worktree" }),
    );
    const dialog = await screen.findByRole("dialog", {
      name: "Remove Worktree",
    });

    await waitFor(() => {
      expect(
        within(dialog).getByRole("button", { name: "Remove Worktree" }),
      ).toBeDisabled();
    });
    expect(dialog).toHaveTextContent("Ignored files are present");
    expect(dialog).toHaveTextContent("assume-unchanged");
    expect(dialog).toHaveTextContent("skip-worktree");
    expect(dialog).toHaveTextContent("Git has locked this worktree");
    expect(dialog).toHaveTextContent("live session is still using");
    expect(dialog).toHaveTextContent("Another session or operation is using");
    expect(client.removeWorktree).not.toHaveBeenCalled();
  });

  it("distinguishes stopping, removing a worktree, and deleting metadata", async () => {
    const project = createProject();
    const runningSession = createSession({
      id: "running-session",
      name: "Running work",
      status: "running",
      updatedAtMs: TEST_TIME + 2,
    });
    const stoppedSession = createSession({
      id: "stopped-session",
      name: "Stopped work",
      status: "exited",
      worktreeId: "worktree-one",
      worktreePath: "/repos/project/.worktrees/stopped",
      updatedAtMs: TEST_TIME + 1,
    });
    const worktree = createWorktree({
      id: "worktree-one",
      sessionId: stoppedSession.id,
      path: stoppedSession.worktreePath,
    });
    const client = createMockIpcClient({
      bootstrap: createBootstrap({
        projects: [project],
        agents: [createAgent()],
        sessions: [runningSession, stoppedSession],
        worktrees: [worktree],
      }),
      handlers: {
        stopSession: async () => ({
          ...runningSession,
          status: "exited",
          updatedAtMs: TEST_TIME + 3,
        }),
        deleteSession: async () => undefined,
        prepareWorktreeRemoval: async () => ({
          status: "ready",
          confirmationToken: "safe-removal-token",
          worktreeId: worktree.id,
          expiresAtMs: TEST_TIME + 60_000,
        }),
        removeWorktree: async () => undefined,
      },
    });
    const user = await renderApp(client);

    let actions = await openSessionActions(user, runningSession.name);
    expect(
      within(actions).getByRole("button", { name: "Stop process" }),
    ).toBeEnabled();
    expect(
      within(actions).getByRole("button", {
        name: "Delete session metadata",
      }),
    ).toHaveAttribute("aria-disabled", "true");
    await user.click(
      within(actions).getByRole("button", { name: "Stop process" }),
    );
    const stopDialog = await screen.findByRole("dialog", {
      name: "Stop Process",
    });
    expect(stopDialog).toHaveTextContent("Only the live process stops.");
    expect(stopDialog).toHaveTextContent(
      "Session metadata and any Git worktree remain available.",
    );
    await user.click(
      within(stopDialog).getByRole("button", { name: "Stop Process" }),
    );
    await waitFor(() => {
      expect(client.stopSession).toHaveBeenCalledWith({
        sessionId: runningSession.id,
      });
    });

    actions = await openSessionActions(user, stoppedSession.name);
    await user.click(
      within(actions).getByRole("button", { name: "Remove worktree" }),
    );
    const worktreeDialog = await screen.findByRole("dialog", {
      name: "Remove Worktree",
    });
    expect(worktreeDialog).toHaveTextContent(
      "This is separate from stopping or deleting a session.",
    );
    expect(worktreeDialog).toHaveTextContent(
      "Removing the worktree does not delete its Git branch.",
    );
    const removeButton = within(worktreeDialog).getByRole("button", {
      name: "Remove Worktree",
    });
    await waitFor(() => expect(removeButton).toBeEnabled());
    await user.click(removeButton);
    await waitFor(() => {
      expect(client.removeWorktree).toHaveBeenCalledWith({
        worktreeId: worktree.id,
        confirmationToken: "safe-removal-token",
      });
    });

    actions = await openSessionActions(user, stoppedSession.name);
    await user.click(
      within(actions).getByRole("button", {
        name: "Delete session metadata",
      }),
    );
    const deleteDialog = await screen.findByRole("dialog", {
      name: "Delete Session",
    });
    expect(deleteDialog).toHaveTextContent(
      "This deletes session metadata only.",
    );
    expect(deleteDialog).toHaveTextContent(
      "It does not stop a running process, delete project files, or remove an associated worktree.",
    );
    await user.click(
      within(deleteDialog).getByRole("button", { name: "Delete Session" }),
    );
    await waitFor(() => {
      expect(client.deleteSession).toHaveBeenCalledWith({
        sessionId: stoppedSession.id,
      });
    });
    expect(
      within(await sessionNode(stoppedSession.name)).queryByRole("button", {
        name: "Session actions for " + stoppedSession.name,
      }),
    ).not.toBeInTheDocument();
  });

  it("removes a retained worktree through the command palette", async () => {
    const project = createProject();
    const session = createSession({
      status: "exited",
      worktreeId: "retained-worktree",
      worktreePath: "/repos/project/.worktrees/retained",
    });
    const worktree = createWorktree({
      id: "retained-worktree",
      sessionId: session.id,
      branch: "agent/retained",
      path: session.worktreePath,
    });
    const client = createMockIpcClient({
      bootstrap: createBootstrap({
        projects: [project],
        agents: [createAgent()],
        sessions: [session],
        worktrees: [worktree],
      }),
      handlers: {
        deleteSession: async () => undefined,
        prepareWorktreeRemoval: async () => ({
          status: "ready",
          worktreeId: worktree.id,
          confirmationToken: "retained-removal-token",
          expiresAtMs: TEST_TIME + 60_000,
        }),
        removeWorktree: async () => undefined,
      },
    });
    const user = await renderApp(client);

    const actions = await openSessionActions(user, session.name);
    await user.click(
      within(actions).getByRole("button", {
        name: "Delete session metadata",
      }),
    );
    await user.click(
      within(
        await screen.findByRole("dialog", { name: "Delete Session" }),
      ).getByRole("button", { name: "Delete Session" }),
    );
    await waitFor(() => expect(client.deleteSession).toHaveBeenCalledOnce());

    const palette = await openCommandPalette(user);
    const retainedLabel = "Remove Retained Worktree: agent/retained";
    expect(
      within(palette).getByText(retainedLabel, { exact: true }),
    ).toBeVisible();
    await selectPaletteCommand(user, palette, retainedLabel);

    const removeDialog = await screen.findByRole("dialog", {
      name: "Remove Worktree",
    });
    const removeButton = within(removeDialog).getByRole("button", {
      name: "Remove Worktree",
    });
    await waitFor(() => expect(removeButton).toBeEnabled());
    await user.click(removeButton);

    await waitFor(() => {
      expect(client.removeWorktree).toHaveBeenCalledWith({
        worktreeId: worktree.id,
        confirmationToken: "retained-removal-token",
      });
    });
    const refreshedPalette = await openCommandPalette(user);
    expect(
      within(refreshedPalette).queryByText(retainedLabel, { exact: true }),
    ).not.toBeInTheDocument();
  });

  it("opens a session-associated worktree from the command palette", async () => {
    const project = createProject();
    const session = createSession({ status: "exited", worktreeId: undefined });
    const worktree = createWorktree({ sessionId: session.id });
    const client = createMockIpcClient({
      bootstrap: createBootstrap({
        projects: [project],
        agents: [createAgent()],
        sessions: [session],
        worktrees: [worktree],
      }),
    });
    const user = await renderApp(client);
    await user.click(await sessionNode(session.name));

    const palette = await openCommandPalette(user);
    await selectPaletteCommand(user, palette, "Remove Session Worktree");

    expect(
      await screen.findByRole("dialog", { name: "Remove Worktree" }),
    ).toBeVisible();
    await waitFor(() => {
      expect(client.prepareWorktreeRemoval).toHaveBeenCalledWith(worktree.id);
    });
  });
});

async function renderApp(client: MockIpcClient) {
  const user = userEvent.setup();
  render(<App client={client} />);
  await screen.findByRole("button", { name: "Settings" });
  expect(screen.queryByText(/^Daemon connected/)).not.toBeInTheDocument();
  return user;
}

function expectPersistentChrome() {
  const shell = document.querySelector(".app-shell");
  expect(shell).toBeInTheDocument();
  expect(shell).not.toHaveClass("app-shell--canvas");
  expect(
    screen.getByRole("complementary", { name: "Canvas workspaces" }),
  ).toBeVisible();
  expect(
    screen.getByRole("toolbar", { name: "Workspace actions" }),
  ).toBeVisible();
  expect(document.querySelector(".status-bar")).toBeVisible();
}

async function sessionNode(name: string) {
  return screen.findByRole("article", {
    name: name + ", terminal canvas item",
  });
}

async function openSessionActions(
  user: ReturnType<typeof userEvent.setup>,
  sessionName: string,
) {
  const node = await sessionNode(sessionName);
  const trigger = within(node).getByRole("button", {
    name: "Session actions for " + sessionName,
  });
  if (trigger.getAttribute("aria-expanded") !== "true") {
    await user.click(trigger);
  }
  return within(node).findByRole("group", {
    name: "Actions for " + sessionName,
  });
}

async function openCommandPalette(
  user: ReturnType<typeof userEvent.setup>,
) {
  await user.click(
    screen.getByRole("button", {
      name: /^Open command palette,/,
    }),
  );
  return screen.findByRole("dialog", { name: "Command palette" });
}

async function selectPaletteCommand(
  user: ReturnType<typeof userEvent.setup>,
  dialog: HTMLElement,
  label: string,
) {
  const search = within(dialog).getByRole("combobox", {
    name: "Search commands",
  });
  await user.clear(search);
  await user.type(search, label);
  await user.click(within(dialog).getByText(label, { exact: true }));
}

async function openNewSession(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("button", { name: "New Session" }));
}

async function reachSessionReview(
  user: ReturnType<typeof userEvent.setup>,
  sessionName: string,
) {
  await openNewSession(user);
  const dialog = await screen.findByRole("dialog", { name: "New Session" });
  await user.type(
    within(dialog).getByRole("textbox", { name: /Session name/ }),
    sessionName,
  );
  await user.click(
    within(dialog).getByRole("button", { name: "Review Session" }),
  );
  return within(dialog).getByRole("button", { name: "Create Session" });
}

function installCompactMatchMedia() {
  const originalMatchMedia = window.matchMedia;
  let breakpointListener: EventListenerOrEventListenerObject | undefined;
  let compactMatches = true;
  const compactQuery = {
    get matches() {
      return compactMatches;
    },
    media: "(max-width: 47.99rem)",
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn((_type, listener) => {
      breakpointListener = listener;
    }),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  } satisfies MediaQueryList;
  const reducedMotionQuery = {
    ...compactQuery,
    matches: false,
    media: "(prefers-reduced-motion: reduce)",
  } satisfies MediaQueryList;
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: vi.fn((query: string) =>
      query === compactQuery.media ? compactQuery : reducedMotionQuery,
    ),
  });
  return {
    exitCompact() {
      compactMatches = false;
      const event = Object.assign(new Event("change"), {
        matches: false,
        media: compactQuery.media,
      });
      if (typeof breakpointListener === "function") {
        breakpointListener(event);
      } else {
        breakpointListener?.handleEvent(event);
      }
    },
    restore() {
      Object.defineProperty(window, "matchMedia", {
        configurable: true,
        value: originalMatchMedia,
      });
    },
  };
}

function createBootstrap(
  snapshot: Partial<StateSnapshot> = {},
  agentDetections?: readonly AgentDetection[],
): BootstrapResult {
  const mergedSnapshot = { ...EMPTY_BOOTSTRAP.snapshot, ...snapshot };
  return {
    hello: EMPTY_BOOTSTRAP.hello,
    snapshot: mergedSnapshot,
    agentDetections:
      agentDetections ??
      mergedSnapshot.agents.map((agent) => ({
        agentId: agent.id,
        available: true,
        executablePath: "/usr/local/bin/" + agent.command.executable,
      })),
  };
}

function createProject(overrides: Partial<Project> = {}): Project {
  return {
    id: "project-one",
    name: "Test Repository",
    path: "/repos/project",
    repositoryRoot: "/repos/project",
    currentBranch: "main",
    availability: "available",
    createdAtMs: TEST_TIME,
    lastOpenedAtMs: TEST_TIME,
    ...overrides,
  };
}

function createAgent(): AgentRecord {
  return {
    id: "agent-codex",
    displayName: "Codex",
    source: "built_in",
    command: {
      executable: "codex",
      args: [],
      env: {},
    },
    enabled: true,
  };
}

function createSession(overrides: Partial<Session> = {}): Session {
  return {
    id: "session-one",
    projectId: "project-one",
    name: "Test session",
    agentId: "agent-codex",
    cwd: "/repos/project",
    branch: "main",
    status: "idle",
    createdAtMs: TEST_TIME,
    updatedAtMs: TEST_TIME,
    lastActivityAtMs: TEST_TIME,
    ...overrides,
  };
}

function createWorktree(overrides: Partial<Worktree> = {}): Worktree {
  return {
    id: "worktree-one",
    projectId: "project-one",
    sessionId: "session-one",
    path: "/repos/project/.worktrees/session-one",
    branch: "codex/session-one",
    isDirty: false,
    state: "active",
    createdAtMs: TEST_TIME,
    updatedAtMs: TEST_TIME,
    ...overrides,
  };
}

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function capitalize(value: string): string {
  return value.charAt(0).toUpperCase() + value.slice(1);
}
