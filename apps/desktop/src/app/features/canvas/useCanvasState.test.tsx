import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  CANVAS_DOCUMENT_UPDATED_EVENT,
  CANVAS_STORAGE_KEY,
  createBrowserCanvasNode,
} from "./canvas-state";
import { useCanvasState } from "./useCanvasState";

describe("useCanvasState", () => {
  beforeEach(() => {
    localStorage.clear();
  });
  afterEach(() => vi.restoreAllMocks());

  it("redacts browser URLs in both storage and published documents", () => {
    const publish = vi.spyOn(globalThis, "dispatchEvent");
    const { result } = renderHook(() => useCanvasState());
    const browser = {
      ...createBrowserCanvasNode({ x: 20, y: 30 }, "", "browser"),
      projectId: "project",
      url: "https://example.com/?tab=review&token=private#secret",
    };
    act(() => result.current.dispatch({
      type: "document/hydrate",
      document: { version: 2, nodes: [browser], connections: [], zoom: 1, hiddenSessionIds: ["hidden"] },
    }));
    const persisted = JSON.parse(localStorage.getItem(CANVAS_STORAGE_KEY) ?? "{}");
    expect(persisted.nodes[0]).toMatchObject({ url: "https://example.com/?tab=review", projectId: "project" });
    expect(persisted.hiddenSessionIds).toEqual(["hidden"]);
    expect(publish).toHaveBeenLastCalledWith(expect.objectContaining({
      detail: expect.objectContaining({ nodes: persisted.nodes }),
    }));
    expect(JSON.stringify(persisted)).not.toMatch(/private|secret/);
  });

  it("does not save or publish another document for transient selection changes", () => {
    const write = spyOnStorageWrites();
    const publish = vi.spyOn(globalThis, "dispatchEvent");
    const { result } = renderHook(() => useCanvasState());
    write.mockClear();
    publish.mockClear();

    act(() => result.current.dispatch({
      type: "nodes/select", nodeIds: ["terminal-primary", "note-first"],
    }));
    act(() => result.current.dispatch({ type: "connection/start", nodeId: "terminal-primary" }));
    act(() => result.current.dispatch({ type: "connection/cancel" }));

    expect(write).not.toHaveBeenCalled();
    expect(publish).not.toHaveBeenCalled();
    act(() => result.current.dispatch({ type: "note/update", nodeId: "note-first", text: "Changed" }));
    expect(write).toHaveBeenCalledTimes(1);
    expect(publish).toHaveBeenCalledTimes(1);
  });

  it("reports failed saves while retaining edits and recovers on the next successful save", () => {
    const write = spyOnStorageWrites().mockImplementation(() => {
      throw new DOMException("Storage full", "QuotaExceededError");
    });
    const { result } = renderHook(() => useCanvasState());
    expect(result.current.persistenceAvailable).toBe(false);
    act(() => result.current.dispatch({ type: "note/update", nodeId: "note-first", text: "Keep my draft" }));
    expect(result.current.state.nodes.find((node) => node.id === "note-first")).toMatchObject({ text: "Keep my draft" });
    expect(result.current.persistenceAvailable).toBe(false);

    write.mockRestore();
    act(() => result.current.dispatch({ type: "zoom/set", zoom: 1.2 }));
    expect(result.current.persistenceAvailable).toBe(true);
    expect(localStorage.getItem(CANVAS_STORAGE_KEY)).toContain("Keep my draft");
  });

  it("hydrates the first-launch graph and persists durable mutations", () => {
    const onDocumentUpdated = vi.fn();
    window.addEventListener(CANVAS_DOCUMENT_UPDATED_EVENT, onDocumentUpdated);
    const { result } = renderHook(() => useCanvasState());

    expect(result.current.state.nodes).toHaveLength(3);
    expect(result.current.persistenceAvailable).toBe(true);

    act(() => {
      result.current.dispatch({
        type: "node/move",
        nodeId: "terminal-primary",
        position: { x: 720, y: 400 },
      });
      result.current.dispatch({
        type: "node/select",
        nodeId: "terminal-primary",
      });
    });

    const persisted = JSON.parse(
      localStorage.getItem(CANVAS_STORAGE_KEY) ?? "{}",
    ) as Record<string, unknown>;
    expect(persisted).not.toHaveProperty("selectedNodeId");
    expect(persisted).not.toHaveProperty("connectionSourceId");
    expect(persisted.nodes).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: "terminal-primary",
          x: 720,
          y: 400,
        }),
      ]),
    );
    expect(onDocumentUpdated).toHaveBeenLastCalledWith(
      expect.objectContaining({
        detail: expect.objectContaining({ nodes: result.current.state.nodes }),
      }),
    );
    window.removeEventListener(CANVAS_DOCUMENT_UPDATED_EVENT, onDocumentUpdated);
  });

  it("falls back to the safe first-launch graph for corrupt storage", () => {
    localStorage.setItem(CANVAS_STORAGE_KEY, "{not-json");

    const { result } = renderHook(() => useCanvasState());

    expect(result.current.state.nodes.map((node) => node.id)).toEqual([
      "terminal-primary",
      "terminal-secondary",
      "note-first",
    ]);
  });
});

function spyOnStorageWrites() {
  // Node 25's fallback owns its methods; jsdom Storage exposes them on its prototype.
  const owner = Object.prototype.hasOwnProperty.call(localStorage, "setItem")
    ? localStorage
    : Storage.prototype;
  return vi.spyOn(owner, "setItem");
}
