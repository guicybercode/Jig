import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { App } from "../App";
import { IpcError } from "../ipc/client";
import { CANVAS_STORAGE_KEY } from "./features/canvas/canvas-state";
import type {
  AgentRecord,
  AgentDetection,
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

vi.mock("./features/terminal/LiveTerminal", () => ({
  LiveTerminal: ({ session }: { session: Session }) => (
    <div data-testid={`live-terminal-${session.id}`} />
  ),
}));

const TEST_TIME = 1_725_000_000_000;

describe("AppShell project and session workflows", () => {
  it("starts a terminal inside the canvas without opening the session view", async () => {
    localStorage.removeItem(CANVAS_STORAGE_KEY);
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
    const terminal = await screen.findByRole("article", {
      name: "Terminal 1, terminal canvas item",
    });

    await user.click(
      within(terminal).getByRole("button", { name: "Start terminal" }),
    );

    expect(await screen.findByTestId("live-terminal-canvas-terminal-session"))
      .toBeVisible();
    expect(screen.getByRole("main")).toHaveClass("canvas-workspace");
    expect(
      screen.getByRole("heading", { name: project.name, level: 1 }),
    ).toBeVisible();
    expect(client.createSession).toHaveBeenCalledOnce();
    expect(client.startSession).toHaveBeenCalledWith({
      sessionId: createdSession.id,
    });
  });

  it("keeps settings inside the minimal canvas shell", async () => {
    const client = createMockIpcClient({ bootstrap: EMPTY_BOOTSTRAP });
    const user = await renderApp(client);

    await user.click(screen.getByRole("button", { name: "Settings" }));

    const shell = document.querySelector(".app-shell");
    expect(shell).toHaveClass("app-shell--canvas");
    expect(screen.getByRole("heading", { name: "Settings", level: 1 })).toBeVisible();
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

  it("keeps diagnostics inside the minimal canvas shell", async () => {
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

    await user.click(screen.getByRole("button", { name: "Settings" }));
    await user.click(screen.getByRole("button", { name: "Back to canvas" }));
    await user.click(screen.getByRole("button", { name: "Open diagnostics" }));

    expect(document.querySelector(".app-shell")).toHaveClass(
      "app-shell--canvas",
    );
    expect(await screen.findByText("/data/cli-master")).toBeVisible();
    expect(
      screen.getByRole("button", { name: "Open diagnostics" }),
    ).toHaveAttribute("aria-current", "page");
    expect(document.querySelector(".project-rail")).not.toBeInTheDocument();
    expect(document.querySelector(".session-pane")).not.toBeInTheDocument();
  });

  it("hides and restores the canvas workspace sidebar", async () => {
    localStorage.removeItem("cli-master.canvas.sidebar-collapsed");
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

    await user.click(
      screen.getByRole("button", { name: "Show workspace sidebar" }),
    );
    expect(container.querySelector(".app-shell")).not.toHaveClass(
      "app-shell--canvas-sidebar-collapsed",
    );
    expect(localStorage.getItem("cli-master.canvas.sidebar-collapsed")).toBe(
      "false",
    );
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

    await user.click(
      within(screen.getByRole("main")).getByRole("button", {
        name: "Add Project",
      }),
    );
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
      await within(screen.getByRole("main")).findByRole("heading", {
        name: "CLI Master",
        level: 1,
      }),
    ).toBeVisible();
    expect(screen.getByText("/Users/test/code/cli-master")).toBeVisible();
    expect(
      screen.queryByRole("dialog", { name: "Add Project" }),
    ).not.toBeInTheDocument();
  });

  it("switches projects without showing sessions from the previous project", async () => {
    const firstProject = createProject({
      id: "project-one",
      name: "First Repository",
      currentBranch: "main",
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
    const client = createMockIpcClient({
      bootstrap: createBootstrap({
        projects: [firstProject, secondProject],
        sessions: [firstSession],
      }),
    });
    const user = await renderApp(client);

    expect(
      within(screen.getByRole("main")).getByRole("heading", {
        name: "First Repository",
        level: 1,
      }),
    ).toBeVisible();
    expect(screen.getByText("First project session")).toBeVisible();

    const projects = screen.getByRole("navigation", { name: "Recent projects" });
    await user.click(
      within(projects).getByRole("button", {
        name: /^Second Repositoryfeature\/two$/,
      }),
    );

    expect(
      within(screen.getByRole("main")).getByRole("heading", {
        name: "Second Repository",
        level: 1,
      }),
    ).toBeVisible();
    expect(screen.queryByText("First project session")).not.toBeInTheDocument();
    expect(
      within(projects).getByRole("button", {
        name: /^Second Repositoryfeature\/two$/,
      }),
    ).toHaveAttribute("aria-current", "page");
  });

  it("opens New Session and blocks invalid required and relative-path input", async () => {
    const project = createProject();
    const client = createMockIpcClient({
      bootstrap: createBootstrap({ projects: [project], agents: [createAgent()] }),
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
    const directory = within(dialog).getByRole("textbox", { name: /Subdirectory/ });
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
    expect(within(dialog).getByRole("combobox", { name: /Agent/ })).toHaveValue(
      "__custom_agent__",
    );
  });

  it("submits a synchronously resolved session creation only once", async () => {
    const project = createProject();
    const createdSession = createSession({ name: "Fast session" });
    const client = createMockIpcClient({
      bootstrap: createBootstrap({ projects: [project], agents: [createAgent()] }),
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
    expect(
      await within(screen.getByRole("main")).findByRole("heading", {
        name: "Fast session",
        level: 1,
      }),
    ).toBeVisible();
  });

  it("keeps a deferred session creation locked against duplicate submission", async () => {
    const project = createProject();
    const createdSession = createSession({ name: "Deferred session" });
    const creation = deferred<Session>();
    const client = createMockIpcClient({
      bootstrap: createBootstrap({ projects: [project], agents: [createAgent()] }),
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
    expect(
      await within(screen.getByRole("main")).findByRole("heading", {
        name: "Deferred session",
        level: 1,
      }),
    ).toBeVisible();
  });

  it("renders every status plus startup and unsuccessful-exit details", async () => {
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
        id: `session-${status}`,
        name: `${capitalize(status)} session`,
        status,
        updatedAtMs: TEST_TIME + index,
        exitCode: status === "exited" ? 17 : undefined,
        errorCode:
          status === "failed"
            ? "executable_not_found"
            : undefined,
      }),
    );
    const processFailure = createSession({
      id: "session-failed-exit",
      name: "Failed process",
      status: "failed",
      exitCode: 1,
      updatedAtMs: TEST_TIME + statuses.length,
    });
    sessions.push(processFailure);
    const client = createMockIpcClient({
      bootstrap: createBootstrap({
        projects: [project],
        agents: [createAgent()],
        sessions,
      }),
    });
    const user = await renderApp(client);
    const sessionNavigation = screen.getByRole("navigation", {
      name: "Project sessions",
    });

    for (const status of statuses) {
      const badges = within(sessionNavigation).getAllByLabelText(
        `Session status: ${capitalize(status)}`,
      );
      expect(badges[0]).toBeVisible();
    }
    expect(
      within(sessionNavigation).getByText("Process ended · exit 17"),
    ).toBeVisible();
    expect(
      within(sessionNavigation).getByText("Executable not found"),
    ).toBeVisible();
    expect(
      within(sessionNavigation).getByText("Exited with code 1"),
    ).toBeVisible();

    await user.click(
      within(sessionNavigation).getByRole("button", {
        name: /Exited session/,
      }),
    );
    expect(
      within(screen.getByRole("main")).getByText("Ended · exit 17"),
    ).toBeVisible();

    await user.click(
      within(sessionNavigation).getByRole("button", { name: /Failed process/ }),
    );
    expect(
      within(screen.getByRole("main")).getByText("Ended · exit 1"),
    ).toBeVisible();
  });

  it("lists, filters, and executes the complete command palette", async () => {
    const project = createProject();
    const client = createMockIpcClient({
      bootstrap: createBootstrap({ projects: [project] }),
    });
    const user = await renderApp(client);

    await user.click(
      screen.getByRole("button", {
        name: "Open command palette, Ctrl+K",
      }),
    );
    const dialog = await screen.findByRole("dialog", {
      name: "Command palette",
    });
    const expectedCommands = [
      "Add Project",
      "Switch Project",
      "New Session",
      "Open Session",
      "Stop Session",
      "Restart Session",
      "Open Grid",
      "Open Settings",
      "Open Diagnostics",
    ];

    for (const label of expectedCommands) {
      expect(within(dialog).getByText(label, { exact: true })).toBeVisible();
    }

    const search = within(dialog).getByRole("combobox", {
      name: "Search commands",
    });
    await user.type(search, "settings");
    const matches = within(dialog).getAllByRole("option");
    expect(matches).toHaveLength(1);
    expect(matches[0]).toHaveAccessibleName(/Open Settings/);
    await user.keyboard("{Enter}");

    expect(
      screen.queryByRole("dialog", { name: "Command palette" }),
    ).not.toBeInTheDocument();
    expect(
      within(screen.getByRole("main")).getByRole("heading", {
        name: "Settings",
        level: 1,
      }),
    ).toBeVisible();
  });

  it("supports Linux Control shortcuts for palette, new session, grid, and numbered focus", async () => {
    const project = createProject();
    const olderSession = createSession({
      id: "older-session",
      name: "Older session",
      updatedAtMs: TEST_TIME + 1,
    });
    const newestSession = createSession({
      id: "newest-session",
      name: "Newest session",
      updatedAtMs: TEST_TIME + 2,
    });
    const client = createMockIpcClient({
      platform: "linux",
      bootstrap: createBootstrap({
        projects: [project],
        agents: [createAgent()],
        sessions: [olderSession, newestSession],
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

    await user.keyboard("{Control>}{Shift>}g{/Shift}{/Control}");
    expect(
      within(screen.getByRole("main")).getByRole("heading", {
        name: "Session Grid",
        level: 1,
      }),
    ).toBeVisible();

    await user.keyboard("{Control>}1{/Control}");
    expect(
      within(screen.getByRole("main")).getByRole("heading", {
        name: "Newest session",
        level: 1,
      }),
    ).toBeVisible();
  });

  it("uses the macOS Meta modifier for global shortcuts", async () => {
    const project = createProject();
    const olderSession = createSession({
      id: "mac-older-session",
      name: "Mac older session",
      updatedAtMs: TEST_TIME + 1,
    });
    const newestSession = createSession({
      id: "mac-newest-session",
      name: "Mac newest session",
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

    await user.keyboard("{Meta>}{Shift>}g{/Shift}{/Meta}");
    expect(
      within(screen.getByRole("main")).getByRole("heading", {
        name: "Session Grid",
        level: 1,
      }),
    ).toBeVisible();

    await user.keyboard("{Meta>}1{/Meta}");
    expect(
      within(screen.getByRole("main")).getByRole("heading", {
        name: "Mac newest session",
        level: 1,
      }),
    ).toBeVisible();
  });

  it("treats compact navigation as a modal drawer and closes it with Escape", async () => {
    const originalMatchMedia = window.matchMedia;
    let breakpointListener:
      | ((event: MediaQueryListEvent) => void)
      | undefined;
    Object.defineProperty(window, "matchMedia", {
      configurable: true,
      value: vi.fn().mockReturnValue({
        matches: true,
        media: "(max-width: 47.99rem)",
        onchange: null,
        addListener: vi.fn(),
        removeListener: vi.fn(),
        addEventListener: vi.fn((_type, listener) => {
          breakpointListener = listener as (event: MediaQueryListEvent) => void;
        }),
        removeEventListener: vi.fn(),
        dispatchEvent: vi.fn(),
      } satisfies MediaQueryList),
    });
    try {
      const client = createMockIpcClient();
      const user = await renderApp(client);
      await user.click(
        screen.getByRole("button", { name: "Open navigation" }),
      );

      const workspace = screen.getByRole("main", { hidden: true });
      await waitFor(() => {
        expect(workspace).toHaveAttribute("inert");
        expect(workspace).toHaveAttribute("aria-hidden", "true");
      });
      expect(
        screen.getByRole("button", { name: "Close navigation" }),
      ).toBeInTheDocument();

      act(() => {
        breakpointListener?.({ matches: false } as MediaQueryListEvent);
      });
      await waitFor(() => {
        expect(workspace).not.toHaveAttribute("inert");
        expect(workspace).not.toHaveAttribute("aria-hidden");
      });

      await user.click(
        screen.getByRole("button", { name: "Open navigation" }),
      );
      await user.keyboard("{Escape}");
      await waitFor(() => {
        expect(workspace).not.toHaveAttribute("inert");
        expect(workspace).not.toHaveAttribute("aria-hidden");
      });
    } finally {
      Object.defineProperty(window, "matchMedia", {
        configurable: true,
        value: originalMatchMedia,
      });
    }
  });

  it("does not capture application shortcuts from the terminal root", async () => {
    const project = createProject();
    const firstSession = createSession({
      id: "first-session",
      name: "First session",
      updatedAtMs: TEST_TIME + 2,
    });
    const secondSession = createSession({
      id: "second-session",
      name: "Terminal-owned session",
      updatedAtMs: TEST_TIME + 1,
    });
    const client = createMockIpcClient({
      platform: "linux",
      bootstrap: createBootstrap({
        projects: [project],
        agents: [createAgent()],
        sessions: [firstSession, secondSession],
      }),
    });
    const user = await renderApp(client);
    const navigation = screen.getByRole("navigation", {
      name: "Project sessions",
    });
    await user.click(
      within(navigation).getByRole("button", {
        name: /Terminal-owned session/,
      }),
    );
    const terminal = within(screen.getByRole("main")).getByRole("region", {
      name: "Terminal host for Terminal-owned session",
    });
    await user.click(terminal);
    expect(terminal).toHaveFocus();

    await user.keyboard("{Control>}k{/Control}");
    await user.keyboard("{Control>}1{/Control}");

    expect(
      screen.queryByRole("dialog", { name: "Command palette" }),
    ).not.toBeInTheDocument();
    expect(
      within(screen.getByRole("main")).getByRole("heading", {
        name: "Terminal-owned session",
        level: 1,
      }),
    ).toBeVisible();
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
    await user.click(
      within(screen.getByRole("main")).getByRole("button", {
        name: "Add Project",
      }),
    );
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

  it("reports a daemon disconnect and reconnects through a fresh bootstrap", async () => {
    const project = createProject();
    const session = createSession({ name: "Offline metadata" });
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
    await user.click(
      within(
        screen.getByRole("navigation", { name: "Project sessions" }),
      ).getByRole("button", { name: /Offline metadata/ }),
    );
    expect(
      within(screen.getByRole("main")).getByRole("button", { name: "Restart" }),
    ).toBeDisabled();
    expect(
      within(screen.getByRole("main")).getByRole("button", { name: "Stop Process" }),
    ).toBeDisabled();
    await user.click(
      within(banner).getByRole("button", { name: "Reconnect" }),
    );

    await waitFor(() => {
      expect(client.initialize).toHaveBeenCalledTimes(2);
      expect(screen.queryByText(/^Daemon connected/)).not.toBeInTheDocument();
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    });
    expect(client.subscribe).toHaveBeenCalledTimes(2);
    expect(client.listenerCount()).toBe(1);
    expect(
      within(screen.getByRole("main")).getByRole("heading", {
        name: session.name,
        level: 1,
      }),
    ).toBeVisible();
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

    await user.click(
      within(
        screen.getByRole("navigation", { name: "Project sessions" }),
      ).getByRole("button", { name: /Test session/ }),
    );
    await user.click(
      within(screen.getByRole("main")).getByRole("button", {
        name: "Remove Worktree",
      }),
    );
    const dialog = await screen.findByRole("dialog", {
      name: "Remove Worktree",
    });
    expect(
      within(dialog).getByRole("button", { name: "Remove Worktree" }),
    ).toBeDisabled();
    expect(dialog).toHaveTextContent("Ignored files are present");
    expect(dialog).toHaveTextContent("assume-unchanged");
    expect(dialog).toHaveTextContent("skip-worktree");
    expect(dialog).toHaveTextContent("Git has locked this worktree");
    expect(dialog).toHaveTextContent("live session is still using");
    expect(dialog).toHaveTextContent("Another session or operation is using");
    expect(client.removeWorktree).not.toHaveBeenCalled();
  });

  it("distinguishes stopping a process, deleting metadata, and removing a worktree", async () => {
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
    const sessionNavigation = screen.getByRole("navigation", {
      name: "Project sessions",
    });

    await user.click(
      within(sessionNavigation).getByRole("button", { name: /Running work/ }),
    );
    const runningWorkspace = within(screen.getByRole("main"));
    expect(
      runningWorkspace.getByRole("button", { name: "Stop Process" }),
    ).toBeEnabled();
    expect(
      runningWorkspace.getByRole("button", { name: "Delete Session" }),
    ).toBeDisabled();
    expect(
      runningWorkspace.getByRole("button", { name: "Remove Worktree" }),
    ).toBeDisabled();
    await user.click(
      runningWorkspace.getByRole("button", { name: "Stop Process" }),
    );
    const stopDialog = await screen.findByRole("dialog", { name: "Stop Process" });
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

    await user.click(
      within(sessionNavigation).getByRole("button", { name: /Stopped work/ }),
    );
    const stoppedWorkspace = within(screen.getByRole("main"));
    await user.click(
      stoppedWorkspace.getByRole("button", { name: "Remove Worktree" }),
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
    await waitFor(() => {
      expect(client.prepareWorktreeRemoval).toHaveBeenCalledWith(worktree.id);
    });
    await user.click(
      within(worktreeDialog).getByRole("button", { name: "Remove Worktree" }),
    );
    await waitFor(() => {
      expect(client.removeWorktree).toHaveBeenCalledWith({
        worktreeId: worktree.id,
        confirmationToken: "safe-removal-token",
      });
    });

    await user.click(
      stoppedWorkspace.getByRole("button", { name: "Delete Session" }),
    );
    const deleteDialog = await screen.findByRole("dialog", {
      name: "Delete Session",
    });
    expect(deleteDialog).toHaveTextContent("This deletes session metadata only.");
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
  });

  it("keeps a retained worktree manageable after session metadata is deleted", async () => {
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

    await user.click(
      within(
        screen.getByRole("navigation", { name: "Project sessions" }),
      ).getByRole("button", { name: /Test session/ }),
    );
    await user.click(
      within(screen.getByRole("main")).getByRole("button", {
        name: "Delete Session",
      }),
    );
    await user.click(
      within(
        await screen.findByRole("dialog", { name: "Delete Session" }),
      ).getByRole("button", { name: "Delete Session" }),
    );

    const retainedAction = await screen.findByRole("button", {
      name: "Remove retained worktree agent/retained",
    });
    expect(retainedAction).toBeEnabled();
    await user.click(retainedAction);
    const removeDialog = await screen.findByRole("dialog", {
      name: "Remove Worktree",
    });
    await user.click(
      await within(removeDialog).findByRole("button", {
        name: "Remove Worktree",
      }),
    );

    await waitFor(() => {
      expect(client.removeWorktree).toHaveBeenCalledWith({
        worktreeId: worktree.id,
        confirmationToken: "retained-removal-token",
      });
      expect(
        screen.queryByRole("button", {
          name: "Remove retained worktree agent/retained",
        }),
      ).not.toBeInTheDocument();
    });
  });

});

async function renderApp(client: MockIpcClient) {
  const user = userEvent.setup();
  render(<App client={client} initialView="session" />);
  await screen.findByRole("button", { name: "Settings" });
  expect(screen.queryByText(/^Daemon connected/)).not.toBeInTheDocument();
  return user;
}

async function openNewSession(user: ReturnType<typeof userEvent.setup>) {
  await user.click(
    screen.getByRole("button", {
      name: /^New SessionCtrl T$/,
    }),
  );
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
        executablePath: `/usr/local/bin/${agent.command.executable}`,
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
  return `${value.charAt(0).toUpperCase()}${value.slice(1)}`;
}
