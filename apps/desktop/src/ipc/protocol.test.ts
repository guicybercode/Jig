import { describe, expect, it } from "vitest";

import catalog from "../../../../protocol/catalog.json";
import desktopPackage from "../../package.json";
import {
  IPC_EVENTS,
  IPC_METHODS,
  SESSION_STATUSES,
  isIpcEvent,
  isIpcMethod,
  parseAgentId,
  parseProjectId,
  parseWorktreeId,
} from "./index";
import type { WorktreePrepareRemoveResponse } from "./index";

const UUID_V7 = "01900000-0000-7000-8000-000000000001";

describe("authoritative Beta IPC mirror", () => {
  it("shares every method and event name with the Rust catalog mirror", () => {
    expect([...IPC_METHODS]).toEqual(catalog.methods);
    expect([...IPC_EVENTS]).toEqual(catalog.events);
    expect(catalog.protocolVersion).toBe(1);
    expect(catalog.applicationVersion).toBe(desktopPackage.version);

    for (const method of catalog.methods) {
      expect(isIpcMethod(method)).toBe(true);
    }
    for (const event of catalog.events) {
      expect(isIpcEvent(event)).toBe(true);
    }

    expect(isIpcMethod("worktree.create")).toBe(false);
    expect(isIpcEvent("daemon.status_changed")).toBe(false);
    expect(isIpcMethod("worktree.prepare_remove")).toBe(true);
    expect(isIpcEvent("session.output_gap")).toBe(true);
  });

  it("treats every entity identifier, including agents, as a UUID", () => {
    expect(parseAgentId(UUID_V7)).toBe(UUID_V7);
    expect(parseProjectId(UUID_V7)).toBe(UUID_V7);
    expect(() => parseAgentId("codex")).toThrow(/UUID/);
    expect(() => parseProjectId("project-1")).toThrow(/UUID/);
  });

  it("mirrors the frozen public session status values", () => {
    expect(SESSION_STATUSES).toEqual([
      "starting",
      "running",
      "idle",
      "exited",
      "failed",
      "unknown",
    ]);
  });

  it("keeps success and error envelopes mutually exclusive", () => {
    const envelope = {
      kind: "response",
      version: 1,
      requestId: UUID_V7,
      status: "error",
      error: {
        code: "executable_not_found",
        message: "Could not start the selected agent",
        action: "Install the executable or update the custom agent",
      },
    } as const;

    expect(envelope.status).toBe("error");
    expect(envelope.error.code).toBe("executable_not_found");
    expect("data" in envelope).toBe(false);
  });

  it("cannot attach a removal token to a blocked worktree result", () => {
    const worktreeId = parseWorktreeId(UUID_V7);
    const blocked: WorktreePrepareRemoveResponse = {
      status: "blocked",
      worktreeId,
      isDirty: true,
      blockers: ["tracked_changes"],
    };
    const ready: WorktreePrepareRemoveResponse = {
      status: "ready",
      worktreeId,
      confirmationToken: "state-bound-token",
      expiresAtMs: 1_800_000_000_000,
    };

    expect("confirmationToken" in blocked).toBe(false);
    expect("confirmationToken" in ready).toBe(true);
  });
});
