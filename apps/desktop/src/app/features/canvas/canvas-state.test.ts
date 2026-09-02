import { describe, expect, it } from "vitest";

import {
  canvasReducer,
  createBrowserCanvasNode,
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
      "https://example.com/search?q=tauri&access_token=secret&X-Amz-Signature=signed#callback",
      "browser-redacted",
    );

    expect(browser.url).toBe("https://example.com/search?q=tauri");
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
            x: 100,
            y: 200,
          },
        ],
        connections: [],
        zoom: 0.75,
      }),
    );

    expect(parsed).toMatchObject({
      version: 2,
      nodes: [{ id: "note-legacy", text: "Keep me" }],
      zoom: 0.75,
    });
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
