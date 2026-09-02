import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { Session } from "../../../ipc/types";
import type { BrowserRuntime } from "../browser/browser-runtime";
import type { LiveTerminalTransport } from "../terminal/LiveTerminal";
import {
  CANVAS_STORAGE_KEY,
  type BrowserCanvasNode,
  type CanvasDocument,
  type NoteCanvasNode,
  type TerminalCanvasNode,
} from "./canvas-state";
import { CanvasWorkspace } from "./CanvasWorkspace";

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

const BROWSER_NODE: BrowserCanvasNode = {
  id: "browser-test",
  kind: "browser",
  title: "Browser",
  url: "https://docs.example.com/guide",
  x: 160,
  y: 120,
  width: 640,
  height: 420,
};

const NOTE_NODE: NoteCanvasNode = {
  id: "note-test",
  kind: "note",
  title: "Notes",
  text: "Review the integration",
  x: 840,
  y: 120,
};

const LIVE_SESSION: Session = {
  id: "0198f000-0000-7000-8000-000000000004",
  projectId: PROJECT.id,
  name: "Terminal 1",
  agentId: SHELL_AGENT.id,
  cwd: PROJECT.path,
  pid: 123,
  ptyId: "pty-browser-handoff",
  status: "running",
  createdAtMs: 2,
  updatedAtMs: 2,
};

const TERMINAL_NODE: TerminalCanvasNode = {
  id: "terminal-test",
  kind: "terminal",
  title: "Terminal 1",
  sessionId: LIVE_SESSION.id,
  preset: "shell",
  x: 840,
  y: 120,
  width: 432,
  height: 256,
};

describe("CanvasWorkspace", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
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
    render(
      <CanvasWorkspace
        isConnected
        projects={[PROJECT]}
        project={PROJECT}
        agents={[SHELL_AGENT]}
        sessions={[]}
        onAddProject={vi.fn()}
        onNewSession={vi.fn()}
        onSelectSession={vi.fn()}
        onCreateCustomAgent={vi.fn()}
        onCreateSession={onCreateSession}
        onStartSession={onStartSession}
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

  it("adds an integrated browser card to the persisted canvas", async () => {
    const user = userEvent.setup();
    const { container } = renderCanvas();

    await user.click(screen.getByRole("button", { name: "Add browser" }));

    const browser = screen.getByRole("article", {
      name: "Browser, browser canvas item",
    });
    expect(
      within(browser).getByRole("region", {
        name: "Browser surface for Browser",
      }),
    ).toBeVisible();
    expect(within(browser).getByRole("textbox", { name: "Address" })).toHaveValue(
      "",
    );
    expect(container.querySelector("iframe")).not.toBeInTheDocument();

    await waitFor(() => {
      expect(readCanvasDocument().nodes).toContainEqual(
        expect.objectContaining({
          kind: "browser",
          title: "Browser",
          url: "",
          width: 640,
          height: 420,
        }),
      );
    });
  });

  it("persists a normalized address entered in the browser chrome", async () => {
    const user = userEvent.setup();
    renderCanvas();
    await user.click(screen.getByRole("button", { name: "Add browser" }));

    const browser = screen.getByRole("article", {
      name: "Browser, browser canvas item",
    });
    const address = within(browser).getByRole("textbox", { name: "Address" });
    await user.type(address, "docs.example.com/guides?mode=compact{Enter}");

    expect(address).toHaveValue(
      "https://docs.example.com/guides?mode=compact",
    );
    await waitFor(() => {
      const persistedBrowser = readCanvasDocument().nodes.find(
        (node) => node.kind === "browser",
      );
      expect(persistedBrowser).toEqual(
        expect.objectContaining({
          kind: "browser",
          url: "https://docs.example.com/guides?mode=compact",
        }),
      );
    });
  });

  it("keeps address-field arrow keys out of canvas panning", () => {
    seedCanvasDocument([BROWSER_NODE]);
    const { container } = renderCanvas();
    const viewport = container.querySelector<HTMLElement>(".canvas-viewport");
    expect(viewport).not.toBeNull();
    viewport!.scrollLeft = 2_000;
    viewport!.scrollTop = 1_500;

    const address = screen.getByRole("textbox", { name: "Address" });
    address.focus();
    fireEvent.keyDown(address, { key: "ArrowRight" });
    fireEvent.keyDown(address, { key: "ArrowDown" });

    expect(viewport!.scrollLeft).toBe(2_000);
    expect(viewport!.scrollTop).toBe(1_500);
  });

  it("connects a browser to a note and appends its URL as plain text", async () => {
    const user = userEvent.setup();
    seedCanvasDocument([BROWSER_NODE, NOTE_NODE]);
    renderCanvas();

    const browser = screen.getByRole("article", {
      name: "Browser, browser canvas item",
    });
    const note = screen.getByRole("article", {
      name: "Notes, note canvas item",
    });
    await user.click(
      within(browser).getByRole("button", {
        name: "Start connection from Browser",
      }),
    );
    await user.click(
      within(note).getByRole("button", { name: "Connect to Notes" }),
    );

    const inspector = screen.getByRole("region", {
      name: "Connections for Notes",
    });
    await user.click(
      within(inspector).getByRole("button", {
        name: "Add browser URL to Notes",
      }),
    );

    expect(within(note).getByRole("textbox", { name: "Notes content" })).toHaveValue(
      "Review the integration\n\nhttps://docs.example.com/guide",
    );
    await waitFor(() => {
      const document = readCanvasDocument();
      expect(document.connections).toContainEqual(
        expect.objectContaining({
          sourceNodeId: "browser-test",
          targetNodeId: "note-test",
        }),
      );
      expect(document.nodes.find((node) => node.id === NOTE_NODE.id)).toEqual(
        expect.objectContaining({
          text: "Review the integration\n\nhttps://docs.example.com/guide",
        }),
      );
    });
  });

  it("inserts a POSIX-quoted browser URL into a live terminal without submitting it", async () => {
    const user = userEvent.setup();
    stubMatchMedia();
    const browserUrl =
      "https://example.test/it's/$(touch-pwned)?q=a;b|c&next=`id`";
    const writeTerminal = vi.fn<LiveTerminalTransport["writeTerminal"]>(
      async () => undefined,
    );
    seedCanvasDocument([{ ...BROWSER_NODE, url: browserUrl }, TERMINAL_NODE]);
    renderCanvas({
      sessions: [LIVE_SESSION],
      writeTerminal,
      subscribeTerminal: vi.fn(async () => vi.fn()),
    });

    const browser = screen.getByRole("article", {
      name: "Browser, browser canvas item",
    });
    const terminal = screen.getByRole("article", {
      name: "Terminal 1, terminal canvas item",
    });
    await user.click(
      within(browser).getByRole("button", {
        name: "Start connection from Browser",
      }),
    );
    await user.click(
      within(terminal).getByRole("button", {
        name: "Connect to Terminal 1",
      }),
    );

    const inspector = screen.getByRole("region", {
      name: "Connections for Terminal 1",
    });
    await user.click(
      within(inspector).getByRole("button", {
        name: "Insert browser URL into Terminal 1",
      }),
    );

    await waitFor(() => expect(writeTerminal).toHaveBeenCalledOnce());
    const [sessionId, bytes] = writeTerminal.mock.calls[0] ?? [];
    const payload = new TextDecoder().decode(bytes);
    expect(sessionId).toBe(LIVE_SESSION.id);
    expect(payload).toBe(
      "'https://example.test/it'\\''s/$(touch-pwned)?q=a;b|c&next=`id`'",
    );
    expect(payload).not.toMatch(/[\r\n]/);
    expect(within(inspector).getByRole("status")).toHaveTextContent(
      "Inserted a shell-safe URL into Terminal 1. Review it before pressing Enter.",
    );
  });

  it("hides an active native browser surface while its card is manipulated", async () => {
    const user = userEvent.setup();
    stubVisibleBrowserGeometry();
    seedCanvasDocument([BROWSER_NODE]);
    renderCanvas({ browserRuntime: createAvailableBrowserRuntime() });

    const browser = screen.getByRole("article", {
      name: "Browser, browser canvas item",
    });
    await user.click(browser);
    const webPage = within(browser).getByRole("region", { name: "Web page" });
    await waitFor(() => {
      expect(browser).toHaveAttribute("aria-selected", "true");
      expect(webPage).toHaveAttribute("data-native-browser-visible", "true");
    });

    const header = browser.querySelector<HTMLElement>(".canvas-node__header");
    expect(header).not.toBeNull();
    fireEvent.pointerDown(header!, {
      pointerId: 41,
      clientX: 100,
      clientY: 100,
    });
    await waitFor(() =>
      expect(webPage).toHaveAttribute("data-native-browser-visible", "false"),
    );
    expect(
      within(browser).getByText(
        "The browser is hidden while the canvas item moves.",
      ),
    ).toBeVisible();
    fireEvent.pointerUp(header!, { pointerId: 41 });
    await waitFor(() =>
      expect(webPage).toHaveAttribute("data-native-browser-visible", "true"),
    );

    const resizeHandle = within(browser).getByRole("button", {
      name: "Resize Browser",
    });
    fireEvent.pointerDown(resizeHandle, {
      pointerId: 42,
      clientX: 100,
      clientY: 100,
    });
    await waitFor(() =>
      expect(webPage).toHaveAttribute("data-native-browser-visible", "false"),
    );
    fireEvent.pointerCancel(resizeHandle, { pointerId: 42 });
    await waitFor(() =>
      expect(webPage).toHaveAttribute("data-native-browser-visible", "true"),
    );

    browser.focus();
    fireEvent.keyDown(browser, { key: "ArrowRight" });
    await waitFor(() =>
      expect(webPage).toHaveAttribute("data-native-browser-visible", "false"),
    );
    fireEvent.keyUp(browser, { key: "ArrowRight" });
    await waitFor(() =>
      expect(webPage).toHaveAttribute("data-native-browser-visible", "true"),
    );

    resizeHandle.focus();
    fireEvent.keyDown(resizeHandle, { key: "ArrowDown" });
    await waitFor(() =>
      expect(webPage).toHaveAttribute("data-native-browser-visible", "false"),
    );
    fireEvent.keyUp(resizeHandle, { key: "ArrowDown" });
    await waitFor(() =>
      expect(webPage).toHaveAttribute("data-native-browser-visible", "true"),
    );
  });

  it("hides the active browser until keyboard, wheel, and scroll movement settles", async () => {
    const user = userEvent.setup();
    const runtime = createAvailableBrowserRuntime();
    stubVisibleBrowserGeometry();
    seedCanvasDocument([BROWSER_NODE]);
    const { container } = renderCanvas({ browserRuntime: runtime });

    const browser = screen.getByRole("article", {
      name: "Browser, browser canvas item",
    });
    await user.click(browser);
    const webPage = within(browser).getByRole("region", { name: "Web page" });
    await waitFor(() =>
      expect(webPage).toHaveAttribute("data-native-browser-visible", "true"),
    );
    await waitFor(() =>
      expect(runtime.update).toHaveBeenCalledWith(
        expect.objectContaining({ visible: true }),
      ),
    );
    const viewport = container.querySelector<HTMLElement>(".canvas-viewport");
    expect(viewport).not.toBeNull();

    vi.useFakeTimers();
    vi.mocked(runtime.update).mockClear();
    viewport!.focus();
    fireEvent.keyDown(viewport!, { key: "ArrowRight" });
    expect(webPage).toHaveAttribute("data-native-browser-visible", "false");
    expect(runtime.update).toHaveBeenCalledWith(
      expect.objectContaining({ visible: false }),
    );

    act(() => vi.advanceTimersByTime(100));
    fireEvent.wheel(viewport!, { deltaY: 40 });
    act(() => vi.advanceTimersByTime(100));
    fireEvent.scroll(viewport!);
    act(() => vi.advanceTimersByTime(159));
    expect(webPage).toHaveAttribute("data-native-browser-visible", "false");

    act(() => vi.advanceTimersByTime(1));
    expect(webPage).toHaveAttribute("data-native-browser-visible", "true");
    act(() => vi.advanceTimersByTime(20));
    expect(runtime.update).toHaveBeenLastCalledWith(
      expect.objectContaining({ visible: true }),
    );
  });

  it("hides the active browser while smooth focus and fit scrolling settles", async () => {
    const user = userEvent.setup();
    const runtime = createAvailableBrowserRuntime();
    stubVisibleBrowserGeometry();
    seedCanvasDocument([BROWSER_NODE]);
    const { container } = renderCanvas({ browserRuntime: runtime });

    const browser = screen.getByRole("article", {
      name: "Browser, browser canvas item",
    });
    await user.click(browser);
    const webPage = within(browser).getByRole("region", { name: "Web page" });
    await waitFor(() =>
      expect(webPage).toHaveAttribute("data-native-browser-visible", "true"),
    );
    const viewport = container.querySelector<HTMLElement>(".canvas-viewport");
    expect(viewport).not.toBeNull();
    const scrollTo = vi.fn();
    Object.defineProperties(viewport!, {
      clientWidth: { configurable: true, value: 1_000 },
      clientHeight: { configurable: true, value: 700 },
      scrollTo: { configurable: true, value: scrollTo },
    });

    vi.useFakeTimers();
    fireEvent.click(screen.getByRole("button", { name: "Show canvas items" }));
    const panel = screen.getByRole("region", { name: "Canvas items" });
    fireEvent.click(within(panel).getByRole("button", { name: /Browser/ }));
    expect(scrollTo).toHaveBeenLastCalledWith(
      expect.objectContaining({ behavior: "smooth" }),
    );
    expect(webPage).toHaveAttribute("data-native-browser-visible", "false");
    act(() => vi.advanceTimersByTime(160));
    expect(webPage).toHaveAttribute("data-native-browser-visible", "true");

    fireEvent.click(screen.getByRole("button", { name: "Fit canvas to items" }));
    expect(scrollTo).toHaveBeenLastCalledWith(
      expect.objectContaining({ behavior: "smooth" }),
    );
    expect(webPage).toHaveAttribute("data-native-browser-visible", "false");
    act(() => vi.advanceTimersByTime(160));
    expect(webPage).toHaveAttribute("data-native-browser-visible", "true");
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
});

interface RenderCanvasOptions {
  readonly sessions?: readonly Session[];
  readonly browserRuntime?: BrowserRuntime;
  readonly subscribeTerminal?: LiveTerminalTransport["subscribeTerminal"];
  readonly writeTerminal?: LiveTerminalTransport["writeTerminal"];
}

function renderCanvas({
  sessions = [],
  browserRuntime,
  subscribeTerminal = vi.fn(async () => vi.fn()),
  writeTerminal = vi.fn(async () => undefined),
}: RenderCanvasOptions = {}) {
  return render(
    <CanvasWorkspace
      isConnected
      projects={[]}
      agents={[]}
      sessions={sessions}
      onAddProject={vi.fn()}
      onNewSession={vi.fn()}
      onSelectSession={vi.fn()}
      onCreateCustomAgent={vi.fn()}
      onCreateSession={vi.fn()}
      onStartSession={vi.fn()}
      subscribeTerminal={subscribeTerminal}
      writeTerminal={writeTerminal}
      resizeTerminal={vi.fn(async () => undefined)}
      browserRuntime={browserRuntime}
    />,
  );
}

function seedCanvasDocument(
  nodes: CanvasDocument["nodes"],
  connections: CanvasDocument["connections"] = [],
) {
  const document: CanvasDocument = {
    version: 2,
    nodes,
    connections,
    zoom: 1,
  };
  localStorage.setItem(CANVAS_STORAGE_KEY, JSON.stringify(document));
}

function readCanvasDocument(): CanvasDocument {
  return JSON.parse(
    localStorage.getItem(CANVAS_STORAGE_KEY) ?? "{}",
  ) as CanvasDocument;
}

function createAvailableBrowserRuntime(): BrowserRuntime {
  return {
    isAvailable: () => true,
    open: vi.fn(async () => undefined),
    navigate: vi.fn(async () => undefined),
    update: vi.fn(async () => undefined),
    reload: vi.fn(async () => undefined),
    goBack: vi.fn(async () => undefined),
    goForward: vi.fn(async () => undefined),
    focus: vi.fn(async () => undefined),
    close: vi.fn(async () => undefined),
    openExternal: vi.fn(async () => undefined),
  };
}

function stubMatchMedia() {
  vi.stubGlobal(
    "matchMedia",
    vi.fn(() => ({
      matches: false,
      media: "(prefers-reduced-motion: reduce)",
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(() => false),
    })),
  );
}

function stubVisibleBrowserGeometry() {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
    function getBoundingClientRect(this: HTMLElement) {
      if (this.hasAttribute("data-browser-surface-node-id")) {
        return new DOMRect(100, 100, 640, 360);
      }
      if (this.hasAttribute("data-browser-viewport")) {
        return new DOMRect(0, 0, 1_024, 768);
      }
      return new DOMRect();
    },
  );
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
