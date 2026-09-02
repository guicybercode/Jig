import type { ComponentProps } from "react";
import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { IpcError } from "../../../ipc/client";
import type { Session, Worktree } from "../../../ipc/types";
import { CANVAS_STORAGE_KEY, parseCanvasDocument } from "./canvas-state";
import { CanvasWorkspace } from "./CanvasWorkspace";

vi.mock("../terminal/LiveTerminal", () => ({
  LiveTerminal: ({ session }: { readonly session: Session }) => (
    <div data-testid={`live-terminal-${session.id}`} />
  ),
}));

const PROJECT = {
  id: "0198f000-0000-7000-8000-000000000001",
  name: "Jig",
  path: "/workspace/jig",
  repositoryRoot: "/workspace/jig",
  currentBranch: "main",
  createdAtMs: 1,
  lastOpenedAtMs: 1,
} as const;

const SHELL_AGENT = {
  id: "0198f000-0000-7000-8000-000000000002",
  displayName: "Shell",
  source: "built_in",
  command: { executable: "/bin/zsh", args: ["-l"], env: {} },
  enabled: true,
} as const;

const OTHER_PROJECT = {
  ...PROJECT,
  id: "0198f000-0000-7000-8000-000000000010",
  name: "Other project",
  path: "/workspace/other",
  repositoryRoot: "/workspace/other",
} as const;

const STOPPED_SESSION: Session = {
  id: "0198f000-0000-7000-8000-000000000003",
  projectId: PROJECT.id,
  name: "Review agent",
  agentId: SHELL_AGENT.id,
  cwd: "/workspace/jig/.worktrees/review",
  branch: "agent/review",
  worktreeId: "0198f000-0000-7000-8000-000000000004",
  worktreePath: "/workspace/jig/.worktrees/review",
  status: "exited",
  createdAtMs: 2,
  updatedAtMs: 3,
};

const MANAGED_WORKTREE: Worktree = {
  id: "0198f000-0000-7000-8000-000000000004",
  projectId: PROJECT.id,
  sessionId: STOPPED_SESSION.id,
  path: "/workspace/jig/.worktrees/review",
  branch: "agent/review",
  isDirty: false,
  state: "active",
  createdAtMs: 2,
  updatedAtMs: 3,
};

describe("CanvasWorkspace", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("renders the first-launch terminal and note composition", () => {
    const { container } = renderCanvas();

    expect(
      screen.getByRole("heading", { name: "My Workspace", level: 1 }),
    ).toBeVisible();
    expect(
      screen.getAllByRole("region", { name: /Terminal surface/ }),
    ).toHaveLength(2);
    expect(screen.getByRole("textbox", { name: "Notes content" })).toHaveValue(
      "Write a note for this workspace…",
    );
    expect(screen.getByText("Canvas saved locally")).toBeVisible();
    expect(screen.queryByText(/Daemon (connected|offline)/)).not.toBeInTheDocument();
    expect(
      container.querySelectorAll("[data-connection-id]"),
    ).toHaveLength(2);
  });

  it("adds notes, creates a two-click connection, and resets the layout", async () => {
    const user = userEvent.setup();
    renderCanvas();

    await user.click(screen.getByRole("button", { name: "Add note" }));
    expect(screen.getAllByRole("textbox", { name: /Notes content/ })).toHaveLength(
      2,
    );

    const secondTerminal = screen.getByRole("article", {
      name: "Terminal 2, terminal canvas item",
    });
    await user.click(
      within(secondTerminal).getByRole("button", {
        name: "Start connection from Terminal 2",
      }),
    );
    const addedNote = screen.getAllByRole("article", {
      name: "Notes, note canvas item",
    })[1];
    expect(addedNote).toBeDefined();
    await user.click(
      within(addedNote as HTMLElement).getByRole("button", {
        name: "Connect to Notes",
      }),
    );

    await waitFor(() => {
      const persisted = JSON.parse(
        localStorage.getItem(CANVAS_STORAGE_KEY) ?? "{}",
      ) as { connections?: readonly unknown[] };
      expect(persisted.connections).toHaveLength(3);
    });
    expect(
      document.querySelectorAll("[data-connection-id]"),
    ).toHaveLength(3);

    await user.click(
      screen.getByRole("button", { name: "Reset canvas layout" }),
    );
    expect(screen.getAllByRole("textbox", { name: /Notes content/ })).toHaveLength(
      1,
    );
  });

  it("creates and starts a real project session from a terminal card", async () => {
    const user = userEvent.setup();
    const createdSession = {
      id: "0198f000-0000-7000-8000-000000000003",
      projectId: PROJECT.id,
      name: "Terminal 1",
      agentId: SHELL_AGENT.id,
      cwd: PROJECT.path,
      status: "unknown",
      createdAtMs: 2,
      updatedAtMs: 2,
    } as const;
    const onCreateSession = vi.fn().mockResolvedValue(createdSession);
    const onStartSession = vi.fn().mockResolvedValue({
      ...createdSession,
      status: "running",
      pid: 123,
    });
    const onSelectSession = vi.fn();
    render(
      <CanvasWorkspace
        isConnected
        projects={[PROJECT]}
        project={PROJECT}
        agents={[SHELL_AGENT]}
        sessions={[]}
        worktrees={[]}
        sessionFocusRevision={0}
        onSelectSession={onSelectSession}
        onCreateCustomAgent={vi.fn()}
        onCreateSession={onCreateSession}
        onStartSession={onStartSession}
        onRestartSession={vi.fn()}
        onRenameSession={vi.fn()}
        onStopSession={vi.fn()}
        onDeleteSession={vi.fn()}
        onRemoveWorktree={vi.fn()}
        onGitStatus={vi.fn()}
        onOpenPath={vi.fn()}
        subscribeTerminal={vi.fn()}
        writeTerminal={vi.fn()}
        resizeTerminal={vi.fn()}
      />,
    );

    const terminal = screen.getByRole("article", {
      name: "Terminal 1, terminal canvas item",
    });
    await user.click(
      within(terminal).getByRole("button", { name: "Start terminal" }),
    );

    await waitFor(() => {
      expect(onCreateSession).toHaveBeenCalledWith({
        projectId: PROJECT.id,
        name: "Terminal 1",
        agentId: SHELL_AGENT.id,
        isolation: "current",
        relativeDirectory: undefined,
      });
      expect(onStartSession).toHaveBeenCalledWith(createdSession.id);
      expect(onSelectSession).toHaveBeenCalledWith(createdSession.id);
    });
    const persisted = JSON.parse(
      localStorage.getItem(CANVAS_STORAGE_KEY) ?? "{}",
    ) as { nodes?: readonly { id: string; sessionId?: string }[] };
    expect(
      persisted.nodes?.find((node) => node.id === "terminal-primary")
        ?.sessionId,
    ).toBe(createdSession.id);
  });

  it("configures a Codex terminal from the terminal tool", async () => {
    const user = userEvent.setup();
    renderCanvas();

    await user.click(screen.getByRole("button", { name: "Add terminal card" }));
    const dialog = screen.getByRole("dialog", { name: "New Terminal" });
    await user.click(within(dialog).getByRole("radio", { name: "Codex" }));
    expect(within(dialog).getByLabelText("Terminal name")).toHaveValue("Codex");
    expect(within(dialog).getByLabelText("Command")).toHaveValue("codex");
    await user.click(
      within(dialog).getByRole("button", { name: "Create terminal" }),
    );

    expect(
      screen.getByRole("article", { name: "Codex, terminal canvas item" }),
    ).toBeVisible();
    await waitFor(() => {
      const persisted = JSON.parse(
        localStorage.getItem(CANVAS_STORAGE_KEY) ?? "{}",
      ) as {
        nodes?: readonly { readonly title?: string; readonly preset?: string }[];
      };
      expect(persisted.nodes).toContainEqual(
        expect.objectContaining({ title: "Codex", preset: "codex" }),
      );
    });
  });

  it("moves a selected node with keyboard and pointer alternatives", async () => {
    const user = userEvent.setup();
    const { container } = renderCanvas();
    const terminal = screen.getByRole("article", {
      name: "Terminal 1, terminal canvas item",
    });

    terminal.focus();
    await user.keyboard("{ArrowRight}{Alt>}{ArrowUp}{/Alt}");

    await waitFor(() => {
      expect(readNodePosition("terminal-primary")).toEqual({ x: 178, y: 209 });
    });

    const header = terminal.querySelector<HTMLElement>(".canvas-node__header");
    expect(header).not.toBeNull();
    fireEvent.pointerDown(header!, {
      pointerId: 7,
      clientX: 100,
      clientY: 100,
    });
    fireEvent.pointerMove(header!, {
      pointerId: 7,
      clientX: 132,
      clientY: 124,
    });
    fireEvent.pointerUp(header!, {
      pointerId: 7,
      clientX: 132,
      clientY: 124,
    });

    await waitFor(() => {
      expect(readNodePosition("terminal-primary")).toEqual({ x: 210, y: 233 });
    });
    expect(container.querySelector(".canvas-node--selected")).toBe(terminal);
  });

  it("resizes a terminal by keyboard and pointer", async () => {
    const user = userEvent.setup();
    renderCanvas();
    const terminal = screen.getByRole("article", {
      name: "Terminal 1, terminal canvas item",
    });
    await user.click(terminal);
    const handle = within(terminal).getByRole("button", {
      name: "Resize Terminal 1",
    });

    handle.focus();
    await user.keyboard("{ArrowRight}{Alt>}{ArrowDown}{/Alt}");
    await waitFor(() => {
      expect(readTerminalSize("terminal-primary")).toEqual({
        width: 448,
        height: 257,
      });
    });

    fireEvent.pointerDown(handle, {
      pointerId: 11,
      clientX: 100,
      clientY: 100,
    });
    fireEvent.pointerMove(handle, {
      pointerId: 11,
      clientX: 164,
      clientY: 132,
    });
    fireEvent.pointerUp(handle, {
      pointerId: 11,
      clientX: 164,
      clientY: 132,
    });

    await waitFor(() => {
      expect(readTerminalSize("terminal-primary")).toEqual({
        width: 512,
        height: 289,
      });
    });
    expect(terminal).toHaveStyle({ width: "512px", height: "289px" });
  });

  it("shows canvas items and fits them from the reference controls", async () => {
    const user = userEvent.setup();
    const { container } = renderCanvas();
    const viewport = container.querySelector<HTMLElement>(".canvas-viewport");
    expect(viewport).not.toBeNull();
    const scrollTo = vi.fn();
    Object.defineProperties(viewport!, {
      clientWidth: { configurable: true, value: 1000 },
      clientHeight: { configurable: true, value: 700 },
      scrollTo: { configurable: true, value: scrollTo },
    });

    await user.click(screen.getByRole("button", { name: "Show canvas items" }));
    const panel = screen.getByRole("region", { name: "Canvas items" });
    expect(within(panel).getByRole("button", { name: /Terminal 1/ })).toBeVisible();
    expect(within(panel).getByRole("button", { name: /Notes/ })).toBeVisible();

    await user.click(screen.getByRole("button", { name: "Fit canvas to items" }));
    expect(scrollTo).toHaveBeenCalledOnce();
  });

  it("pans the canvas up, down, left, and right", () => {
    const { container } = renderCanvas();
    const viewport = container.querySelector<HTMLElement>(".canvas-viewport");
    expect(viewport).not.toBeNull();
    viewport!.scrollLeft = 2_000;
    viewport!.scrollTop = 1_500;
    viewport!.focus();

    fireEvent.keyDown(viewport!, { key: "ArrowLeft" });
    fireEvent.keyDown(viewport!, { key: "ArrowUp" });
    expect(viewport!.scrollLeft).toBe(1_920);
    expect(viewport!.scrollTop).toBe(1_420);
    fireEvent.keyDown(viewport!, { key: "ArrowRight" });
    fireEvent.keyDown(viewport!, { key: "ArrowDown" });
    expect(viewport!.scrollLeft).toBe(2_000);
    expect(viewport!.scrollTop).toBe(1_500);

    fireEvent.pointerDown(viewport!, {
      button: 0,
      pointerId: 19,
      clientX: 100,
      clientY: 100,
    });
    fireEvent.pointerMove(viewport!, {
      pointerId: 19,
      clientX: 140,
      clientY: 160,
    });
    fireEvent.pointerUp(viewport!, {
      pointerId: 19,
      clientX: 140,
      clientY: 160,
    });
    expect(viewport!.scrollLeft).toBe(1_960);
    expect(viewport!.scrollTop).toBe(1_440);
  });

  it("leaves arrow-key editing inside notes to the textarea", () => {
    const { container } = renderCanvas();
    const viewport = container.querySelector<HTMLElement>(".canvas-viewport");
    expect(viewport).not.toBeNull();
    viewport!.scrollLeft = 2_000;
    viewport!.scrollTop = 1_500;
    const note = screen.getByRole("textbox", { name: "Notes content" });

    expect(fireEvent.keyDown(note, { key: "ArrowLeft" })).toBe(true);
    expect(fireEvent.keyDown(note, { key: "ArrowUp" })).toBe(true);

    expect(viewport!.scrollLeft).toBe(2_000);
    expect(viewport!.scrollTop).toBe(1_500);
  });

  it("uses one compact terminal width for rendering and connection geometry", async () => {
    const user = userEvent.setup();
    const view = renderProjectCanvas({ isCompact: true });
    const viewport = view.container.querySelector<HTMLElement>(
      ".canvas-viewport",
    );
    expect(viewport).not.toBeNull();
    Object.defineProperty(viewport, "clientWidth", {
      configurable: true,
      value: 320,
    });
    fireEvent(window, new Event("resize"));
    const terminal = screen.getByRole("article", {
      name: "Terminal 1, terminal canvas item",
    });

    await waitFor(() => {
      expect(Number.parseFloat(terminal.style.width)).toBeCloseTo(272);
      expect(connectionEndpointX(view.container)).toBeCloseTo(442);
    });

    await user.click(terminal);
    const resize = within(terminal).getByRole("button", {
      name: "Resize Terminal 1",
    });
    fireEvent.keyDown(resize, { key: "ArrowRight" });
    await waitFor(() => {
      expect(readTerminalSize("terminal-primary").width).toBe(448);
    });
    fireEvent.pointerDown(resize, {
      pointerId: 73,
      clientX: 100,
      clientY: 100,
    });
    fireEvent.pointerMove(resize, {
      pointerId: 73,
      clientX: 110,
      clientY: 100,
    });
    fireEvent.pointerUp(resize, { pointerId: 73 });
    await waitFor(() => {
      expect(readTerminalSize("terminal-primary").width).toBe(458);
    });
    expect(Number.parseFloat(terminal.style.width)).toBeCloseTo(272);

    const zoomIn = screen.getByRole("button", { name: "Zoom in" });
    for (let step = 0; step < 5; step += 1) {
      await user.click(zoomIn);
    }
    await waitFor(() => {
      const modelWidth = Number.parseFloat(terminal.style.width);
      expect(modelWidth).toBeCloseTo(272 / 1.5);
      expect(modelWidth * 1.5).toBeCloseTo(272);
      expect(connectionEndpointX(view.container)).toBeCloseTo(
        170 + 272 / 1.5,
      );
    });
    expect(
      readCanvasDocument().nodes.find((node) => node.id === "terminal-primary"),
    ).toEqual(expect.objectContaining({ width: 458 }));

    view.rerender(
      <CanvasWorkspace {...view.props} isCompact={false} />,
    );
    await waitFor(() => {
      expect(Number.parseFloat(terminal.style.width)).toBe(458);
      expect(connectionEndpointX(view.container)).toBe(628);
    });
  });

  it("fits compact nodes using their geometry at the destination zoom", async () => {
    localStorage.setItem(
      CANVAS_STORAGE_KEY,
      JSON.stringify({
        version: 1,
        nodes: [
          {
            id: "terminal-wide",
            kind: "terminal",
            title: "Wide terminal",
            x: 0,
            y: 0,
            width: 960,
            height: 256,
            preset: "shell",
          },
        ],
        connections: [],
        zoom: 1.5,
        hiddenSessionIds: [],
      }),
    );
    const user = userEvent.setup();
    const view = renderCanvas({ isCompact: true });
    const viewport = view.container.querySelector<HTMLElement>(
      ".canvas-viewport",
    );
    expect(viewport).not.toBeNull();
    const scrollTo = vi.fn();
    Object.defineProperties(viewport, {
      clientWidth: { configurable: true, value: 320 },
      clientHeight: { configurable: true, value: 640 },
      scrollTo: { configurable: true, value: scrollTo },
    });
    fireEvent(window, new Event("resize"));
    const terminal = screen.getByRole("article", {
      name: "Wide terminal, terminal canvas item",
    });

    await user.click(screen.getByRole("button", { name: "Fit canvas to items" }));

    await waitFor(() => {
      expect(screen.getByText("50%")).toBeVisible();
      expect(Number.parseFloat(terminal.style.width) * 0.5).toBeCloseTo(272);
      expect(scrollTo).toHaveBeenLastCalledWith(
        expect.objectContaining({ left: 1_476, top: 1_244 }),
      );
    });
  });

  it("removes a selected item's connection from the inspector", async () => {
    const user = userEvent.setup();
    const { container } = renderCanvas();
    const terminal = screen.getByRole("article", {
      name: "Terminal 1, terminal canvas item",
    });

    await user.click(terminal);
    const inspector = screen.getByRole("region", {
      name: "Connections for Terminal 1",
    });
    await user.click(
      within(inspector).getByRole("button", {
        name: "Remove connection to Notes",
      }),
    );

    await waitFor(() => {
      const persisted = JSON.parse(
        localStorage.getItem(CANVAS_STORAGE_KEY) ?? "{}",
      ) as { connections?: readonly unknown[] };
      expect(persisted.connections).toHaveLength(1);
    });
    expect(
      container.querySelectorAll("[data-connection-id]"),
    ).toHaveLength(1);
  });

  it("reconciles every selected-project session and hides attached nodes from other projects", async () => {
    const user = userEvent.setup();
    const otherSession: Session = {
      ...STOPPED_SESSION,
      id: "0198f000-0000-7000-8000-000000000011",
      projectId: OTHER_PROJECT.id,
      name: "Other agent",
      cwd: OTHER_PROJECT.path,
      worktreeId: undefined,
      worktreePath: undefined,
    };
    const view = renderProjectCanvas({
      projects: [PROJECT, OTHER_PROJECT],
      sessions: [STOPPED_SESSION, otherSession],
    });

    const projectTerminal = await screen.findByRole("article", {
      name: "Review agent, terminal canvas item",
    });
    expect(projectTerminal).toBeVisible();
    await user.click(projectTerminal);
    expect(projectTerminal).toHaveAttribute("data-selected", "true");
    expect(
      screen.queryByRole("article", {
        name: "Other agent, terminal canvas item",
      }),
    ).not.toBeInTheDocument();

    view.rerender(
      <CanvasWorkspace {...view.props} project={OTHER_PROJECT} />,
    );

    expect(
      await screen.findByRole("article", {
        name: "Other agent, terminal canvas item",
      }),
    ).toBeVisible();
    expect(
      screen.queryByRole("article", {
        name: "Review agent, terminal canvas item",
      }),
    ).not.toBeInTheDocument();

    view.rerender(
      <CanvasWorkspace {...view.props} project={PROJECT} />,
    );

    expect(
      await screen.findByRole("article", {
        name: "Review agent, terminal canvas item",
      }),
    ).not.toHaveAttribute("data-selected");
    const persisted = readCanvasDocument();
    expect(persisted.nodes).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          sessionId: STOPPED_SESSION.id,
          projectId: PROJECT.id,
        }),
        expect.objectContaining({
          sessionId: otherSession.id,
          projectId: OTHER_PROJECT.id,
        }),
      ]),
    );
  });

  it("removes an attached card only from the canvas and persists its dismissal", async () => {
    const user = userEvent.setup();
    const onDeleteSession = vi.fn();
    const firstView = renderProjectCanvas({
      sessions: [STOPPED_SESSION],
      onDeleteSession,
    });
    const terminal = await screen.findByRole("article", {
      name: "Review agent, terminal canvas item",
    });

    await user.click(
      within(terminal).getByRole("button", {
        name: "Remove Review agent from canvas",
      }),
    );

    await waitFor(() => {
      expect(
        screen.queryByRole("article", {
          name: "Review agent, terminal canvas item",
        }),
      ).not.toBeInTheDocument();
      expect(readCanvasDocument().hiddenSessionIds).toContain(
        STOPPED_SESSION.id,
      );
    });
    expect(onDeleteSession).not.toHaveBeenCalled();

    firstView.unmount();
    renderProjectCanvas({ sessions: [STOPPED_SESSION], onDeleteSession });
    await waitFor(() => {
      expect(
        screen.queryByRole("article", {
          name: "Review agent, terminal canvas item",
        }),
      ).not.toBeInTheDocument();
    });
    expect(onDeleteSession).not.toHaveBeenCalled();
  });

  it("reconciles project sessions again after resetting the canvas document", async () => {
    const user = userEvent.setup();
    renderProjectCanvas({ sessions: [STOPPED_SESSION] });
    const terminal = await screen.findByRole("article", {
      name: "Review agent, terminal canvas item",
    });
    await user.click(
      within(terminal).getByRole("button", {
        name: "Remove Review agent from canvas",
      }),
    );
    await waitFor(() => {
      expect(terminal).not.toBeInTheDocument();
      expect(readCanvasDocument().hiddenSessionIds).toContain(
        STOPPED_SESSION.id,
      );
    });

    await user.click(
      screen.getByRole("button", { name: "Reset canvas layout" }),
    );

    await waitFor(() => {
      expect(
        screen.getByRole("article", {
          name: "Review agent, terminal canvas item",
        }),
      ).toBeVisible();
      expect(
        readCanvasDocument().nodes.some(
          (node) =>
            node.kind === "terminal" &&
            node.sessionId === STOPPED_SESSION.id,
        ),
      ).toBe(true);
      expect(readCanvasDocument().hiddenSessionIds).not.toContain(
        STOPPED_SESSION.id,
      );
    });
  });

  it("reveals, selects, focuses, and centers repeated session focus requests", async () => {
    localStorage.setItem(
      CANVAS_STORAGE_KEY,
      JSON.stringify({
        version: 1,
        nodes: [],
        connections: [],
        zoom: 1,
        hiddenSessionIds: [STOPPED_SESSION.id],
      }),
    );
    const scrollTo = vi.fn();
    Object.defineProperty(HTMLElement.prototype, "scrollTo", {
      configurable: true,
      value: scrollTo,
    });
    vi.stubGlobal(
      "matchMedia",
      vi.fn().mockReturnValue({ matches: true }),
    );
    const view = renderProjectCanvas({
      sessions: [STOPPED_SESSION],
      selectedSessionId: STOPPED_SESSION.id,
      sessionFocusRevision: 1,
    });

    const terminal = await screen.findByRole("article", {
      name: "Review agent, terminal canvas item",
    });
    await waitFor(() => {
      expect(terminal).toHaveAttribute("data-selected", "true");
      expect(terminal).toHaveFocus();
      expect(scrollTo).toHaveBeenLastCalledWith(
        expect.objectContaining({ behavior: "auto" }),
      );
    });
    expect(readCanvasDocument().hiddenSessionIds).not.toContain(
      STOPPED_SESSION.id,
    );

    scrollTo.mockClear();
    screen.getByRole("main").focus();
    view.rerender(
      <CanvasWorkspace
        {...view.props}
        selectedSessionId={STOPPED_SESSION.id}
        sessionFocusRevision={2}
      />,
    );

    await waitFor(() => {
      expect(terminal).toHaveFocus();
      expect(scrollTo).toHaveBeenCalledWith(
        expect.objectContaining({ behavior: "auto" }),
      );
    });
    vi.unstubAllGlobals();
  });

  it("exposes all stopped-session actions and reports direct action errors", async () => {
    const user = userEvent.setup();
    const onStartSession = vi.fn().mockResolvedValue(STOPPED_SESSION);
    const onRestartSession = vi
      .fn()
      .mockRejectedValueOnce(
        new IpcError({
          code: "restart_failed",
          message: "Restart failed safely",
          action: "Inspect the session and retry.",
        }),
      )
      .mockResolvedValue(STOPPED_SESSION);
    const onRenameSession = vi.fn();
    const onDeleteSession = vi.fn();
    const onRemoveWorktree = vi.fn();
    const onGitStatus = vi.fn();
    const onOpenPath = vi.fn().mockResolvedValue(undefined);
    renderProjectCanvas({
      sessions: [STOPPED_SESSION],
      worktrees: [MANAGED_WORKTREE],
      onStartSession,
      onRestartSession,
      onRenameSession,
      onDeleteSession,
      onRemoveWorktree,
      onGitStatus,
      onOpenPath,
    });
    const terminal = await screen.findByRole("article", {
      name: "Review agent, terminal canvas item",
    });
    const actionsTrigger = within(terminal).getByRole("button", {
      name: "Session actions for Review agent",
    });
    const openActions = async () => {
      await user.click(actionsTrigger);
    };

    await openActions();
    const actions = within(terminal).getByRole("group", {
      name: "Actions for Review agent",
    });
    expect(
      within(actions).getByRole("button", { name: "Stop process" }),
    ).toHaveAttribute("aria-disabled", "true");
    await user.click(
      within(actions).getByRole("button", { name: "Start session" }),
    );
    await waitFor(() =>
      expect(onStartSession).toHaveBeenCalledWith(STOPPED_SESSION.id),
    );
    expect(actionsTrigger).toHaveFocus();
    expect(actionsTrigger).toHaveAttribute("aria-expanded", "false");

    await openActions();
    await user.click(
      within(terminal).getByRole("button", { name: "Restart session" }),
    );
    expect(await within(terminal).findByRole("alert")).toHaveTextContent(
      "Restart failed safely",
    );
    await user.click(
      within(terminal).getByRole("button", {
        name: "Dismiss session action error",
      }),
    );
    await user.click(
      within(terminal).getByRole("button", { name: "Restart session" }),
    );
    await waitFor(() =>
      expect(onRestartSession).toHaveBeenCalledTimes(2),
    );
    expect(actionsTrigger).toHaveFocus();

    const overlayActions = [
      ["Rename session", onRenameSession, STOPPED_SESSION.id],
      ["Git status", onGitStatus, STOPPED_SESSION.id],
      ["Delete session metadata", onDeleteSession, STOPPED_SESSION.id],
      ["Remove worktree", onRemoveWorktree, MANAGED_WORKTREE.id],
    ] as const;
    for (const [label, callback, expectedId] of overlayActions) {
      await openActions();
      await user.click(within(terminal).getByRole("button", { name: label }));
      expect(callback).toHaveBeenCalledWith(expectedId);
      expect(actionsTrigger).toHaveFocus();
    }

    await openActions();
    await user.click(
      within(terminal).getByRole("button", {
        name: "Open working directory",
      }),
    );
    await waitFor(() =>
      expect(onOpenPath).toHaveBeenCalledWith(MANAGED_WORKTREE.path),
    );
    expect(
      within(terminal).queryByRole("button", { name: "Session details" }),
    ).not.toBeInTheDocument();
  });

  it("closes session actions with Escape and outside pointer or focus", async () => {
    const user = userEvent.setup();
    renderProjectCanvas({
      sessions: [STOPPED_SESSION],
      worktrees: [MANAGED_WORKTREE],
    });
    const terminal = await screen.findByRole("article", {
      name: "Review agent, terminal canvas item",
    });
    const trigger = within(terminal).getByRole("button", {
      name: "Session actions for Review agent",
    });

    await user.click(trigger);
    const restart = within(terminal).getByRole("button", {
      name: "Restart session",
    });
    restart.focus();
    await user.keyboard("{Escape}");
    expect(trigger).toHaveFocus();
    expect(
      within(terminal).queryByRole("group", {
        name: "Actions for Review agent",
      }),
    ).not.toBeInTheDocument();

    await user.click(trigger);
    const addNote = screen.getByRole("button", { name: "Add note" });
    await user.click(addNote);
    expect(addNote).toHaveFocus();
    expect(trigger).toHaveAttribute("aria-expanded", "false");

    await user.click(trigger);
    const zoomIn = screen.getByRole("button", { name: "Zoom in" });
    zoomIn.focus();
    await waitFor(() =>
      expect(trigger).toHaveAttribute("aria-expanded", "false"),
    );
    expect(zoomIn).toHaveFocus();
  });

  it("scopes project-owned notes and terminal drafts while retaining legacy nodes", async () => {
    const user = userEvent.setup();
    localStorage.setItem(
      CANVAS_STORAGE_KEY,
      JSON.stringify({
        version: 1,
        zoom: 1,
        connections: [],
        nodes: [
          {
            id: "own-note",
            kind: "note",
            projectId: PROJECT.id,
            title: "Project note",
            text: "Only in Jig",
            x: 0,
            y: 0,
          },
          {
            id: "other-terminal-draft",
            kind: "terminal",
            projectId: OTHER_PROJECT.id,
            title: "Other draft",
            preset: "shell",
            width: 432,
            height: 256,
            x: 20,
            y: 20,
          },
          {
            id: "legacy-note",
            kind: "note",
            title: "Legacy note",
            text: "Shared compatibility node",
            x: 60,
            y: 60,
          },
        ],
      }),
    );
    const view = renderProjectCanvas({ projects: [PROJECT, OTHER_PROJECT] });

    expect(
      screen.getByRole("article", { name: "Project note, note canvas item" }),
    ).toBeVisible();
    expect(
      screen.getByRole("article", { name: "Legacy note, note canvas item" }),
    ).toBeVisible();
    expect(
      screen.queryByRole("article", {
        name: "Other draft, terminal canvas item",
      }),
    ).not.toBeInTheDocument();
    view.rerender(
      <CanvasWorkspace {...view.props} project={OTHER_PROJECT} />,
    );
    expect(
      screen.queryByRole("article", {
        name: "Project note, note canvas item",
      }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("article", {
        name: "Other draft, terminal canvas item",
      }),
    ).toBeVisible();
    expect(
      screen.getByRole("article", { name: "Legacy note, note canvas item" }),
    ).toBeVisible();

    await user.click(screen.getByRole("button", { name: "Add note" }));
    await waitFor(() => {
      expect(readCanvasDocument().nodes).toContainEqual(
        expect.objectContaining({
          kind: "note",
          title: "Notes",
          projectId: OTHER_PROJECT.id,
        }),
      );
    });
  });

  it("resets only the selected project's canvas layout", async () => {
    const user = userEvent.setup();
    const otherSession: Session = {
      ...STOPPED_SESSION,
      id: "0198f000-0000-7000-8000-000000000011",
      projectId: OTHER_PROJECT.id,
      name: "Other review agent",
    };
    localStorage.setItem(
      CANVAS_STORAGE_KEY,
      JSON.stringify({
        version: 1,
        zoom: 0.8,
        nodes: [
          {
            id: "selected-project-note",
            kind: "note",
            projectId: PROJECT.id,
            title: "Selected project note",
            text: "Reset me",
            x: 0,
            y: 0,
          },
          {
            id: "other-project-note",
            kind: "note",
            projectId: OTHER_PROJECT.id,
            title: "Other project note",
            text: "Keep me",
            x: 20,
            y: 20,
          },
          {
            id: "legacy-note",
            kind: "note",
            title: "Legacy note",
            text: "Keep compatibility",
            x: 40,
            y: 40,
          },
        ],
        connections: [
          {
            id: "cross-project-connection",
            sourceNodeId: "selected-project-note",
            targetNodeId: "other-project-note",
          },
        ],
        hiddenSessionIds: [STOPPED_SESSION.id, otherSession.id],
      }),
    );
    renderProjectCanvas({
      projects: [PROJECT, OTHER_PROJECT],
      sessions: [STOPPED_SESSION, otherSession],
    });

    await user.click(
      screen.getByRole("button", { name: "Reset canvas layout" }),
    );

    await waitFor(() => {
      const document = readCanvasDocument();
      expect(document.nodes.map((node) => node.id)).toEqual(
        expect.arrayContaining([
          "other-project-note",
          "legacy-note",
          `terminal-session-${STOPPED_SESSION.id}`,
        ]),
      );
      expect(document.nodes.map((node) => node.id)).not.toContain(
        "selected-project-note",
      );
      expect(document.connections).toEqual([]);
      expect(document.zoom).toBe(0.8);
      expect(document.hiddenSessionIds).toEqual([otherSession.id]);
    });
  });

  it("removes a worktree resolved by session association", async () => {
    const user = userEvent.setup();
    const session = { ...STOPPED_SESSION, worktreeId: undefined };
    const onRemoveWorktree = vi.fn();
    renderProjectCanvas({
      sessions: [session],
      worktrees: [MANAGED_WORKTREE],
      onRemoveWorktree,
    });
    const terminal = await screen.findByRole("article", {
      name: "Review agent, terminal canvas item",
    });

    await user.click(
      within(terminal).getByRole("button", {
        name: "Session actions for Review agent",
      }),
    );
    await user.click(
      within(terminal).getByRole("button", { name: "Remove worktree" }),
    );

    expect(onRemoveWorktree).toHaveBeenCalledWith(MANAGED_WORKTREE.id);
  });

  it("enables stop for a live session while protecting destructive actions", async () => {
    const user = userEvent.setup();
    const runningSession: Session = {
      ...STOPPED_SESSION,
      status: "running",
      pid: 811,
    };
    const onStopSession = vi.fn();
    renderProjectCanvas({
      sessions: [runningSession],
      worktrees: [MANAGED_WORKTREE],
      onStopSession,
    });
    const terminal = await screen.findByRole("article", {
      name: "Review agent, terminal canvas item",
    });

    await user.click(
      within(terminal).getByRole("button", {
        name: "Session actions for Review agent",
      }),
    );
    expect(
      within(terminal).getByRole("button", { name: "Start session" }),
    ).toHaveAttribute("aria-disabled", "true");
    expect(
      within(terminal).getByRole("button", {
        name: "Delete session metadata",
      }),
    ).toHaveAttribute("aria-disabled", "true");
    expect(
      within(terminal).getByRole("button", { name: "Remove worktree" }),
    ).toHaveAttribute("aria-disabled", "true");
    await user.click(
      within(terminal).getByRole("button", { name: "Stop process" }),
    );
    expect(onStopSession).toHaveBeenCalledWith(runningSession.id);
  });

  it("never falls back to stale paths when a managed worktree is missing", async () => {
    const user = userEvent.setup();
    const onStartSession = vi.fn();
    const onRestartSession = vi.fn();
    const onRemoveWorktree = vi.fn();
    const onGitStatus = vi.fn();
    const onOpenPath = vi.fn();
    renderProjectCanvas({
      sessions: [STOPPED_SESSION],
      worktrees: [],
      onStartSession,
      onRestartSession,
      onRemoveWorktree,
      onGitStatus,
      onOpenPath,
    });
    const terminal = await screen.findByRole("article", {
      name: "Review agent, terminal canvas item",
    });

    const startTerminal = within(terminal).getByRole("button", {
      name: "Start terminal",
    });
    expect(startTerminal).toHaveAttribute("aria-disabled", "true");
    await user.click(startTerminal);
    await user.click(
      within(terminal).getByRole("button", {
        name: "Session actions for Review agent",
      }),
    );
    for (const label of [
      "Start session",
      "Restart session",
      "Git status",
      "Open working directory",
      "Remove worktree",
    ]) {
      const action = within(terminal).getByRole("button", { name: label });
      expect(action).toHaveAttribute("aria-disabled", "true");
      await user.click(action);
    }
    expect(onStartSession).not.toHaveBeenCalled();
    expect(onRestartSession).not.toHaveBeenCalled();
    expect(onRemoveWorktree).not.toHaveBeenCalled();
    expect(onGitStatus).not.toHaveBeenCalled();
    expect(onOpenPath).not.toHaveBeenCalled();
  });
});

function renderCanvas(
  overrides: Partial<ComponentProps<typeof CanvasWorkspace>> = {},
) {
  const props: ComponentProps<typeof CanvasWorkspace> = {
    isConnected: true,
    projects: [],
    agents: [],
    sessions: [],
    worktrees: [],
    sessionFocusRevision: 0,
    onSelectSession: vi.fn(),
    onCreateCustomAgent: vi.fn(),
    onCreateSession: vi.fn(),
    onStartSession: vi.fn(),
    onRestartSession: vi.fn(),
    onRenameSession: vi.fn(),
    onStopSession: vi.fn(),
    onDeleteSession: vi.fn(),
    onRemoveWorktree: vi.fn(),
    onGitStatus: vi.fn(),
    onOpenPath: vi.fn(),
    subscribeTerminal: vi.fn(),
    writeTerminal: vi.fn(),
    resizeTerminal: vi.fn(),
    ...overrides,
  };
  return { ...render(<CanvasWorkspace {...props} />), props };
}

function renderProjectCanvas(
  overrides: Partial<ComponentProps<typeof CanvasWorkspace>> = {},
) {
  const props: ComponentProps<typeof CanvasWorkspace> = {
    isConnected: true,
    projects: [PROJECT],
    project: PROJECT,
    agents: [SHELL_AGENT],
    sessions: [],
    worktrees: [],
    sessionFocusRevision: 0,
    onSelectSession: vi.fn(),
    onCreateCustomAgent: vi.fn().mockResolvedValue(SHELL_AGENT),
    onCreateSession: vi.fn().mockResolvedValue(STOPPED_SESSION),
    onStartSession: vi.fn().mockResolvedValue(STOPPED_SESSION),
    onRestartSession: vi.fn().mockResolvedValue(STOPPED_SESSION),
    onRenameSession: vi.fn(),
    onStopSession: vi.fn(),
    onDeleteSession: vi.fn(),
    onRemoveWorktree: vi.fn(),
    onGitStatus: vi.fn(),
    onOpenPath: vi.fn().mockResolvedValue(undefined),
    subscribeTerminal: vi.fn(),
    writeTerminal: vi.fn(),
    resizeTerminal: vi.fn(),
    ...overrides,
  };
  return { ...render(<CanvasWorkspace {...props} />), props };
}

function readCanvasDocument() {
  return parseCanvasDocument(localStorage.getItem(CANVAS_STORAGE_KEY));
}

function connectionEndpointX(container: HTMLElement): number {
  const path = container.querySelector("[data-connection-id] path");
  const coordinates = path?.getAttribute("d")?.match(/-?\d+(?:\.\d+)?/g);
  if (!coordinates || coordinates.length < 2) {
    throw new Error("Expected a rendered canvas connection path.");
  }
  return Number(coordinates[coordinates.length - 2]);
}

function readNodePosition(nodeId: string) {
  const persisted = JSON.parse(
    localStorage.getItem(CANVAS_STORAGE_KEY) ?? "{}",
  ) as {
    nodes?: readonly { readonly id?: string; readonly x?: number; readonly y?: number }[];
  };
  const node = persisted.nodes?.find((candidate) => candidate.id === nodeId);
  return { x: node?.x, y: node?.y };
}

function readTerminalSize(nodeId: string) {
  const persisted = JSON.parse(
    localStorage.getItem(CANVAS_STORAGE_KEY) ?? "{}",
  ) as {
    nodes?: readonly {
      readonly id?: string;
      readonly width?: number;
      readonly height?: number;
    }[];
  };
  const node = persisted.nodes?.find((candidate) => candidate.id === nodeId);
  return { width: node?.width, height: node?.height };
}
