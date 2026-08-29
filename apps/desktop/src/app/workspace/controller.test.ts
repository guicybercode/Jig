import { afterEach, describe, expect, it } from "vitest";
import { waitFor } from "@testing-library/react";

import {
  createMockIpcClient,
  disconnectedError,
  rejectWith,
  type MockIpcClient,
} from "../../ipc";
import { createWorkspaceController } from "./controller";
import { createWorkspaceStore } from "./store";
import { createTerminalRegistry } from "./terminal-registry";
import { createInitialWorkspaceState } from "./types";
import {
  AGENT_ID,
  PROJECT_ID,
  SESSION_ID,
  agentFixture,
  helloFixture,
  projectFixture,
  sessionFixture,
  snapshotFixture,
} from "../../test/ipc-fixtures";
import type { TerminalSurfaceHandle } from "../features/terminal";

function connectedClient(
  snapshot = snapshotFixture(),
): MockIpcClient {
  return createMockIpcClient({
    "system.hello": () => helloFixture(),
    "state.snapshot": () => snapshot,
    "agent.detect": () => ({ detections: [] }),
  });
}

function startRuntime(client: MockIpcClient) {
  const store = createWorkspaceStore(createInitialWorkspaceState("loading"));
  const terminals = createTerminalRegistry();
  const actions = createWorkspaceController({ client, store, terminals });
  const stop = actions.start();
  return { store, actions, terminals, stop };
}

async function waitForPhase(
  store: ReturnType<typeof createWorkspaceStore>,
  phase: string,
): Promise<void> {
  await waitFor(() => {
    expect(store.getState().connection.phase).toBe(phase);
  });
}

describe("workspace controller", () => {
  const cleanups: Array<() => void> = [];

  afterEach(() => {
    while (cleanups.length > 0) {
      cleanups.pop()?.();
    }
  });

  it("ignores an older snapshot that resolves after a newer refresh", async () => {
    const client = connectedClient();
    const runtime = startRuntime(client);
    cleanups.push(runtime.stop);
    await waitForPhase(runtime.store, "ready");

    client.stall();
    client.setHandler("state.snapshot", () =>
      snapshotFixture({
        projects: [projectFixture({ name: "Stale" })],
      }),
    );
    const stale = runtime.actions.refresh();
    client.setHandler("state.snapshot", () =>
      snapshotFixture({
        projects: [projectFixture({ name: "Latest" })],
      }),
    );
    const latest = runtime.actions.refresh();

    await client.flushLast();
    await latest;
    await client.flushNext();
    await stale;

    expect(runtime.store.getState().snapshot?.projects[0]?.name).toBe("Latest");
  });

  it("rolls back an optimistic project when project.add fails", async () => {
    const client = connectedClient(
      snapshotFixture({ projects: [], agents: [agentFixture()] }),
    );
    const runtime = startRuntime(client);
    cleanups.push(runtime.stop);
    await waitForPhase(runtime.store, "ready");

    client.setHandler(
      "project.add",
      rejectWith({ code: "PROJECT_ADD_FAILED", message: "path missing" }),
    );

    await expect(runtime.actions.addProject("/tmp/missing")).rejects.toThrow(
      /path missing/,
    );

    expect(runtime.store.getState().snapshot?.projects).toEqual([]);
    expect(runtime.store.getState().optimisticProjects).toEqual([]);
    expect(runtime.store.getState().selection.projectId).toBeNull();
    expect(
      runtime.store.getState().notifications.some((item) =>
        item.message.includes("path missing"),
      ),
    ).toBe(true);
  });

  it("keeps a connected snapshot when git.status fails", async () => {
    const client = connectedClient();
    const runtime = startRuntime(client);
    cleanups.push(runtime.stop);
    await waitForPhase(runtime.store, "ready");

    client.setHandler(
      "git.status",
      rejectWith({ code: "GIT_STATUS_FAILED", message: "not a git repo" }),
    );

    await runtime.actions.inspectGit({
      kind: "project",
      projectId: PROJECT_ID,
    });

    expect(runtime.store.getState().connection.phase).toBe("ready");
    expect(runtime.store.getState().git.error).toMatch(/not a git repo/);
    expect(runtime.store.getState().snapshot?.projects[0]?.name).toBe("Demo");
  });

  it("reconnects after a failed hello and then a successful handshake", async () => {
    const client = connectedClient();
    const runtime = startRuntime(client);
    cleanups.push(runtime.stop);
    await waitForPhase(runtime.store, "ready");

    client.setHandler("system.hello", rejectWith(disconnectedError));
    await runtime.actions.reconnect();
    await waitForPhase(runtime.store, "disconnected");

    client.setHandler("system.hello", () => helloFixture());
    await runtime.actions.reconnect();
    await waitForPhase(runtime.store, "ready");
    expect(runtime.store.getState().connection.daemonVersion).toBe("0.1.0");
  });

  it("treats daemon.shutting_down as a reconnect", async () => {
    const client = connectedClient();
    const runtime = startRuntime(client);
    cleanups.push(runtime.stop);
    await waitForPhase(runtime.store, "ready");

    client.setHandler("system.hello", rejectWith(disconnectedError));
    client.emit("daemon.shutting_down", {
      reasonCode: "shutdown",
      activeSessionCount: 0,
    });
    await waitForPhase(runtime.store, "disconnected");
  });

  it("never stores session.output bytes in React workspace state", async () => {
    const client = connectedClient(
      snapshotFixture({ sessions: [sessionFixture()] }),
    );
    const runtime = startRuntime(client);
    cleanups.push(runtime.stop);
    await waitForPhase(runtime.store, "ready");

    const writes: Array<{ sequence: number }> = [];
    const handle: TerminalSurfaceHandle = {
      write: () => true,
      writeOutput(chunk) {
        writes.push({ sequence: chunk.sequence });
        return "queued";
      },
      markOutputGap: () => true,
      markReplayComplete: () => true,
      reset: () => true,
      focus: () => true,
      getCursor: () => 0,
    };
    runtime.terminals.attach(SESSION_ID, handle);

    const marker = "UF5ZX09VVFBVVA==";
    client.emit("session.output", {
      sessionId: SESSION_ID,
      base64: marker,
      outputSequence: 4,
      replay: false,
    });

    expect(writes).toEqual([{ sequence: 4 }]);
    expect(JSON.stringify(runtime.store.getState())).not.toContain(marker);
    expect(runtime.store.getState().snapshot?.sessions[0]?.id).toBe(SESSION_ID);
  });

  it("marks an incompatible protocol without applying a snapshot", async () => {
    const client = createMockIpcClient({
      "system.hello": () => helloFixture({ protocolVersion: 2 }),
      "state.snapshot": () => snapshotFixture(),
    });
    const runtime = startRuntime(client);
    cleanups.push(runtime.stop);
    await waitForPhase(runtime.store, "incompatible");
    expect(runtime.store.getState().snapshot).toBeNull();
  });

  it("keeps agent.detect failures as a partial error", async () => {
    const client = createMockIpcClient({
      "system.hello": () => helloFixture(),
      "state.snapshot": () => snapshotFixture(),
      "agent.detect": rejectWith({
        code: "AGENT_DETECT_FAILED",
        message: "PATH unreadable",
      }),
    });
    const runtime = startRuntime(client);
    cleanups.push(runtime.stop);
    await waitForPhase(runtime.store, "ready");
    expect(runtime.store.getState().snapshot?.projects[0]?.id).toBe(PROJECT_ID);
    expect(
      runtime.store.getState().notifications.some((item) =>
        item.message.includes("PATH unreadable"),
      ),
    ).toBe(true);
    expect(AGENT_ID).toBeTruthy();
  });
});
