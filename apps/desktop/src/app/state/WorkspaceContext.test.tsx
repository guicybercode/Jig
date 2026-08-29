import { act, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { WorkspaceContextValue } from "./WorkspaceContext";
import { useWorkspace, WorkspaceProvider } from "./WorkspaceContext";
import type {
  AgentRecord,
  BootstrapResult,
  Project,
  Session,
  Worktree,
} from "../../ipc/types";
import { createMockIpcClient, EMPTY_BOOTSTRAP } from "../../test/mockIpc";

const TEST_TIME = 1_725_000_000_000;

describe("WorkspaceProvider terminal event isolation", () => {
  it("does not update metadata or rerender consumers for terminal byte events", async () => {
    const session = createSession();
    const client = createMockIpcClient({
      bootstrap: createBootstrap(session),
    });
    const onRender = vi.fn<(value: WorkspaceContextValue) => void>();
    render(
      <WorkspaceProvider client={client}>
        <MetadataProbe onRender={onRender} />
      </WorkspaceProvider>,
    );
    const metadata = await screen.findByRole("status", {
      name: "React session metadata",
    });
    await waitFor(() => expect(metadata).toHaveTextContent("connected|session-one:idle"));
    const settledRenderCount = onRender.mock.calls.length;
    const settledSnapshot = latestContext(onRender).snapshot;

    await act(async () => {
      client.emit("session.output", {
        sessionId: session.id,
        base64: "dGVybWluYWwgb3V0cHV0IG11c3QgYnlwYXNzIFJlYWN0",
        outputSequence: 1,
        replay: false,
      });
      client.emit("session.output_gap", {
        sessionId: session.id,
        requestedCursor: 1,
        firstAvailableSequence: 2,
        latestSequence: 3,
      });
      client.emit("session.replay_complete", {
        sessionId: session.id,
        outputSequence: 3,
      });
      await Promise.resolve();
    });

    expect(onRender).toHaveBeenCalledTimes(settledRenderCount);
    expect(latestContext(onRender).snapshot).toBe(settledSnapshot);
    expect(metadata).toHaveTextContent("connected|session-one:idle");

    act(() => {
      client.emit("session.status_changed", {
        sessionId: session.id,
        previousStatus: "idle",
        status: "running",
        changedAtMs: TEST_TIME + 1,
      });
    });
    await waitFor(() => expect(metadata).toHaveTextContent("session-one:running"));
    expect(onRender.mock.calls.length).toBeGreaterThan(settledRenderCount);
  });

  it("routes official wrapped metadata, lifecycle, and removal events", async () => {
    const session = createSession();
    const project = createProject();
    const agent = createAgent();
    const worktree = createWorktree();
    const client = createMockIpcClient({
      bootstrap: {
        hello: EMPTY_BOOTSTRAP.hello,
        snapshot: {
          ...EMPTY_BOOTSTRAP.snapshot,
          projects: [project],
          agents: [agent],
          sessions: [session],
          worktrees: [worktree],
        },
        agentDetections: [],
      },
    });
    const onRender = vi.fn<(value: WorkspaceContextValue) => void>();
    render(
      <WorkspaceProvider client={client}>
        <MetadataProbe onRender={onRender} />
      </WorkspaceProvider>,
    );
    await screen.findByText(/connected\|session-one:idle/);

    act(() => {
      client.emit("project.updated", {
        project: { ...project, name: "Renamed Repository" },
      });
      client.emit("agent.updated", {
        agent: {
          ...agent,
          displayName: "Codex CLI",
        },
      });
      client.emit("session.updated", {
        session: { ...session, name: "Renamed session" },
      });
      client.emit("worktree.updated", {
        worktree: { ...worktree, isDirty: true },
      });
      client.emit("session.status_changed", {
        sessionId: session.id,
        previousStatus: "idle",
        status: "running",
        changedAtMs: TEST_TIME + 1,
      });
      client.emit("session.exited", {
        sessionId: session.id,
        status: "exited",
        exitedAtMs: TEST_TIME + 2,
      });
    });

    await waitFor(() => {
      const workspace = latestContext(onRender);
      expect(workspace.projects[0]?.name).toBe("Renamed Repository");
      expect(workspace.agents[0]?.displayName).toBe("Codex CLI");
      expect(workspace.sessions[0]).toMatchObject({
        name: "Renamed session",
        status: "exited",
        updatedAtMs: TEST_TIME + 2,
      });
      expect(workspace.sessions[0]?.exitCode).toBeUndefined();
      expect(workspace.worktrees[0]?.isDirty).toBe(true);
    });

    act(() => {
      client.emit("worktree.removed", { worktreeId: worktree.id });
      client.emit("session.deleted", { sessionId: session.id });
      client.emit("agent.removed", { agentId: agent.id });
      client.emit("project.removed", { projectId: project.id });
    });

    await waitFor(() => {
      const workspace = latestContext(onRender);
      expect(workspace.projects).toHaveLength(0);
      expect(workspace.agents).toHaveLength(0);
      expect(workspace.sessions).toHaveLength(0);
      expect(workspace.worktrees).toHaveLength(0);
    });
  });
});

function MetadataProbe({
  onRender,
}: {
  readonly onRender: (value: WorkspaceContextValue) => void;
}) {
  const workspace = useWorkspace();
  onRender(workspace);
  return (
    <div role="status" aria-label="React session metadata">
      {workspace.connection.status}|
      {workspace.sessions
        .map((session) => `${session.id}:${session.status}`)
        .join(",")}
    </div>
  );
}

function latestContext(
  onRender: ReturnType<typeof vi.fn<(value: WorkspaceContextValue) => void>>,
): WorkspaceContextValue {
  const latestCall = onRender.mock.calls[onRender.mock.calls.length - 1];
  if (!latestCall) {
    throw new Error("Expected the workspace metadata probe to render.");
  }
  return latestCall[0];
}

function createBootstrap(session: Session): BootstrapResult {
  return {
    hello: EMPTY_BOOTSTRAP.hello,
    snapshot: {
      ...EMPTY_BOOTSTRAP.snapshot,
      projects: [createProject()],
      sessions: [session],
    },
    agentDetections: [],
  };
}

function createProject(): Project {
  return {
    id: "project-one",
    name: "Test Repository",
    path: "/repos/project",
    repositoryRoot: "/repos/project",
    currentBranch: "main",
    availability: "available",
    createdAtMs: TEST_TIME,
    lastOpenedAtMs: TEST_TIME,
  };
}

function createAgent(): AgentRecord {
  return {
    id: "agent-codex",
    displayName: "Codex",
    source: "built_in",
    command: {
      executable: "codex",
      args: [],
      env: {},
    },
    enabled: true,
  };
}

function createWorktree(): Worktree {
  return {
    id: "worktree-one",
    projectId: "project-one",
    sessionId: "session-one",
    path: "/repos/project/.worktrees/session-one",
    branch: "agent/session-one",
    isDirty: false,
    state: "active",
    createdAtMs: TEST_TIME,
    updatedAtMs: TEST_TIME,
  };
}

function createSession(): Session {
  return {
    id: "session-one",
    projectId: "project-one",
    name: "Test session",
    agentId: "agent-codex",
    cwd: "/repos/project",
    branch: "main",
    status: "idle",
    createdAtMs: TEST_TIME,
    updatedAtMs: TEST_TIME,
    lastActivityAtMs: TEST_TIME,
  };
}
