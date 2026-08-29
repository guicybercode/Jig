import { describe, expect, it } from "vitest";

import {
  decodeAgentDetectResponse,
  decodeApiError,
  decodeSnapshot,
} from "./schema";

const PROJECT_ID = "0198f000-0000-7000-8000-000000000001";
const AGENT_ID = "0198f000-0000-7000-8000-000000000002";
const SESSION_ID = "0198f000-0000-7000-8000-000000000003";
const WORKTREE_ID = "0198f000-0000-7000-8000-000000000004";

describe("IPC schema fixtures", () => {
  it("decodes an official Rust snapshot fixture", () => {
    const snapshot = decodeSnapshot({
      schemaVersion: 1,
      projects: [
        {
          id: PROJECT_ID,
          name: "CLI Master",
          path: "/repos/cli-master",
          repositoryRoot: "/repos/cli-master",
          currentBranch: "main",
          createdAtMs: 1_725_000_000_000,
          lastOpenedAtMs: 1_725_000_000_100,
        },
      ],
      agents: [
        {
          id: AGENT_ID,
          displayName: "Codex",
          source: "built_in",
          command: {
            executable: "codex",
            args: [],
            env: {},
          },
          enabled: true,
        },
      ],
      sessions: [
        {
          id: SESSION_ID,
          projectId: PROJECT_ID,
          name: "Active session",
          agentId: AGENT_ID,
          cwd: "/repos/cli-master",
          status: "idle",
          createdAtMs: 1_725_000_000_200,
          updatedAtMs: 1_725_000_000_300,
        },
      ],
      worktrees: [
        {
          id: WORKTREE_ID,
          projectId: PROJECT_ID,
          sessionId: SESSION_ID,
          path: "/repos/cli-master/.worktrees/session-one",
          branch: "agent/future-lifecycle",
          isDirty: false,
          state: "active",
          createdAtMs: 1_725_000_000_200,
          updatedAtMs: 1_725_000_000_300,
        },
      ],
    });

    expect(snapshot.schemaVersion).toBe(1);
    expect(snapshot.projects[0]?.repositoryRoot).toBe("/repos/cli-master");
    expect(snapshot.agents[0]).toEqual({
      id: AGENT_ID,
      displayName: "Codex",
      description: undefined,
      source: "built_in",
      command: {
        executable: "codex",
        args: [],
        env: {},
      },
      enabled: true,
    });
    expect(snapshot.worktrees[0]).toMatchObject({
      state: "active",
      updatedAtMs: 1_725_000_000_300,
    });
    const detections = decodeAgentDetectResponse({
      detections: [{
        agentId: AGENT_ID,
        available: false,
        errorCode: "executable_not_found",
      }],
    });
    expect(detections[0]).toEqual({
      agentId: AGENT_ID,
      available: false,
      executablePath: undefined,
      errorCode: "executable_not_found",
    });
    expect(snapshot.sessions[0]?.status).toBe("idle");
    expect(snapshot.sessions[0]?.name).toBe("Active session");
  });

  it("normalizes future session status and optional project availability", () => {
    const snapshot = decodeSnapshot({
      schemaVersion: 1,
      projects: [
        {
          id: PROJECT_ID,
          name: "Moved repository",
          path: "/repos/moved",
          availability: "missing",
          availabilityMessage: "The directory no longer exists.",
          createdAtMs: 1_725_000_000_000,
          lastOpenedAtMs: 1_725_000_000_100,
        },
      ],
      agents: [],
      sessions: [
        {
          id: SESSION_ID,
          projectId: PROJECT_ID,
          name: "Future lifecycle",
          agentId: AGENT_ID,
          cwd: "/repos/moved",
          status: "paused_by_future_daemon",
          createdAtMs: 1_725_000_000_200,
          updatedAtMs: 1_725_000_000_300,
        },
      ],
      worktrees: [],
    });

    expect(snapshot.projects[0]).toMatchObject({
      availability: "missing",
      availabilityMessage: "The directory no longer exists.",
    });
    expect(snapshot.sessions[0]?.status).toBe("unknown");
  });

  it("preserves actionable daemon error fields", () => {
    const error = decodeApiError({
      code: "repository_not_found",
      message: "No Git repository was found.",
      action: "Choose a repository root and try again.",
      details: { path: "/tmp/not-a-repository" },
    });

    expect(error.code).toBe("repository_not_found");
    expect(error.message).toBe("No Git repository was found.");
    expect(error.action).toBe("Choose a repository root and try again.");
    expect(error.details).toEqual({ path: "/tmp/not-a-repository" });
  });
});
