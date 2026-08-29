import type { Project, ProjectId, StateSnapshotResponse } from "../../ipc";
import { parseProjectId } from "../../ipc";

function upsertById<T extends { id: string }>(items: T[], item: T): T[] {
  const index = items.findIndex((current) => current.id === item.id);
  if (index < 0) {
    return [...items, item];
  }
  const next = items.slice();
  next[index] = item;
  return next;
}

/** Empty snapshot used only while an optimistic project is the first entity. */
export function emptySnapshot(): StateSnapshotResponse {
  return {
    schemaVersion: 1,
    projects: [],
    agents: [],
    sessions: [],
    worktrees: [],
  };
}

/** Builds a client-side project row replaced after `project.add` succeeds. */
export function createOptimisticProject(
  path: string,
  name: string | undefined,
): Project {
  const now = Date.now();
  const trimmedName = name?.trim();
  const inferred =
    trimmedName && trimmedName.length > 0
      ? trimmedName
      : (path.split("/").filter(Boolean).pop() ?? path);
  return {
    id: parseProjectId(crypto.randomUUID()),
    name: inferred,
    path,
    createdAtMs: now,
    lastOpenedAtMs: now,
  };
}

/** Inserts or replaces a project in the snapshot. */
export function withProject(
  snapshot: StateSnapshotResponse,
  project: Project,
): StateSnapshotResponse {
  return {
    ...snapshot,
    projects: upsertById(snapshot.projects, project),
  };
}

/** Drops a project from the snapshot. */
export function withoutProject(
  snapshot: StateSnapshotResponse,
  projectId: ProjectId,
): StateSnapshotResponse {
  return {
    ...snapshot,
    projects: snapshot.projects.filter((project) => project.id !== projectId),
  };
}

/** Replaces an optimistic row with the daemon's committed project. */
export function replaceProject(
  snapshot: StateSnapshotResponse,
  previousId: ProjectId,
  project: Project,
): StateSnapshotResponse {
  const withoutPrevious = snapshot.projects.filter(
    (item) => item.id !== previousId,
  );
  return {
    ...snapshot,
    projects: upsertById(withoutPrevious, project),
  };
}
