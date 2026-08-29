import { describe, expect, it } from "vitest";

import catalog from "../../../../protocol/catalog.json";
import {
  canTransitionSessionStatus,
  IPC_EVENTS,
  IPC_METHODS,
  isIpcEvent,
  isIpcMethod,
  isLiveSessionStatus,
  parseAgentId,
  parseProjectId,
  recoveredSessionStatus,
  SESSION_STATUSES,
} from "./index";

describe("ipc contracts", () => {
  it("shares method and event names with the Rust catalog", () => {
    expect([...IPC_METHODS]).toEqual(catalog.methods);
    expect([...IPC_EVENTS]).toEqual(catalog.events);
    expect(catalog.protocolVersion).toBe(1);
    for (const method of catalog.methods) {
      expect(isIpcMethod(method)).toBe(true);
    }
    for (const event of catalog.events) {
      expect(isIpcEvent(event)).toBe(true);
    }
    expect(isIpcMethod("session.explode")).toBe(false);
    expect(isIpcEvent("session.output_gap")).toBe(false);
  });

  it("accepts built-in agent keys and rejects shell-like ids", () => {
    expect(parseAgentId("codex")).toBe("codex");
    expect(() => parseAgentId("codex cli")).toThrow(/ASCII/);
    expect(() => parseAgentId("")).toThrow(/empty/);
  });

  it("rejects non-UUID project ids", () => {
    expect(() => parseProjectId("project-1")).toThrow(/UUID/);
    expect(
      parseProjectId("01900000-0000-7000-8000-000000000001"),
    ).toBe("01900000-0000-7000-8000-000000000001");
  });

  it("mirrors the Rust session state machine", () => {
    expect(SESSION_STATUSES).toEqual([
      "created",
      "starting",
      "running",
      "idle",
      "stopping",
      "exited",
      "failed",
      "unknown",
    ]);
    expect(canTransitionSessionStatus("created", "starting")).toBe(true);
    expect(canTransitionSessionStatus("created", "running")).toBe(false);
    expect(canTransitionSessionStatus("running", "starting")).toBe(false);
    expect(canTransitionSessionStatus("exited", "starting")).toBe(true);
    expect(isLiveSessionStatus("running")).toBe(true);
    expect(isLiveSessionStatus("created")).toBe(false);
    expect(recoveredSessionStatus("running")).toBe("unknown");
    expect(recoveredSessionStatus("exited")).toBe("exited");
  });

  it("parses a version 1 error envelope without treating it as success", () => {
    const envelope = {
      kind: "response",
      version: 1,
      requestId: "01900000-0000-7000-8000-000000000002",
      status: "error",
      error: {
        code: "AGENT_EXECUTABLE_NOT_FOUND",
        message: "Could not start Codex",
        action: "Install Codex or configure a custom executable",
        details: { executable: "codex" },
      },
    } as const;

    expect(envelope.status).toBe("error");
    expect(envelope.error.code).toBe("AGENT_EXECUTABLE_NOT_FOUND");
  });
});
