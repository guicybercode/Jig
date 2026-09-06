import { beforeEach, describe, expect, it, vi } from "vitest";

const transport = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  openPath: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: transport.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: transport.listen }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: transport.openPath }));

import { createTauriIpcClient } from "./client";
import discoveryFixture from "../../../../protocol/fixtures/knowledge-discovery.json";
import organizationFixture from "../../../../protocol/fixtures/organization.json";
import type { OrganizationGetRequest, OrganizationSaveRequest } from "./domain";
import type { RequestEnvelope } from "./types";

const PROJECT = {
  id: "0198f000-0000-7000-8000-000000000001",
  name: "CLI Master",
  path: "/repos/cli-master",
  repositoryRoot: "/repos/cli-master",
  currentBranch: "main",
  createdAtMs: 1_725_000_000_000,
  lastOpenedAtMs: 1_725_000_000_100,
};

const AGENT_RECORD = {
  id: "0198f000-0000-7000-8000-000000000002",
  displayName: "Codex",
  source: "built_in",
  command: {
    executable: "codex",
    args: [],
    env: {},
  },
  enabled: true,
};

const SESSION = {
  id: "0198f000-0000-7000-8000-000000000003",
  projectId: PROJECT.id,
  name: "Review auth",
  agentId: AGENT_RECORD.id,
  cwd: "/repos/cli-master/apps/desktop",
  branch: "agent/review-auth",
  status: "starting",
  createdAtMs: 1_725_000_000_200,
  updatedAtMs: 1_725_000_000_200,
};

describe("production IPC wire contract", () => {
  beforeEach(() => {
    transport.invoke.mockReset();
    transport.listen.mockReset();
    transport.openPath.mockReset();
  });

  it("bootstraps AgentRecord and joins the separate detection response", async () => {
    installWireResponder({
      "system.hello": {
        protocolVersion: 1,
        daemonVersion: "0.1.0",
        instanceId: "0198f000-0000-7000-8000-000000000005",
      },
      "state.snapshot": {
        schemaVersion: 1,
        projects: [PROJECT],
        agents: [AGENT_RECORD],
        sessions: [],
        worktrees: [],
      },
      "agent.detect": {
        detections: [
          {
            agentId: AGENT_RECORD.id,
            available: true,
            executablePath: "/usr/local/bin/codex",
          },
        ],
      },
    });

    const bootstrap = await createTauriIpcClient().initialize();

    expect(bootstrap.snapshot.agents[0]).toEqual(AGENT_RECORD);
    expect(bootstrap.agentDetections[0]).toMatchObject({
      agentId: AGENT_RECORD.id,
      available: true,
    });
    expect(capturedRequests().map((request) => request.method)).toEqual([
      "system.hello",
      "state.snapshot",
      "agent.detect",
    ]);
    expect(capturedRequests()[2]?.payload).toEqual({
      agentIds: [AGENT_RECORD.id],
    });
  });

  it("sends daemon-authoritative session, agent, Git, and worktree payloads", async () => {
    installWireResponder({
      "agent.custom.create": {
        ...AGENT_RECORD,
        id: "0198f000-0000-7000-8000-000000000006",
        displayName: "Local agent",
        source: "custom",
      },
      "session.create": SESSION,
      "git.status": {
        branch: "main",
        files: [],
        counts: { modified: 0, added: 0, deleted: 0, untracked: 0 },
        hasStaged: false,
        hasTrackedChanges: false,
        hasUntracked: false,
        isDirty: false,
      },
      "worktree.prepare_remove": {
        status: "ready",
        worktreeId: "0198f000-0000-7000-8000-000000000004",
        confirmationToken: "abcdefghijklmnop",
        expiresAtMs: 1_725_000_060_000,
      },
      "worktree.remove": {},
    });
    const client = createTauriIpcClient();

    await client.createCustomAgent({
      displayName: "Local agent",
      command: { executable: "local-agent", args: ["--safe"], env: {} },
    });
    await client.createSession({
      projectId: PROJECT.id,
      name: "Review auth",
      agentId: AGENT_RECORD.id,
      isolation: "new_worktree",
      relativeDirectory: "apps/desktop",
    });
    await client.getGitStatus({ kind: "session", sessionId: SESSION.id });
    await client.prepareWorktreeRemoval("0198f000-0000-7000-8000-000000000004");
    await client.removeWorktree({
      worktreeId: "0198f000-0000-7000-8000-000000000004",
      confirmationToken: "abcdefghijklmnop",
    });

    const requests = Object.fromEntries(
      capturedRequests().map((request) => [request.method, request.payload]),
    );
    expect(requests["agent.custom.create"]).toEqual({
      displayName: "Local agent",
      command: { executable: "local-agent", args: ["--safe"], env: {} },
    });
    expect(requests["session.create"]).toEqual({
      projectId: PROJECT.id,
      name: "Review auth",
      agentId: AGENT_RECORD.id,
      isolation: "new_worktree",
      relativeDirectory: "apps/desktop",
    });
    expect(requests["session.create"]).not.toHaveProperty("cwd");
    expect(requests["session.create"]).not.toHaveProperty("additionalArgs");
    expect(requests["git.status"]).toEqual({
      target: { kind: "session", sessionId: SESSION.id },
    });
    expect(requests["worktree.remove"]).toEqual({
      worktreeId: "0198f000-0000-7000-8000-000000000004",
      confirmationToken: "abcdefghijklmnop",
    });
    expect(requests["worktree.remove"]).not.toHaveProperty("allowDirty");
    for (const request of capturedRequests()) {
      expect(request.requestId).toMatch(
        /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i,
      );
    }
  });

  it("subscribes to the official Tauri channel and validates event envelopes", async () => {
    let receive: ((event: { readonly payload: unknown }) => void) | undefined;
    const unlisten = vi.fn();
    transport.listen.mockImplementation(async (_channel, handler) => {
      receive = handler;
      return unlisten;
    });
    const handler = vi.fn();
    const onError = vi.fn();

    const unsubscribe = await createTauriIpcClient().subscribe(handler, onError);
    expect(transport.listen).toHaveBeenCalledWith(
      "daemon:event",
      expect.any(Function),
    );

    receive?.({
      payload: {
        kind: "event",
        version: 1,
        event: "session.status_changed",
        sequence: 7,
        payload: {
          sessionId: SESSION.id,
          previousStatus: "starting",
          status: "running",
          changedAtMs: 1_725_000_000_300,
        },
      },
    });
    expect(handler).toHaveBeenCalledWith(
      expect.objectContaining({
        event: "session.status_changed",
        sequence: 7,
      }),
    );
    expect(onError).not.toHaveBeenCalled();

    receive?.({ payload: { kind: "event", version: 2 } });
    expect(onError).toHaveBeenCalledWith(
      expect.objectContaining({ code: "invalid_ipc_payload" }),
    );
    unsubscribe();
    expect(unlisten).toHaveBeenCalledOnce();
  });

  it("opens a filtered terminal relay and encodes input and resize requests", async () => {
    let receive: ((event: { readonly payload: unknown }) => void) | undefined;
    const unlisten = vi.fn();
    transport.listen.mockImplementation(async (_channel, handler) => {
      receive = handler;
      return unlisten;
    });
    transport.invoke.mockImplementation(async (command, args) => {
      if (command === "daemon_terminal_unsubscribe") {
        return undefined;
      }
      const request = requestFromArgs(args);
      return {
        kind: "response",
        version: 1,
        requestId: request.requestId,
        status: "success",
        data: {},
      };
    });
    const handler = vi.fn();
    const onError = vi.fn();
    const client = createTauriIpcClient();

    const unsubscribe = await client.subscribeTerminal(
      { sessionId: SESSION.id, cursor: 12 },
      handler,
      onError,
    );
    await client.writeTerminal(SESSION.id, new Uint8Array([0x03, 0x0a]));
    await client.resizeTerminal({
      sessionId: SESSION.id,
      columns: 120,
      rows: 36,
    });

    receive?.({
      payload: {
        kind: "event",
        version: 1,
        event: "session.output",
        sequence: 8,
        payload: {
          sessionId: "0198f000-0000-7000-8000-000000000099",
          base64: "eA==",
          outputSequence: 13,
          replay: false,
        },
      },
    });
    receive?.({
      payload: {
        kind: "event",
        version: 1,
        event: "session.output",
        sequence: 9,
        payload: {
          sessionId: SESSION.id,
          base64: "eA==",
          outputSequence: 13,
          replay: false,
        },
      },
    });

    expect(handler).toHaveBeenCalledOnce();
    expect(onError).not.toHaveBeenCalled();
    expect(requestFromArgs(transport.invoke.mock.calls[0]?.[1])).toMatchObject({
      method: "session.subscribe",
      payload: { sessionId: SESSION.id, cursor: 12 },
    });
    expect(requestFromArgs(transport.invoke.mock.calls[1]?.[1])).toMatchObject({
      method: "session.write",
      payload: { sessionId: SESSION.id, base64: "Awo=" },
    });
    expect(requestFromArgs(transport.invoke.mock.calls[2]?.[1])).toMatchObject({
      method: "session.resize",
      payload: { sessionId: SESSION.id, columns: 120, rows: 36 },
    });

    unsubscribe();
    expect(unlisten).toHaveBeenCalledOnce();
    expect(transport.invoke).toHaveBeenLastCalledWith(
      "daemon_terminal_unsubscribe",
      { sessionId: SESSION.id },
    );
  });

  it("decodes the conservative blocked worktree-removal variant", async () => {
    installWireResponder({
      "worktree.prepare_remove": {
        status: "blocked",
        worktreeId: "0198f000-0000-7000-8000-000000000004",
        isDirty: true,
        blockers: [
          "ignored_files",
          "assume_unchanged",
          "skip_worktree",
          "locked",
          "running",
          "in_use",
        ],
      },
    });

    await expect(
      createTauriIpcClient().prepareWorktreeRemoval(
        "0198f000-0000-7000-8000-000000000004",
      ),
    ).resolves.toEqual({
      status: "blocked",
      worktreeId: "0198f000-0000-7000-8000-000000000004",
      isDirty: true,
      blockers: [
        "ignored_files",
        "assume_unchanged",
        "skip_worktree",
        "locked",
        "running",
        "in_use",
      ],
    });
  });
});

function installWireResponder(
  responses: Readonly<Record<string, unknown>>,
): void {
  transport.invoke.mockImplementation(async (command, args) => {
    if (command !== "daemon_request") {
      throw new Error(`Unexpected Tauri command: ${String(command)}`);
    }
    const request = requestFromArgs(args);
    if (!(request.method in responses)) {
      throw new Error(`No wire response configured for ${request.method}`);
    }
    return {
      kind: "response",
      version: 1,
      requestId: request.requestId,
      status: "success",
      data: responses[request.method],
    };
  });
}

function capturedRequests(): readonly RequestEnvelope<unknown>[] {
  return transport.invoke.mock.calls.map(([, args]) => requestFromArgs(args));
}

function requestFromArgs(value: unknown): RequestEnvelope<unknown> {
  if (
    typeof value !== "object" ||
    value === null ||
    !("request" in value)
  ) {
    throw new Error("Expected invoke args containing a request envelope.");
  }
  return value.request as RequestEnvelope<unknown>;
}

// Knowledge uses the same versioned request transport as all domain methods.
describe("knowledge IPC transport", () => {
  it("keeps revisions and scope and surfaces conflicts without issuing session writes", async () => {
    const entry = {
      id: "0198f000-0000-7000-8000-000000000010", kind: "prompt", projectId: null,
      title: "Review", body: "Review the selected changes", revision: 1,
      createdAtMs: 100, updatedAtMs: 100,
    };
    transport.invoke.mockReset();
    installWireResponder({
      "knowledge.list": { entries: [entry], nextCursor: null },
      "knowledge.save": entry,
      "knowledge.delete": {},
    });
    const client = createTauriIpcClient();
    expect(await client.listKnowledge({ projectId: null, query: "review" })).toEqual({ entries: [entry], nextCursor: null });
    const input = { kind: "prompt" as const, projectId: null, title: entry.title, body: entry.body };
    expect(await client.saveKnowledge(input)).toEqual(entry);
    await client.deleteKnowledge({ id: entry.id, expectedRevision: 1 });
    expect(capturedRequests().map((request) => request.method)).toEqual(["knowledge.list", "knowledge.save", "knowledge.delete"]);
    expect(capturedRequests()[1].payload).toEqual(input);
    expect(capturedRequests()[2].payload).toEqual({ id: entry.id, expectedRevision: 1 });

    transport.invoke.mockImplementation(async (_command, { request }) => ({
      kind: "response", version: 1, requestId: request.requestId, status: "error",
      error: { code: "knowledge_conflict", message: "This entry changed. Reload or save a copy." },
    }));
    await expect(client.saveKnowledge({ ...input, id: entry.id, expectedRevision: 1 })).rejects.toMatchObject({ code: "knowledge_conflict" });
  });
});

describe("discovery IPC transport", () => {
  beforeEach(() => {
    transport.invoke.mockReset();
    transport.openPath.mockReset();
  });

  it("sends opaque scan/source capabilities through the generic daemon bridge", async () => {
    installWireResponder({ "knowledge.discover": discoveryFixture.scan, "knowledge.read": discoveryFixture.read });
    const client = createTauriIpcClient();
    expect(await client.discoverKnowledge({ projectId: PROJECT.id })).toEqual(discoveryFixture.scan);
    const selection = { scanId: discoveryFixture.scan.scanId, entryId: discoveryFixture.read.entry.entryId };
    expect(await client.readKnowledge(selection)).toEqual(discoveryFixture.read);
    expect(capturedRequests().map((request) => ({ method: request.method, payload: request.payload }))).toEqual([
      { method: "knowledge.discover", payload: { projectId: PROJECT.id } },
      { method: "knowledge.read", payload: selection },
    ]);
    expect(transport.openPath).not.toHaveBeenCalled();
  });

  it("rejects content returned for a different source capability", async () => {
    installWireResponder({ "knowledge.read": discoveryFixture.read });
    await expect(createTauriIpcClient().readKnowledge({
      scanId: discoveryFixture.scan.scanId,
      entryId: "0198b6e0-0002-7000-8000-000000000002",
    })).rejects.toThrow("Discovery read returned another source capability");
  });

  it("preserves scan expiry errors for explicit rescan recovery", async () => {
    transport.invoke.mockImplementation(async (_command, { request }) => ({
      kind: "response", version: 1, requestId: request.requestId, status: "error",
      error: { code: "knowledge_scan_expired", message: "Rescan sources before reading this item." },
    }));
    await expect(createTauriIpcClient().readKnowledge({
      scanId: discoveryFixture.scan.scanId, entryId: discoveryFixture.read.entry.entryId,
    })).rejects.toMatchObject({ code: "knowledge_scan_expired" });
    expect(capturedRequests()).toHaveLength(1);
  });
});

describe("organization IPC transport", () => {
  beforeEach(() => { transport.invoke.mockReset(); });

  it("batches existing targets and explicitly saves metadata without runtime methods", async () => {
    installWireResponder({ "organization.get": organizationFixture.response, "organization.save": organizationFixture.saved });
    const client = createTauriIpcClient();
    const input: OrganizationGetRequest = { targets: [
      { kind: "session", id: organizationFixture.request.targets[0].id },
      { kind: "project", id: organizationFixture.request.targets[1].id },
    ] };
    expect(await client.getOrganization(input)).toEqual(organizationFixture.response);
    const save: OrganizationSaveRequest = { target: input.targets[0], expectedRevision: 1, pinned: true, archived: true, workflow: "in_review" };
    expect(await client.saveOrganization(save)).toEqual(organizationFixture.saved);
    expect(capturedRequests().map((request) => ({ method: request.method, payload: request.payload }))).toEqual([
      { method: "organization.get", payload: input }, { method: "organization.save", payload: save },
    ]);
  });

  it("rejects an empty batch before transport and preserves optimistic conflicts", async () => {
    const client = createTauriIpcClient();
    await expect(client.getOrganization({ targets: [] })).rejects.toThrow();
    expect(transport.invoke).not.toHaveBeenCalled();
    transport.invoke.mockImplementation(async (_command, { request }) => ({
      kind: "response", version: 1, requestId: request.requestId, status: "error",
      error: { code: "organization_conflict", message: "Organization changed in another window." },
    }));
    await expect(client.saveOrganization({ target: { kind: "project", id: PROJECT.id }, expectedRevision: 0, pinned: true, archived: false, workflow: null })).rejects.toMatchObject({ code: "organization_conflict" });
  });
});
