import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";

import { CANVAS_STORAGE_KEY } from "./canvas-state";
import { useCanvasState } from "./useCanvasState";

describe("useCanvasState", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("hydrates the first-launch graph and persists durable mutations", () => {
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
