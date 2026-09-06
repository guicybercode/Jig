import { describe, expect, it } from "vitest";

import {
  canvasReducer,
  createCanvasNode,
  createInitialCanvasDocument,
  createInitialCanvasState,
  createSessionTerminalCanvasNode,
  createTerminalCanvasNode,
  duplicateCanvasSelection,
  parseCanvasDocument,
  serializeCanvasDocument,
} from "./canvas-state";

describe("canvas state", () => {
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
      version: 1,
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
      version: 1, nodes: [terminal, note, outside], zoom: 1,
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
      version: 1, nodes: [terminal, note, otherProject], zoom: 1,
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
      version: 1,
      nodes: [terminal],
      connections: [],
      zoom: 1,
    });
    const resized = canvasReducer(initial, {
      type: "terminal/resize",
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
      version: 1,
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
      version: 1,
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
      version: 1,
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
      version: 1,
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
        version: 1,
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
