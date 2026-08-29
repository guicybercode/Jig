import { describe, expect, it } from "vitest";

import {
  canvasReducer,
  createCanvasNode,
  createInitialCanvasDocument,
  createInitialCanvasState,
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
  });
});
