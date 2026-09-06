import { useImperativeHandle, useState } from "react";
import type { ComponentProps, Ref } from "react";
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

import { IpcError } from "../../../ipc/client";
import type { Session, Worktree } from "../../../ipc/types";
import type { BrowserRuntime } from "../browser/browser-runtime";
import type {
  LiveTerminalInputHandle,
  LiveTerminalTransport,
} from "../terminal/LiveTerminal";
import {
  CANVAS_STORAGE_KEY,
  parseCanvasDocument,
  type BrowserCanvasNode,
  type CanvasDocument,
  type NoteCanvasNode,
  type TerminalCanvasNode,
} from "./canvas-state";
import { CanvasWorkspace } from "./CanvasWorkspace";

vi.mock("../terminal/LiveTerminal", () => ({
  LiveTerminal: ({ session, inputRef, writeTerminal }: {
    readonly session: Session;
    readonly inputRef?: Ref<LiveTerminalInputHandle>;
    readonly writeTerminal: LiveTerminalTransport["writeTerminal"];
  }) => {
    useImperativeHandle(inputRef, () => ({
      writeInput: async (encode) => {
        await writeTerminal(session.id, encode({
          bracketedPasteMode: true,
          applicationCursorKeysMode: false,
        }));
      },
    }), [session.id, writeTerminal]);
    return <div data-testid={`live-terminal-${session.id}`} />;
  },
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

  describe("Prompt Composer", () => {
    it("delivers a multiline draft through the attached terminal transport", async () => {
      const user = userEvent.setup();
      const writeTerminal = vi.fn<LiveTerminalTransport["writeTerminal"]>().mockResolvedValue(undefined);
      seedCanvasDocument([TERMINAL_NODE]);
      const { props } = renderProjectCanvas({ sessions: [LIVE_SESSION], writeTerminal });
      const trigger = screen.getByRole("button", { name: "Open Prompt Composer for Terminal 1" });
      await user.click(trigger);
      expect(trigger).toHaveAttribute("aria-expanded", "true");
      const composer = screen.getByRole("region", { name: "Prompt Composer" });
      const editor = within(composer).getByRole("textbox", { name: "Prompt for Terminal 1" });
      expect(editor).toHaveFocus();
      await user.type(editor, "  Review Linux{Shift>}{Enter}{/Shift}and macOS  ");
      await user.click(within(composer).getByRole("button", { name: "Send prompt" }));

      expect(writeTerminal).toHaveBeenCalledExactlyOnceWith(
        LIVE_SESSION.id,
        new TextEncoder().encode("\x1b[200~  Review Linux\rand macOS  \x1b[201~\r"),
      );
      expect(editor).toHaveValue("");
      expect(readPromptDraft(TERMINAL_NODE.id)).toBe("");
      expect(props.onCreateSession).not.toHaveBeenCalled();
      expect(props.onStartSession).not.toHaveBeenCalled();
    });

    it.each(["disconnected", "stopped", "unattached"] as const)(
      "allows drafting but never starts or writes a %s terminal",
      async (availability) => {
        const user = userEvent.setup();
        seedCanvasDocument([{
          ...TERMINAL_NODE,
          sessionId: availability === "unattached" ? undefined : LIVE_SESSION.id,
        }]);
        const { props } = renderProjectCanvas({
          isConnected: availability !== "disconnected",
          sessions: availability === "unattached" ? [] : [{
            ...LIVE_SESSION,
            status: availability === "stopped" ? "exited" : "running",
          }],
        });
        await user.click(screen.getByRole("button", { name: "Open Prompt Composer for Terminal 1" }));
        const editor = screen.getByRole("textbox", { name: "Prompt for Terminal 1" });
        await user.keyboard("{Enter}{ArrowUp}");
        await user.type(editor, "Continue when ready{Enter}");

        expect(editor).toHaveValue("Continue when ready");
        expect(screen.getByRole("button", { name: "Send prompt" })).toBeDisabled();
        expect(readPromptDraft(TERMINAL_NODE.id)).toBe("Continue when ready");
        expect(props.writeTerminal).not.toHaveBeenCalled();
        expect(props.onCreateSession).not.toHaveBeenCalled();
        expect(props.onStartSession).not.toHaveBeenCalled();
      },
    );

    it("restores the same terminal's offline draft after workspace reload", async () => {
      const user = userEvent.setup();
      seedCanvasDocument([TERMINAL_NODE]);
      const first = renderProjectCanvas({ isConnected: false });
      await user.click(screen.getByRole("button", { name: "Open Prompt Composer for Terminal 1" }));
      await user.type(screen.getByRole("textbox", { name: "Prompt for Terminal 1" }), "Revisar a implantação");
      await waitFor(() => expect(readPromptDraft(TERMINAL_NODE.id)).toBe("Revisar a implantação"));
      first.unmount();

      const second = renderProjectCanvas({ isConnected: false });
      await user.click(screen.getByRole("button", { name: "Open Prompt Composer for Terminal 1" }));
      expect(screen.getByRole("textbox", { name: "Prompt for Terminal 1" })).toHaveValue("Revisar a implantação");
      expect(first.props.writeTerminal).not.toHaveBeenCalled();
      expect(second.props.writeTerminal).not.toHaveBeenCalled();
      expect(second.props.onStartSession).not.toHaveBeenCalled();
    });

    it.each([false, true])(
      "keeps pending delivery isolated across close/reopen (revised draft: %s)",
      async (reviseDraft) => {
        const user = userEvent.setup();
        const delivery = deferredPromptDelivery();
        const writeTerminal = vi.fn<LiveTerminalTransport["writeTerminal"]>().mockReturnValue(delivery.promise);
        seedCanvasDocument([TERMINAL_NODE]);
        renderProjectCanvas({ sessions: [LIVE_SESSION], writeTerminal });
        const trigger = screen.getByRole("button", { name: "Open Prompt Composer for Terminal 1" });
        await user.click(trigger);
        await user.type(screen.getByRole("textbox", { name: "Prompt for Terminal 1" }), "Repeat{Enter}");
        await user.click(screen.getByRole("button", { name: "Close Prompt Composer" }));
        await user.click(trigger);
        const editor = screen.getByRole("textbox", { name: "Prompt for Terminal 1" });
        expect(editor).toHaveValue("Repeat");
        await user.keyboard("{Enter}");
        expect(writeTerminal).toHaveBeenCalledTimes(1);
        if (reviseDraft) {
          await user.clear(editor);
          await user.type(editor, "Repeat");
        }
        await user.click(screen.getByRole("button", { name: "Close Prompt Composer" }));
        await act(async () => delivery.complete());
        expect(screen.queryByRole("region", { name: "Prompt Composer" })).not.toBeInTheDocument();
        await user.click(trigger);

        expect(screen.getByRole("textbox", { name: "Prompt for Terminal 1" })).toHaveValue(reviseDraft ? "Repeat" : "");
        expect(readPromptDraft(TERMINAL_NODE.id)).toBe(reviseDraft ? "Repeat" : "");
        expect(writeTerminal).toHaveBeenCalledTimes(1);
      },
    );

    it("follows the primary terminal without letting the previous delivery erase its draft", async () => {
      const user = userEvent.setup();
      const delivery = deferredPromptDelivery();
      const secondSession = { ...LIVE_SESSION, id: "0198f000-0000-7000-8000-000000000011", name: "Terminal 2" };
      const secondNode = { ...TERMINAL_NODE, id: "terminal-second", title: "Terminal 2", sessionId: secondSession.id };
      const writeTerminal = vi.fn<LiveTerminalTransport["writeTerminal"]>()
        .mockReturnValueOnce(delivery.promise).mockResolvedValue(undefined);
      seedCanvasDocument([TERMINAL_NODE, secondNode, NOTE_NODE]);
      renderProjectCanvas({ sessions: [LIVE_SESSION, secondSession], writeTerminal });
      await user.click(screen.getByRole("button", { name: "Open Prompt Composer for Terminal 1" }));
      await user.type(screen.getByRole("textbox", { name: "Prompt for Terminal 1" }), "First request{Enter}");
      await user.click(screen.getByRole("article", { name: "Terminal 2, terminal canvas item" }));
      const secondEditor = screen.getByRole("textbox", { name: "Prompt for Terminal 2" });
      await user.type(secondEditor, "Second request");
      await act(async () => delivery.complete());
      expect(secondEditor).toHaveValue("Second request");
      expect(readPromptDraft(TERMINAL_NODE.id)).toBe("");
      await user.keyboard("{Enter}");

      expect(writeTerminal).toHaveBeenNthCalledWith(1, LIVE_SESSION.id, new TextEncoder().encode("\x1b[200~First request\x1b[201~\r"));
      expect(writeTerminal).toHaveBeenNthCalledWith(2, secondSession.id, new TextEncoder().encode("\x1b[200~Second request\x1b[201~\r"));
      await user.click(screen.getByRole("article", { name: "Notes, note canvas item" }));
      expect(screen.queryByRole("region", { name: "Prompt Composer" })).not.toBeInTheDocument();
    });

    it("hides the composer on project changes and completes only the original project's draft", async () => {
      const user = userEvent.setup();
      const delivery = deferredPromptDelivery();
      const otherSession = {
        ...LIVE_SESSION,
        id: "0198f000-0000-7000-8000-000000000011",
        projectId: OTHER_PROJECT.id,
        name: "Other terminal",
        cwd: OTHER_PROJECT.path,
      };
      const otherNode = {
        ...TERMINAL_NODE, id: "terminal-other", title: "Other terminal",
        projectId: OTHER_PROJECT.id, sessionId: otherSession.id,
      };
      const writeTerminal = vi.fn<LiveTerminalTransport["writeTerminal"]>().mockReturnValue(delivery.promise);
      seedCanvasDocument([{ ...TERMINAL_NODE, projectId: PROJECT.id }, otherNode]);
      const props = createProjectCanvasProps({
        projects: [PROJECT, OTHER_PROJECT], sessions: [LIVE_SESSION, otherSession], writeTerminal,
      });
      function ProjectSwitcher() {
        const [project, setProject] = useState<typeof PROJECT | typeof OTHER_PROJECT>(PROJECT);
        return <>
          <button type="button" onClick={() => setProject(OTHER_PROJECT)}>Switch project</button>
          <CanvasWorkspace {...props} project={project} />
        </>;
      }
      render(<ProjectSwitcher />);
      await user.click(screen.getByRole("button", { name: "Open Prompt Composer for Terminal 1" }));
      await user.type(screen.getByRole("textbox", { name: "Prompt for Terminal 1" }), "For Jig{Enter}");
      await user.click(screen.getByRole("button", { name: "Switch project" }));
      expect(screen.queryByRole("region", { name: "Prompt Composer" })).not.toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Open Prompt Composer for Other terminal" }));
      const editor = screen.getByRole("textbox", { name: "Prompt for Other terminal" });
      await user.type(editor, "Private draft");
      await act(async () => delivery.complete());

      expect(editor).toHaveValue("Private draft");
      expect(readPromptDraft(otherNode.id)).toBe("Private draft");
      expect(readPromptDraft(TERMINAL_NODE.id)).toBe("");
      expect(writeTerminal).toHaveBeenCalledExactlyOnceWith(LIVE_SESSION.id, new TextEncoder().encode("\x1b[200~For Jig\x1b[201~\r"));
    });

    it.each(["ctrlKey", "metaKey"] as const)("guards the %s shortcut and Enter against repeat and IME confirmation", async (modifier) => {
      const user = userEvent.setup();
      const writeTerminal = vi.fn<LiveTerminalTransport["writeTerminal"]>().mockResolvedValue(undefined);
      seedCanvasDocument([TERMINAL_NODE]);
      renderProjectCanvas({ sessions: [LIVE_SESSION], writeTerminal });
      const terminal = screen.getByRole("article", { name: "Terminal 1, terminal canvas item" });
      await user.click(terminal);
      const shortcut = { key: "P", shiftKey: true, [modifier]: true };
      fireEvent.keyDown(terminal, { ...shortcut, repeat: true });
      fireEvent.keyDown(terminal, { ...shortcut, isComposing: true });
      expect(screen.queryByRole("region", { name: "Prompt Composer" })).not.toBeInTheDocument();
      fireEvent.keyDown(terminal, shortcut);
      const editor = screen.getByRole("textbox", { name: "Prompt for Terminal 1" });
      await user.type(editor, "Intentional request");
      fireEvent.compositionStart(editor);
      fireEvent.keyDown(editor, shortcut);
      expect(editor).toBeVisible();
      fireEvent.compositionEnd(editor);
      fireEvent.keyDown(editor, { key: "Enter", repeat: true });
      fireEvent.keyDown(editor, { key: "Enter", isComposing: true });
      fireEvent.keyDown(editor, { key: "Enter", keyCode: 229 });
      expect(writeTerminal).not.toHaveBeenCalled();
      expect(editor).toHaveValue("Intentional request");
      await user.keyboard("{Enter}");
      expect(writeTerminal).toHaveBeenCalledExactlyOnceWith(LIVE_SESSION.id, new TextEncoder().encode("\x1b[200~Intentional request\x1b[201~\r"));
      fireEvent.keyDown(editor, shortcut);
      expect(screen.queryByRole("region", { name: "Prompt Composer" })).not.toBeInTheDocument();
      const toolbarTrigger = screen.getByRole("button", { name: "Toggle Prompt Composer" });
      await user.click(toolbarTrigger);
      expect(screen.getByRole("textbox", { name: "Prompt for Terminal 1" })).toHaveValue("");
      await user.keyboard("{Escape}");
      expect(toolbarTrigger).toHaveFocus();
      fireEvent.keyDown(toolbarTrigger, shortcut);
      expect(screen.getByRole("textbox", { name: "Prompt for Terminal 1" })).toHaveFocus();
    });

    it("forwards intentional empty-editor keys as terminal input", async () => {
      const user = userEvent.setup();
      const writeTerminal = vi.fn<LiveTerminalTransport["writeTerminal"]>().mockResolvedValue(undefined);
      seedCanvasDocument([TERMINAL_NODE]);
      renderProjectCanvas({ sessions: [LIVE_SESSION], writeTerminal });
      await user.click(screen.getByRole("button", { name: "Open Prompt Composer for Terminal 1" }));
      for (const key of ["Enter", "Tab", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"]) {
        await user.keyboard(`{${key}}`);
      }
      expect(writeTerminal.mock.calls.map(([sessionId, bytes]) => [sessionId, new TextDecoder().decode(bytes)])).toEqual(
        ["\r", "\t", "\x1b[A", "\x1b[B", "\x1b[D", "\x1b[C"].map((text) => [LIVE_SESSION.id, text]),
      );
      expect(screen.getByRole("textbox", { name: "Prompt for Terminal 1" })).toHaveValue("");
    });

    it("inserts only connected context snapshots and sends them only on explicit submission", async () => {
      const user = userEvent.setup();
      const writeTerminal = vi.fn<LiveTerminalTransport["writeTerminal"]>().mockResolvedValue(undefined);
      seedCanvasDocument([
        TERMINAL_NODE, NOTE_NODE, { ...BROWSER_NODE, url: "docs.example.com/guide" },
        { ...NOTE_NODE, id: "unconnected-note", title: "Unconnected", text: "Not selected as context" },
      ], [
        { id: "terminal-note", sourceNodeId: TERMINAL_NODE.id, targetNodeId: NOTE_NODE.id },
        { id: "browser-terminal", sourceNodeId: BROWSER_NODE.id, targetNodeId: TERMINAL_NODE.id },
      ]);
      const { props } = renderProjectCanvas({ sessions: [LIVE_SESSION], writeTerminal });
      await user.click(screen.getByRole("button", { name: "Open Prompt Composer for Terminal 1" }));
      const composer = screen.getByRole("region", { name: "Prompt Composer" });
      await user.click(within(composer).getByText(/Insert context/));
      expect(within(composer).queryByRole("button", { name: "Insert context from Unconnected" })).not.toBeInTheDocument();
      await user.click(within(composer).getByRole("button", { name: "Insert context from Notes" }));
      await user.click(within(composer).getByRole("button", { name: "Insert context from Browser URL" }));
      const editor = within(composer).getByRole("textbox", { name: "Prompt for Terminal 1" });
      const draft = (editor as HTMLTextAreaElement).value;
      expect(draft).toContain("Context snapshot: Notes\nReview the integration\n");
      expect(draft).toContain("Context snapshot: Browser URL\nhttps://docs.example.com/guide\n");
      fireEvent.change(within(screen.getByRole("article", { name: "Notes, note canvas item" })).getByRole("textbox"), {
        target: { value: "Changed after insertion" },
      });
      expect(editor).toHaveValue(draft);
      expect(writeTerminal).not.toHaveBeenCalled();
      expect(props.onCreateSession).not.toHaveBeenCalled();
      expect(props.onStartSession).not.toHaveBeenCalled();
      await user.click(within(composer).getByRole("button", { name: "Send prompt" }));
      expect(writeTerminal).toHaveBeenCalledExactlyOnceWith(LIVE_SESSION.id, new TextEncoder().encode(`\x1b[200~${draft.replace(/\n/g, "\r")}\x1b[201~\r`));
    });

    it("preserves a rejected transport write for an explicit retry", async () => {
      const user = userEvent.setup();
      const writeTerminal = vi.fn<LiveTerminalTransport["writeTerminal"]>()
        .mockRejectedValueOnce(new Error("Socket disconnected")).mockResolvedValue(undefined);
      seedCanvasDocument([TERMINAL_NODE]);
      renderProjectCanvas({ sessions: [LIVE_SESSION], writeTerminal });
      await user.click(screen.getByRole("button", { name: "Open Prompt Composer for Terminal 1" }));
      const editor = screen.getByRole("textbox", { name: "Prompt for Terminal 1" });
      await user.type(editor, "Keep for retry{Enter}");
      expect(screen.getByRole("alert")).toHaveTextContent("Could not send the prompt");
      expect(editor).toHaveValue("Keep for retry");
      expect(readPromptDraft(TERMINAL_NODE.id)).toBe("Keep for retry");
      await user.click(screen.getByRole("button", { name: "Try again" }));
      expect(writeTerminal).toHaveBeenCalledTimes(2);
      expect(writeTerminal).toHaveBeenLastCalledWith(LIVE_SESSION.id, new TextEncoder().encode("\x1b[200~Keep for retry\x1b[201~\r"));
      expect(editor).toHaveValue("");
    });
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
    renderProjectCanvas({
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
      expect(browser).toHaveAttribute("data-selected", "true");
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

  it("keeps only the primary browser active during group selection and movement", async () => {
    const user = userEvent.setup();
    const runtime = createAvailableBrowserRuntime();
    stubVisibleBrowserGeometry();
    seedCanvasDocument([
      BROWSER_NODE,
      { ...BROWSER_NODE, id: "browser-second", title: "Preview", x: 840 },
    ]);
    renderCanvas({ browserRuntime: runtime });
    const browser = screen.getByRole("article", { name: "Browser, browser canvas item" });
    const preview = screen.getByRole("article", { name: "Preview, browser canvas item" });
    const firstPage = within(browser).getByRole("region", { name: "Web page" });
    const secondPage = within(preview).getByRole("region", { name: "Web page" });

    await user.click(browser);
    await waitFor(() => expect(firstPage).toHaveAttribute("data-native-browser-visible", "true"));
    await user.keyboard("{Shift>}");
    await user.click(preview);
    await user.keyboard("{/Shift}");

    expect(browser).toHaveAttribute("data-selected", "true");
    expect(preview).toHaveAttribute("data-selected", "true");
    await waitFor(() => {
      expect(firstPage).toHaveAttribute("data-native-browser-visible", "false");
      expect(secondPage).toHaveAttribute("data-native-browser-visible", "true");
      expect(runtime.close).toHaveBeenCalledWith({ nodeId: BROWSER_NODE.id });
    });
    expect(runtime.open).toHaveBeenCalledTimes(2);

    const header = within(preview).getByLabelText("Move Preview");
    fireEvent.pointerDown(header, { pointerId: 51, button: 0, clientX: 100, clientY: 100 });
    fireEvent.pointerMove(header, { pointerId: 51, clientX: 124, clientY: 116 });
    expect(secondPage).toHaveAttribute("data-native-browser-visible", "false");
    expect(readNodePosition(BROWSER_NODE.id)).toEqual({ x: 184, y: 136 });
    expect(readNodePosition("browser-second")).toEqual({ x: 864, y: 136 });
    fireEvent.pointerUp(header, { pointerId: 51 });
    await waitFor(() => expect(secondPage).toHaveAttribute("data-native-browser-visible", "true"));

    await user.click(screen.getByRole("button", { name: "Duplicate selected canvas items" }));
    expect(screen.getAllByRole("article", { name: /browser canvas item/ })).toHaveLength(4);
    await waitFor(() => expect(runtime.open).toHaveBeenCalledTimes(3));
    expect(runtime.close).toHaveBeenCalledWith({ nodeId: "browser-second" });
    expect(document.querySelectorAll('[data-native-browser-visible="true"]')).toHaveLength(1);
  });

  it("scopes browsers and URL search to the selected project and hides the surface while searching", async () => {
    const user = userEvent.setup();
    const runtime = createAvailableBrowserRuntime();
    stubVisibleBrowserGeometry();
    seedCanvasDocument([
      { ...BROWSER_NODE, projectId: PROJECT.id },
      { ...BROWSER_NODE, id: "other-browser", title: "Private preview", projectId: OTHER_PROJECT.id, url: "https://other.example.com/private" },
    ]);
    const { props, rerender } = renderProjectCanvas({
      projects: [PROJECT, OTHER_PROJECT],
      browserRuntime: runtime,
    });
    const browser = screen.getByRole("article", { name: "Browser, browser canvas item" });
    const webPage = within(browser).getByRole("region", { name: "Web page" });
    expect(screen.queryByRole("article", { name: /Private preview/ })).not.toBeInTheDocument();
    expect(screen.getByText(/1 browsers/)).toBeVisible();
    await user.click(browser);
    await waitFor(() => expect(webPage).toHaveAttribute("data-native-browser-visible", "true"));

    await user.click(screen.getByRole("button", { name: "Show canvas items" }));
    expect(webPage).toHaveAttribute("data-native-browser-visible", "false");
    expect(screen.getByRole("region", { name: "Canvas items" })).toHaveAttribute("data-browser-obstruction", "true");
    const search = screen.getByRole("searchbox", { name: "Search canvas items" });
    await user.type(search, "docs.example.com guide");
    expect(screen.getByRole("button", { name: /Browser https:\/\/docs.example.com/ })).toBeVisible();
    await user.clear(search);
    await user.type(search, "other.example.com");
    expect(screen.getByText(/No matching items/)).toBeVisible();
    await user.keyboard("{Escape}");
    expect(screen.getByRole("button", { name: "Show canvas items" })).toHaveFocus();
    await waitFor(() => expect(webPage).toHaveAttribute("data-native-browser-visible", "true"));

    await user.click(screen.getByRole("button", { name: "Add browser" }));
    const added = readCanvasDocument().nodes.find((node) => node.kind === "browser" && node.url === "");
    expect(added).toEqual(expect.objectContaining({ projectId: PROJECT.id }));
    rerender(<CanvasWorkspace {...props} project={OTHER_PROJECT} />);
    expect(screen.getAllByRole("article", { name: /browser canvas item/ })).toHaveLength(1);
    expect(screen.getByRole("article", { name: "Private preview, browser canvas item" })).toBeVisible();
    expect(screen.getByText(/1 browsers/)).toBeVisible();
    expect(runtime.close).toHaveBeenCalledWith({ nodeId: BROWSER_NODE.id });
    expect(runtime.open).not.toHaveBeenCalledWith(expect.objectContaining({ nodeId: "other-browser" }));
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

  it("toggles group selection with Shift+click and Shift+Space without child focus collapsing it", async () => {
    const user = userEvent.setup();
    renderCanvas();
    const terminal = screen.getByRole("article", { name: "Terminal 1, terminal canvas item" });
    const other = screen.getByRole("article", { name: "Terminal 2, terminal canvas item" });
    const note = screen.getByRole("article", { name: "Notes, note canvas item" });
    await user.click(terminal);
    await user.keyboard("{Shift>}");
    await user.click(within(other).getByLabelText("Move Terminal 2"));
    await user.keyboard("{/Shift}");
    expect(terminal).toHaveAttribute("data-selected", "true");
    expect(other).toHaveAttribute("data-selected", "true");
    expect(screen.getByText(/2 selected/)).toBeVisible();
    await user.click(within(terminal).getByRole("region", { name: "Terminal surface for Terminal 1" }));
    expect(screen.getByText(/2 selected/)).toBeVisible();
    await user.click(screen.getByRole("textbox", { name: "Notes content" }));
    expect(note).not.toHaveAttribute("data-selected");
    expect(screen.getByText(/2 selected/)).toBeVisible();
    note.focus();
    await user.keyboard("{Shift>} {/Shift}");
    expect(screen.getByText(/3 selected/)).toBeVisible();
    await user.keyboard("{Shift>}");
    await user.click(other);
    await user.keyboard("{/Shift}");
    expect(other).not.toHaveAttribute("data-selected");
    expect(screen.getByText(/2 selected/)).toBeVisible();
  });

  it("moves the selected group by keyboard and drag while preserving relative spacing", async () => {
    const user = userEvent.setup();
    renderCanvas();
    const terminal = screen.getByRole("article", { name: "Terminal 1, terminal canvas item" });
    const other = screen.getByRole("article", { name: "Terminal 2, terminal canvas item" });
    await user.click(terminal);
    await user.keyboard("{Shift>}");
    await user.click(other);
    await user.keyboard("{/Shift}{ArrowRight}");
    await waitFor(() => {
      expect(readNodePosition("terminal-primary")).toEqual({ x: 178, y: 210 });
      expect(readNodePosition("terminal-secondary")).toEqual({ x: 568, y: 90 });
    });
    const header = within(terminal).getByLabelText("Move Terminal 1");
    fireEvent.pointerDown(header, { button: 0, pointerId: 7, clientX: 100, clientY: 100 });
    fireEvent.pointerMove(header, { pointerId: 7, clientX: 120, clientY: 130 });
    fireEvent.pointerMove(header, { pointerId: 7, clientX: 132, clientY: 140 });
    fireEvent.pointerUp(header, { pointerId: 7 });
    await waitFor(() => {
      expect(readNodePosition("terminal-primary")).toEqual({ x: 210, y: 250 });
      expect(readNodePosition("terminal-secondary")).toEqual({ x: 600, y: 130 });
      expect(readNodePosition("note-first")).toEqual({ x: 600, y: 390 });
    });
    expect(screen.getByText(/2 selected/)).toBeVisible();
  });

  it("selects, duplicates, and removes groups with shortcuts without daemon mutations", async () => {
    const user = userEvent.setup();
    const { props } = renderCanvas();
    const viewport = screen.getByLabelText("Pannable canvas");
    viewport.focus();
    await user.keyboard("{Control>}a{/Control}{Control>}d{/Control}");
    expect(screen.getAllByRole("article")).toHaveLength(6);
    expect(screen.getByText(/3 selected/)).toBeVisible();
    expect(screen.getByRole("article", { name: "Notes copy, note canvas item" })).toHaveFocus();
    await waitFor(() => expect(readCanvasDocument()?.connections).toHaveLength(4));
    await user.keyboard("{Delete}");
    expect(screen.getAllByRole("article")).toHaveLength(3);
    expect(viewport).toHaveFocus();
    expect(props.onCreateCustomAgent).not.toHaveBeenCalled();
    expect(props.onCreateSession).not.toHaveBeenCalled();
    expect(props.onStartSession).not.toHaveBeenCalled();
    expect(props.onStopSession).not.toHaveBeenCalled();
    expect(props.onDeleteSession).not.toHaveBeenCalled();
    expect(props.onRemoveWorktree).not.toHaveBeenCalled();
  });

  it("leaves group shortcuts inside editors, terminal surfaces, and buttons to those controls", async () => {
    const user = userEvent.setup();
    renderCanvas();
    await user.click(screen.getByRole("button", { name: "Select all canvas items" }));
    const note = screen.getByRole("textbox", { name: "Notes content" });
    const terminal = screen.getByRole("region", { name: "Terminal surface for Terminal 1" });
    const button = screen.getByRole("button", { name: "Duplicate selected canvas items" });
    for (const target of [note, terminal, button]) {
      expect(fireEvent.keyDown(target, { key: "d", ctrlKey: true })).toBe(true);
      expect(fireEvent.keyDown(target, { key: "a", metaKey: true })).toBe(true);
      expect(fireEvent.keyDown(target, { key: "Backspace" })).toBe(true);
    }
    expect(screen.getAllByRole("article")).toHaveLength(3);
    expect(screen.getByText(/3 selected/)).toBeVisible();
  });

  it("copies the exact attached agent and starts it only when explicitly requested", async () => {
    const user = userEvent.setup();
    const agent = {
      ...SHELL_AGENT,
      displayName: "Review Codex",
      command: { executable: "/opt/bin/codex", args: ["--model", "review"], env: { CUSTOM_TOKEN: "must-not-persist" } },
    };
    const { props } = renderProjectCanvas({ agents: [agent], sessions: [STOPPED_SESSION], worktrees: [MANAGED_WORKTREE] });
    await user.click(screen.getByRole("article", { name: "Review agent, terminal canvas item" }));
    await user.click(screen.getByRole("button", { name: "Duplicate selected canvas items" }));
    const copied = screen.getByRole("article", { name: "Review agent copy, terminal canvas item" });
    expect(within(copied).getByText("Review Codex draft")).toBeVisible();
    expect(copied).not.toHaveAttribute("data-canvas-session-id");
    expect(props.onCreateSession).not.toHaveBeenCalled();
    expect(props.onStartSession).not.toHaveBeenCalled();
    await waitFor(() => {
      expect(readCanvasDocument()?.nodes).toContainEqual(expect.objectContaining({ title: "Review agent copy", agentId: agent.id }));
      expect(localStorage.getItem(CANVAS_STORAGE_KEY)).not.toContain("must-not-persist");
    });
    await user.click(within(copied).getByRole("button", { name: "Start terminal" }));
    expect(props.onCreateSession).toHaveBeenCalledWith({
      projectId: PROJECT.id,
      name: "Review agent copy",
      agentId: agent.id,
      isolation: "current",
      relativeDirectory: ".worktrees/review",
    });
    expect(props.onCreateCustomAgent).not.toHaveBeenCalled();
    expect(props.onStartSession).toHaveBeenCalledWith(STOPPED_SESSION.id);
  });

  it.each(["missing", "disabled"])("refuses to replace a %s saved agent with a shell", async (availability) => {
    const user = userEvent.setup();
    const { props } = renderProjectCanvas({
      agents: availability === "missing" ? [] : [{ ...SHELL_AGENT, enabled: false }],
      sessions: [STOPPED_SESSION],
    });
    await user.click(screen.getByRole("article", { name: "Review agent, terminal canvas item" }));
    await user.click(screen.getByRole("button", { name: "Duplicate selected canvas items" }));
    const copied = screen.getByRole("article", { name: "Review agent copy, terminal canvas item" });
    await user.click(within(copied).getByRole("button", { name: "Start terminal" }));
    expect(within(copied).getByText(/The original agent is (unavailable|disabled)/)).toBeVisible();
    expect(props.onCreateSession).not.toHaveBeenCalled();
    expect(props.onCreateCustomAgent).not.toHaveBeenCalled();
    expect(props.onStartSession).not.toHaveBeenCalled();
  });

  it("scopes group actions to the current project and prunes selection after switching", async () => {
    const user = userEvent.setup();
    const otherSession = { ...STOPPED_SESSION, id: "other-session", projectId: OTHER_PROJECT.id, name: "Other agent" };
    const { props, rerender } = renderProjectCanvas({ sessions: [STOPPED_SESSION, otherSession], projects: [PROJECT, OTHER_PROJECT] });
    await user.click(screen.getByRole("article", { name: "Review agent, terminal canvas item" }));
    rerender(<CanvasWorkspace {...props} project={OTHER_PROJECT} />);
    expect(screen.queryByText(/1 selected/)).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Select all canvas items" }));
    await user.click(screen.getByRole("button", { name: "Remove selected items from canvas" }));
    expect(screen.queryByRole("article")).not.toBeInTheDocument();
    rerender(<CanvasWorkspace {...props} />);
    expect(screen.getByRole("article", { name: "Review agent, terminal canvas item" })).toBeVisible();
    await waitFor(() => {
      expect(readCanvasDocument()?.hiddenSessionIds).toContain(otherSession.id);
      expect(readCanvasDocument()?.hiddenSessionIds).not.toContain(STOPPED_SESSION.id);
    });
    expect(props.onStopSession).not.toHaveBeenCalled();
    expect(props.onDeleteSession).not.toHaveBeenCalled();
  });

  it("searches metadata, focuses results, and keeps live terminals mounted while filtering", async () => {
    const user = userEvent.setup();
    const { props } = renderProjectCanvas({ sessions: [{ ...STOPPED_SESSION, status: "running" }] });
    const terminal = screen.getByTestId(`live-terminal-${STOPPED_SESSION.id}`);
    const viewport = screen.getByLabelText("Pannable canvas");
    const scrollTo = vi.fn();
    Object.defineProperty(viewport, "scrollTo", { configurable: true, value: scrollTo });
    viewport.focus();
    await user.keyboard("{Meta>}f{/Meta}");
    const search = screen.getByRole("searchbox", { name: "Search canvas items" });
    expect(search).toHaveFocus();
    await user.type(search, "agent/review");
    const panel = screen.getByRole("region", { name: "Canvas items" });
    expect(within(panel).getByText("1 of 4 items")).toBeVisible();
    expect(screen.getByTestId(`live-terminal-${STOPPED_SESSION.id}`)).toBe(terminal);
    await user.keyboard("{ArrowDown}{Enter}");
    expect(screen.queryByRole("region", { name: "Canvas items" })).not.toBeInTheDocument();
    expect(screen.getByRole("article", { name: "Review agent, terminal canvas item" })).toHaveFocus();
    expect(scrollTo).toHaveBeenCalledOnce();
    expect(props.onSelectSession).toHaveBeenCalledWith(STOPPED_SESSION.id);
    await user.click(screen.getByRole("button", { name: "Show canvas items" }));
    await user.type(screen.getByRole("searchbox"), "no-such-item");
    expect(screen.getByText(/No matching items/)).toBeVisible();
    expect(screen.getByTestId(`live-terminal-${STOPPED_SESSION.id}`)).toBe(terminal);
    await user.keyboard("{Escape}");
    expect(screen.getByRole("button", { name: "Show canvas items" })).toHaveFocus();
    expect(props.onStopSession).not.toHaveBeenCalled();
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

  it("preserves dismissed sessions during offline edits and connected reconciliation", async () => {
    localStorage.setItem(CANVAS_STORAGE_KEY, JSON.stringify({
      version: 1,
      nodes: [],
      connections: [],
      zoom: 1,
      hiddenSessionIds: [STOPPED_SESSION.id],
    }));
    const user = userEvent.setup();
    const { props, rerender } = renderCanvas({ isConnected: false });

    await user.click(screen.getByRole("button", { name: "Add note" }));
    await waitFor(() => {
      expect(readCanvasDocument().nodes).toHaveLength(1);
      expect(readCanvasDocument().hiddenSessionIds).toContain(STOPPED_SESSION.id);
    });

    rerender(<CanvasWorkspace
      {...props}
      isConnected
      project={PROJECT}
      projects={[PROJECT]}
      agents={[SHELL_AGENT]}
      sessions={[STOPPED_SESSION]}
    />);

    expect(screen.queryByRole("article", {
      name: "Review agent, terminal canvas item",
    })).not.toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Notes content" })).toBeVisible();
    expect(readCanvasDocument().hiddenSessionIds).toContain(STOPPED_SESSION.id);
    expect(props.onDeleteSession).not.toHaveBeenCalled();
    expect(props.onStopSession).not.toHaveBeenCalled();
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
  const props = createProjectCanvasProps(overrides);
  return { ...render(<CanvasWorkspace {...props} />), props };
}

function createProjectCanvasProps(
  overrides: Partial<ComponentProps<typeof CanvasWorkspace>> = {},
): ComponentProps<typeof CanvasWorkspace> {
  return {
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
}

function readCanvasDocument(): CanvasDocument {
  const document = parseCanvasDocument(localStorage.getItem(CANVAS_STORAGE_KEY));
  if (!document) throw new Error("Expected a persisted canvas document.");
  return document;
}

function readPromptDraft(nodeId: string): string {
  const node = readCanvasDocument().nodes.find((candidate) => candidate.id === nodeId);
  if (node?.kind !== "terminal") throw new Error(`Expected terminal ${nodeId}.`);
  return node.promptDraft ?? "";
}

function deferredPromptDelivery() {
  let complete: () => void = () => {};
  const promise = new Promise<void>((resolve) => { complete = resolve; });
  return { promise, complete };
}

function connectionEndpointX(container: HTMLElement): number {
  const path = container.querySelector("[data-connection-id] path");
  const coordinates = path?.getAttribute("d")?.match(/-?\d+(?:\.\d+)?/g);
  if (!coordinates || coordinates.length < 2) {
    throw new Error("Expected a rendered canvas connection path.");
  }
  return Number(coordinates[coordinates.length - 2]);
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
    hiddenSessionIds: [],
  };
  localStorage.setItem(CANVAS_STORAGE_KEY, JSON.stringify(document));
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
