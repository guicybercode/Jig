import { describe, expect, it } from "vitest";

import {
  canvasReducer,
  createBrowserCanvasNode,
  createCanvasNode,
  createInitialCanvasDocument,
  createInitialCanvasState,
  createSessionTerminalCanvasNode,
  createTerminalCanvasNode,
  duplicateCanvasSelection,
  normalizeBrowserNavigationUrl,
  normalizeBrowserUrl,
  parseCanvasDocument,
  serializeCanvasDocument,
} from "./canvas-state";

describe("canvas state", () => {
  it("persists the Gemini quick-start preset with its native executable", () => {
    const node = createTerminalCanvasNode({ x: 0, y: 0 }, { preset: "gemini" }, "gemini");
    const state = createInitialCanvasState({ version: 2, nodes: [node], connections: [], zoom: 1 });
    expect(parseCanvasDocument(serializeCanvasDocument(state)).nodes[0]).toMatchObject({
      preset: "gemini", title: "Gemini", executable: "gemini",
    });
  });

  it("toggles multi-selection, filters unknown IDs, and keeps selection transient", () => {
    const initial = createInitialCanvasState();
    const first = canvasReducer(initial, { type: "node/select", nodeId: "note-first" });
    const multiple = canvasReducer(first, {
      type: "node/select", nodeId: "terminal-primary", additive: true,
    });
    expect(multiple.selectedNodeIds).toEqual(["note-first", "terminal-primary"]);
    expect(multiple.selectedNodeId).toBe("terminal-primary");
    const toggled = canvasReducer(multiple, {
      type: "node/select", nodeId: "terminal-primary", additive: true,
    });
    expect(toggled.selectedNodeIds).toEqual(["note-first"]);
    expect(toggled.selectedNodeId).toBe("note-first");
    const selected = canvasReducer(toggled, {
      type: "nodes/select", nodeIds: ["terminal-primary", "missing", "terminal-primary", "note-first"],
    });
    expect(selected.selectedNodeIds).toEqual(["terminal-primary", "note-first"]);
    expect(JSON.parse(serializeCanvasDocument(selected))).not.toHaveProperty("selectedNodeIds");
    expect(createInitialCanvasState(parseCanvasDocument(serializeCanvasDocument(selected))).selectedNodeIds).toEqual([]);
    expect(canvasReducer(selected, { type: "node/select", nodeId: null }).selectedNodeIds).toEqual([]);
  });

  it("moves a group by one bounded delta without distorting its layout", () => {
    const initial = createInitialCanvasState({
      version: 2,
      nodes: [
        createCanvasNode("note", { x: 7_990, y: -1_990 }, "edge"),
        createCanvasNode("note", { x: 7_500, y: -1_500 }, "neighbor"),
        createCanvasNode("note", { x: 0, y: 0 }, "unselected"),
      ],
      connections: [], zoom: 1,
    });
    const moved = canvasReducer(initial, {
      type: "nodes/move", nodeIds: ["edge", "neighbor"], delta: { x: 80, y: -80 },
    });
    expect(moved.nodes[0]).toMatchObject({ x: 8_000, y: -2_000 });
    expect(moved.nodes[1]).toMatchObject({ x: 7_510, y: -1_510 });
    expect(moved.nodes[2]).toBe(initial.nodes[2]);
    expect(canvasReducer(moved, {
      type: "nodes/move", nodeIds: ["edge"], delta: { x: NaN, y: 3 },
    })).toBe(moved);
  });

  it("duplicates notes, agent references, and internal connections without live sessions", () => {
    const terminal = createSessionTerminalCanvasNode({ x: 0, y: 0 }, {
      id: "live-session", agentId: "custom-agent", projectId: "project-one", name: "Review", cwd: "/repo",
    });
    const note = { ...createCanvasNode("note", { x: 400, y: 0 }, "note"), title: "Release", text: "Ship it", projectId: "project-one" };
    const outside = createCanvasNode("note", { x: 0, y: 400 }, "outside");
    const initial = createInitialCanvasState({
      version: 2, nodes: [terminal, note, outside], zoom: 1,
      connections: [
        { id: "internal", sourceNodeId: terminal.id, targetNodeId: note.id },
        { id: "external", sourceNodeId: terminal.id, targetNodeId: outside.id },
      ],
    });
    const action = duplicateCanvasSelection(initial, [terminal.id, note.id]);
    const copied = canvasReducer(initial, action);
    expect(copied.nodes).toHaveLength(5);
    const copies = copied.nodes.slice(3);
    expect(copies[0]).toMatchObject({ kind: "terminal", title: "Review copy", agentId: "custom-agent", projectId: "project-one", x: 32, y: 32 });
    expect(copies[0]).not.toHaveProperty("sessionId", "live-session");
    expect(copies[1]).toMatchObject({ kind: "note", title: "Release copy", text: "Ship it", x: 432, y: 32 });
    expect(copied.connections).toHaveLength(3);
    expect(copied.connections[2]).toMatchObject({ sourceNodeId: copies[0]?.id, targetNodeId: copies[1]?.id });
    expect(copied.selectedNodeIds).toEqual(copies.map((node) => node.id));
    expect(copied.nodes[0]).toBe(terminal);
    expect(canvasReducer(copied, action)).toBe(copied);
    expect(parseCanvasDocument(serializeCanvasDocument(copied)).nodes[3]).toMatchObject({ agentId: "custom-agent" });
  });

  it("removes a selected group atomically, hiding its sessions and retaining other projects", () => {
    const terminal = createSessionTerminalCanvasNode({ x: 0, y: 0 }, {
      id: "live", projectId: "project-one", name: "Agent", cwd: "/repo",
    });
    const note = createCanvasNode("note", { x: 400, y: 0 }, "note");
    const otherProject = { ...createCanvasNode("note", { x: 0, y: 0 }, "other"), projectId: "project-two" };
    const initial = createInitialCanvasState({
      version: 2, nodes: [terminal, note, otherProject], zoom: 1,
      connections: [{ id: "edge", sourceNodeId: terminal.id, targetNodeId: note.id }],
    });
    const selected = canvasReducer(initial, { type: "nodes/select", nodeIds: [terminal.id, note.id] });
    const deleted = canvasReducer(selected, { type: "nodes/delete", nodeIds: selected.selectedNodeIds });
    expect(deleted.nodes).toEqual([otherProject]);
    expect(deleted.connections).toEqual([]);
    expect(deleted.hiddenSessionIds).toEqual(["live"]);
    expect(deleted.selectedNodeIds).toEqual([]);
    expect(deleted.selectedNodeId).toBeNull();
    expect(canvasReducer(deleted, {
      type: "sessions/reconcile", knownSessionIds: ["live"], sessionNodes: [terminal],
    }).nodes).toEqual([otherProject]);
  });

  it("provides the reference terminal and note composition on first launch", () => {
    const document = createInitialCanvasDocument();

    expect(document.nodes.map((node) => node.kind)).toEqual([
      "terminal",
      "terminal",
      "note",
    ]);
    expect(document.connections).toHaveLength(2);
    expect(document.zoom).toBe(1);
    expect(document.nodes[0]).toMatchObject({
      preset: "shell",
      width: 432,
      height: 256,
    });
  });

  it("persists terminal presets and clamps terminal resizing", () => {
    const terminal = createTerminalCanvasNode(
      { x: 40, y: 50 },
      { title: "Pairing", preset: "codex", workingDirectory: "~/project" },
      "terminal-codex",
    );
    const initial = createInitialCanvasState({
      version: 2,
      nodes: [terminal],
      connections: [],
      zoom: 1,
    });
    const resized = canvasReducer(initial, {
      type: "node/resize",
      nodeId: terminal.id,
      size: { width: 2_000, height: 100 },
    });
    const configured = canvasReducer(resized, {
      type: "terminal/configure",
      nodeId: terminal.id,
      configuration: {
        title: "Review",
        preset: "claude",
        workingDirectory: "~/review",
      },
    });

    expect(configured.nodes[0]).toMatchObject({
      title: "Review",
      preset: "claude",
      executable: "claude",
      workingDirectory: "~/review",
      width: 960,
      height: 192,
    });
    expect(parseCanvasDocument(serializeCanvasDocument(configured))).toEqual(
      expect.objectContaining({ nodes: configured.nodes }),
    );
  });

  it("normalizes, persists, and resizes HTTP browser nodes", () => {
    const browser = createBrowserCanvasNode(
      { x: 24, y: 32 },
      "example.com/docs",
      "browser-docs",
    );
    const initial = createInitialCanvasState({
      version: 2,
      nodes: [browser],
      connections: [],
      zoom: 1,
    });
    const navigated = canvasReducer(initial, {
      type: "browser/navigate",
      nodeId: browser.id,
      url: "http://localhost:4173/preview",
    });
    const resized = canvasReducer(navigated, {
      type: "node/resize",
      nodeId: browser.id,
      size: { width: 4_000, height: 20 },
    });

    expect(browser).toMatchObject({
      kind: "browser",
      url: "https://example.com/docs",
      width: 640,
      height: 420,
    });
    expect(resized.nodes[0]).toMatchObject({
      url: "http://localhost:4173/preview",
      width: 1_280,
      height: 320,
    });
    expect(parseCanvasDocument(serializeCanvasDocument(resized))).toEqual(
      expect.objectContaining({ version: 2, nodes: resized.nodes }),
    );
  });

  it("rejects unsafe browser addresses without replacing a safe address", () => {
    const browser = createBrowserCanvasNode(
      { x: 0, y: 0 },
      "https://example.com/",
      "browser-safe",
    );
    const initial = createInitialCanvasState({
      version: 2,
      nodes: [browser],
      connections: [],
      zoom: 1,
    });
    const navigated = canvasReducer(initial, {
      type: "browser/navigate",
      nodeId: browser.id,
      url: "https://user:secret@example.com/private",
    });
    const parsed = parseCanvasDocument(
      JSON.stringify({
        version: 2,
        nodes: [
          {
            ...browser,
            url: "file:///etc/passwd",
          },
        ],
        connections: [],
        zoom: 1,
      }),
    );

    expect(navigated.nodes[0]).toBe(browser);
    expect(parsed.nodes[0]).toMatchObject({ url: "" });
  });

  it("removes fragments and secret-bearing query parameters before persistence", () => {
    const browser = createBrowserCanvasNode(
      { x: 0, y: 0 },
      "https://example.com/search?q=tauri&access_token=secret&oauth_token=oauth&client_secret=client&credentials=credential&pwd=password&X-Amz-Signature=signed#callback",
      "browser-redacted",
    );

    expect(browser.url).toBe("https://example.com/search?q=tauri");
  });

  it("keeps secret-bearing parameters only for transient browser navigation", () => {
    const address =
      "https://example.com/callback?tab=activity&access_token=secret#complete";

    expect(normalizeBrowserNavigationUrl(address)).toBe(address);
    expect(normalizeBrowserUrl(address)).toBe(
      "https://example.com/callback?tab=activity",
    );
  });

  it("migrates version 1 canvas documents without resetting the layout", () => {
    const parsed = parseCanvasDocument(
      JSON.stringify({
        version: 1,
        nodes: [
          {
            id: "note-legacy",
            kind: "note",
            title: "Legacy note",
            text: "Keep me",
            projectId: "legacy-project",
            x: 100,
            y: 200,
          },
        ],
        connections: [],
        zoom: 0.75,
        hiddenSessionIds: ["hidden-session"],
      }),
    );

    expect(parsed).toMatchObject({
      version: 2,
      nodes: [{ id: "note-legacy", text: "Keep me", projectId: "legacy-project" }],
      zoom: 0.75,
      hiddenSessionIds: ["hidden-session"],
    });
  });

  it("copies a browser and agent together while retaining scope and clearing the live session", () => {
    const browser = { ...createBrowserCanvasNode({ x: 100, y: 200 }, "https://example.com/?token=secret&tab=code#private", "browser"), projectId: "project" };
    const terminal = createSessionTerminalCanvasNode({ x: 800, y: 200 }, {
      id: "session", agentId: "gemini-agent", projectId: "project", name: "Gemini", cwd: "/repo",
    });
    const state = createInitialCanvasState({
      version: 2, nodes: [browser, terminal], zoom: 1,
      connections: [{ id: "edge", sourceNodeId: browser.id, targetNodeId: terminal.id }],
      hiddenSessionIds: ["other-session"],
    });
    const copied = canvasReducer(state, duplicateCanvasSelection(state, [browser.id, terminal.id]));
    expect(copied.nodes[2]).toMatchObject({ kind: "browser", projectId: "project", url: "https://example.com/?tab=code", width: browser.width, height: browser.height });
    expect(copied.nodes[3]).toMatchObject({ kind: "terminal", agentId: "gemini-agent", sessionId: undefined, projectId: "project" });
    const restored = parseCanvasDocument(serializeCanvasDocument(copied));
    expect(restored.nodes).toEqual(copied.nodes);
    expect(restored.hiddenSessionIds).toEqual(["other-session"]);
    expect(restored.connections).toHaveLength(2);
  });

  it("moves, renames, and updates notes without changing other nodes", () => {
    const initial = createInitialCanvasState();
    const moved = canvasReducer(initial, {
      type: "node/move",
      nodeId: "note-first",
      position: { x: 512, y: 640 },
    });
    const renamed = canvasReducer(moved, {
      type: "node/rename",
      nodeId: "note-first",
      title: "  Release checklist  ",
    });
    const updated = canvasReducer(renamed, {
      type: "note/update",
      nodeId: "note-first",
      text: "Run the smoke test",
    });

    expect(updated.nodes.find((node) => node.id === "note-first")).toEqual({
      id: "note-first",
      kind: "note",
      title: "Release checklist",
      text: "Run the smoke test",
      x: 512,
      y: 640,
    });
    expect(updated.nodes.find((node) => node.id === "terminal-primary")).toBe(
      initial.nodes.find((node) => node.id === "terminal-primary"),
    );
  });

  it("clears the selected node when the canvas background is selected", () => {
    const initial = createInitialCanvasState();
    const selected = canvasReducer(initial, {
      type: "node/select",
      nodeId: "note-first",
    });
    const cleared = canvasReducer(selected, {
      type: "node/select",
      nodeId: null,
    });

    expect(selected.selectedNodeId).toBe("note-first");
    expect(cleared.selectedNodeId).toBeNull();
  });

  it("connects distinct nodes once and removes their edges with the node", () => {
    const initial = createInitialCanvasState({
      version: 2,
      nodes: [
        createCanvasNode("terminal", { x: 0, y: 0 }, "terminal-a"),
        createCanvasNode("note", { x: 100, y: 100 }, "note-b"),
      ],
      connections: [],
      zoom: 1,
    });
    const connecting = canvasReducer(initial, {
      type: "connection/start",
      nodeId: "terminal-a",
    });
    const connected = canvasReducer(connecting, {
      type: "connection/complete",
      targetNodeId: "note-b",
    });
    const duplicateAttempt = canvasReducer(
      canvasReducer(connected, {
        type: "connection/start",
        nodeId: "note-b",
      }),
      { type: "connection/complete", targetNodeId: "terminal-a" },
    );

    expect(duplicateAttempt.connections).toHaveLength(1);
    expect(duplicateAttempt.connectionSourceId).toBeNull();

    const disconnected = canvasReducer(duplicateAttempt, {
      type: "connection/delete",
      connectionId: duplicateAttempt.connections[0]?.id ?? "missing",
    });
    expect(disconnected.connections).toEqual([]);

    const deleted = canvasReducer(duplicateAttempt, {
      type: "node/delete",
      nodeId: "note-b",
    });
    expect(deleted.nodes).toHaveLength(1);
    expect(deleted.connections).toEqual([]);
  });

  it("dismisses attached cards without deleting or immediately recreating sessions", () => {
    const sessionNode = createSessionTerminalCanvasNode(
      { x: 20, y: 30 },
      {
        id: "session-one",
        projectId: "project-one",
        name: "Agent session",
        cwd: "/repos/project-one",
      },
    );
    const initial = createInitialCanvasState({
      version: 2,
      nodes: [sessionNode],
      connections: [],
      zoom: 1,
      hiddenSessionIds: [],
    });

    const dismissed = canvasReducer(initial, {
      type: "node/delete",
      nodeId: sessionNode.id,
    });
    const reconciled = canvasReducer(dismissed, {
      type: "sessions/reconcile",
      knownSessionIds: ["session-one"],
      sessionNodes: [sessionNode],
    });

    expect(reconciled.nodes).toEqual([]);
    expect(reconciled.hiddenSessionIds).toEqual(["session-one"]);
    expect(parseCanvasDocument(serializeCanvasDocument(reconciled))).toEqual(
      expect.objectContaining({ hiddenSessionIds: ["session-one"] }),
    );
  });

  it("reconciles known sessions, prunes stale dismissals, and refreshes names", () => {
    const firstNode = createSessionTerminalCanvasNode(
      { x: 20, y: 30 },
      {
        id: "session-one",
        projectId: "project-one",
        name: "Old name",
        cwd: "/repos/project-one",
      },
    );
    const secondNode = createSessionTerminalCanvasNode(
      { x: 200, y: 300 },
      {
        id: "session-two",
        projectId: "project-one",
        name: "Hidden session",
        cwd: "/repos/project-one",
      },
    );
    const initial = createInitialCanvasState({
      version: 2,
      nodes: [firstNode],
      connections: [],
      zoom: 1,
      hiddenSessionIds: ["session-two", "deleted-session"],
    });
    const renamedFirstNode = createSessionTerminalCanvasNode(
      { x: 900, y: 900 },
      {
        id: "session-one",
        projectId: "project-one",
        name: "Renamed session",
        cwd: "/repos/project-one",
      },
    );

    const reconciled = canvasReducer(initial, {
      type: "sessions/reconcile",
      knownSessionIds: ["session-one", "session-two"],
      sessionNodes: [renamedFirstNode, secondNode],
    });

    expect(reconciled.nodes).toHaveLength(1);
    expect(reconciled.nodes[0]).toMatchObject({
      id: firstNode.id,
      sessionId: "session-one",
      projectId: "project-one",
      title: "Renamed session",
      x: 20,
      y: 30,
    });
    expect(reconciled.hiddenSessionIds).toEqual(["session-two"]);
  });

  it("reveals a dismissed session atomically and selects its terminal node", () => {
    const sessionNode = createSessionTerminalCanvasNode(
      { x: 20, y: 30 },
      {
        id: "session-one",
        projectId: "project-one",
        name: "Agent session",
        cwd: "/repos/project-one",
      },
    );
    const initial = createInitialCanvasState({
      version: 2,
      nodes: [],
      connections: [],
      zoom: 1,
      hiddenSessionIds: ["session-one"],
    });

    const revealed = canvasReducer(initial, {
      type: "session/reveal",
      node: sessionNode,
    });

    expect(revealed.nodes).toEqual([sessionNode]);
    expect(revealed.hiddenSessionIds).toEqual([]);
    expect(revealed.selectedNodeId).toBe(sessionNode.id);
  });

  it("round-trips valid documents and drops unsafe persisted references", () => {
    const serialized = serializeCanvasDocument(createInitialCanvasState());
    expect(parseCanvasDocument(serialized)).toEqual(
      createInitialCanvasDocument(),
    );

    const parsed = parseCanvasDocument(
      JSON.stringify({
        version: 2,
        zoom: 9,
        nodes: [
          {
            id: "note-safe",
            kind: "note",
            title: "  Safe note  ",
            text: "hello",
            x: Number.POSITIVE_INFINITY,
            y: -9_999,
          },
          { id: 42, kind: "terminal" },
        ],
        connections: [
          {
            id: "missing-target",
            sourceNodeId: "note-safe",
            targetNodeId: "missing",
          },
        ],
      }),
    );

    expect(parsed.nodes).toEqual([
      {
        id: "note-safe",
        kind: "note",
        title: "Safe note",
        text: "hello",
        x: 0,
        y: -2_000,
      },
    ]);
    expect(parsed.connections).toEqual([]);
    expect(parsed.zoom).toBe(1.5);
    expect(parsed.hiddenSessionIds).toEqual([]);
  });
});
