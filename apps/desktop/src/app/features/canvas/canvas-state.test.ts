import { describe, expect, it } from "vitest";

import {
  canvasReducer,
  createCanvasNode,
  createInitialCanvasDocument,
  createInitialCanvasState,
  createSessionTerminalCanvasNode,
  createTerminalCanvasNode,
  parseCanvasDocument,
  serializeCanvasDocument,
} from "./canvas-state";

describe("canvas state", () => {
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
