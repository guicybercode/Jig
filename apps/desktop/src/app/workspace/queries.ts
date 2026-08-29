import type {
  AgentId,
  DaemonEvent,
  ProjectId,
  SessionId,
  StateSnapshotResponse,
  WorktreeId,
} from "../../ipc";
import { EMPTY_SESSIONS, EMPTY_WORKTREES, INITIAL_GIT } from "./types";
import type { GitViewState, WorkspaceState } from "./types";

export type AppliedEvent =
  | { readonly kind: "snapshot"; readonly snapshot: StateSnapshotResponse }
  | { readonly kind: "git"; readonly git: GitViewState }
  | { readonly kind: "pty" }
  | { readonly kind: "shutdown" }
  | { readonly kind: "ignore" };

function upsertById<T extends { id: string }>(items: T[], item: T): T[] {
  const index = items.findIndex((current) => current.id === item.id);
  if (index < 0) {
    return [...items, item];
  }
  if (items[index] === item) {
    return items;
  }
  const next = items.slice();
  next[index] = item;
  return next;
}

function withoutId<T extends { id: string }>(items: T[], id: string): T[] {
  return items.filter((item) => item.id !== id);
}

/** Merges a snapshot with in-flight optimistic projects so races cannot drop them. */
export function mergeOptimisticProjects(
  snapshot: StateSnapshotResponse,
  optimisticProjects: readonly WorkspaceState["optimisticProjects"][number][],
): StateSnapshotResponse {
  if (optimisticProjects.length === 0) {
    return snapshot;
  }
  const ids = new Set(snapshot.projects.map((project) => project.id));
  const pending = optimisticProjects.filter((project) => !ids.has(project.id));
  if (pending.length === 0) {
    return snapshot;
  }
  return {
    ...snapshot,
    projects: [...snapshot.projects, ...pending],
  };
}

/** Applies one daemon metadata event. PTY events never mutate the snapshot. */
export function applyDaemonEvent(
  snapshot: StateSnapshotResponse,
  event: DaemonEvent,
  git: GitViewState,
): AppliedEvent {
  switch (event.event) {
    case "project.updated":
      return {
        kind: "snapshot",
        snapshot: {
          ...snapshot,
          projects: upsertById(snapshot.projects, event.payload.project),
        },
      };
    case "project.removed":
      return {
        kind: "snapshot",
        snapshot: {
          ...snapshot,
          projects: withoutId(snapshot.projects, event.payload.projectId),
        },
      };
    case "agent.updated":
      return {
        kind: "snapshot",
        snapshot: {
          ...snapshot,
          agents: upsertById(snapshot.agents, event.payload.agent),
        },
      };
    case "agent.removed":
      return {
        kind: "snapshot",
        snapshot: {
          ...snapshot,
          agents: snapshot.agents.filter(
            (agent) => agent.id !== event.payload.agentId,
          ),
        },
      };
    case "session.created":
    case "session.updated":
      return {
        kind: "snapshot",
        snapshot: {
          ...snapshot,
          sessions: upsertById(snapshot.sessions, event.payload.session),
        },
      };
    case "session.deleted":
      return {
        kind: "snapshot",
        snapshot: {
          ...snapshot,
          sessions: snapshot.sessions.filter(
            (session) => session.id !== event.payload.sessionId,
          ),
        },
      };
    case "session.status_changed":
      return {
        kind: "snapshot",
        snapshot: {
          ...snapshot,
          sessions: snapshot.sessions.map((session) =>
            session.id === event.payload.sessionId
              ? {
                  ...session,
                  status: event.payload.status,
                  updatedAtMs: event.payload.changedAtMs,
                  errorCode: event.payload.reasonCode,
                }
              : session,
          ),
        },
      };
    case "session.exited":
      return {
        kind: "snapshot",
        snapshot: {
          ...snapshot,
          sessions: snapshot.sessions.map((session) =>
            session.id === event.payload.sessionId
              ? {
                  ...session,
                  status: event.payload.status,
                  exitCode: event.payload.exitCode,
                  updatedAtMs: event.payload.exitedAtMs,
                }
              : session,
          ),
        },
      };
    case "worktree.updated":
      return {
        kind: "snapshot",
        snapshot: {
          ...snapshot,
          worktrees: upsertById(snapshot.worktrees, event.payload.worktree),
        },
      };
    case "worktree.removed":
      return {
        kind: "snapshot",
        snapshot: {
          ...snapshot,
          worktrees: snapshot.worktrees.filter(
            (worktree) => worktree.id !== event.payload.worktreeId,
          ),
        },
      };
    case "git.status_changed":
      return {
        kind: "git",
        git: {
          ...git,
          status: event.payload.status,
          error: null,
        },
      };
    case "session.output":
    case "session.output_gap":
    case "session.replay_complete":
      return { kind: "pty" };
    case "daemon.shutting_down":
      return { kind: "shutdown" };
    default:
      return { kind: "ignore" };
  }
}

/** Returns whether this event is PTY traffic that must stay out of React state. */
export function isPtyEvent(event: DaemonEvent["event"]): boolean {
  return (
    event === "session.output" ||
    event === "session.output_gap" ||
    event === "session.replay_complete"
  );
}

export function projectById(
  snapshot: StateSnapshotResponse | null,
  projectId: ProjectId | null,
): StateSnapshotResponse["projects"][number] | null {
  if (!snapshot || !projectId) {
    return null;
  }
  return snapshot.projects.find((project) => project.id === projectId) ?? null;
}

export function sessionsForProject(
  snapshot: StateSnapshotResponse | null,
  projectId: ProjectId | null,
): StateSnapshotResponse["sessions"] {
  if (!snapshot || !projectId) {
    return EMPTY_SESSIONS;
  }
  return snapshot.sessions.filter((session) => session.projectId === projectId);
}

export function worktreesForProject(
  snapshot: StateSnapshotResponse | null,
  projectId: ProjectId | null,
): StateSnapshotResponse["worktrees"] {
  if (!snapshot || !projectId) {
    return EMPTY_WORKTREES;
  }
  return snapshot.worktrees.filter(
    (worktree) => worktree.projectId === projectId,
  );
}

export function detectionForAgent(
  detections: WorkspaceState["detections"],
  agentId: AgentId,
): boolean | undefined {
  return detections.find((item) => item.agentId === agentId)?.available;
}

export function clearGit(): GitViewState {
  return INITIAL_GIT;
}

export function worktreeById(
  snapshot: StateSnapshotResponse | null,
  worktreeId: WorktreeId | null,
): StateSnapshotResponse["worktrees"][number] | null {
  if (!snapshot || !worktreeId) {
    return null;
  }
  return snapshot.worktrees.find((worktree) => worktree.id === worktreeId) ?? null;
}

export function sessionById(
  snapshot: StateSnapshotResponse | null,
  sessionId: SessionId | null,
): StateSnapshotResponse["sessions"][number] | null {
  if (!snapshot || !sessionId) {
    return null;
  }
  return snapshot.sessions.find((session) => session.id === sessionId) ?? null;
}
