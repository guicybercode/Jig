import type { ProjectId, SessionId, StateSnapshotResponse } from "../../ipc";
import { EMPTY_SESSION_IDS, type SelectionState } from "./types";

const MAX_VISIBLE_SESSIONS = 4;

function uniqueSessionIds(ids: readonly SessionId[]): SessionId[] {
  return [...new Set(ids)];
}

/** Picks a project and keeps session focus inside that project's sessions. */
export function selectProject(
  snapshot: StateSnapshotResponse | null,
  projectId: ProjectId | null,
): SelectionState {
  if (!snapshot || !projectId) {
    return {
      projectId,
      sessionId: null,
      visibleSessionIds: EMPTY_SESSION_IDS,
    };
  }
  const sessions = snapshot.sessions.filter(
    (session) => session.projectId === projectId,
  );
  const sessionId = sessions[0]?.id ?? null;
  return {
    projectId,
    sessionId,
    visibleSessionIds: sessionId ? [sessionId] : EMPTY_SESSION_IDS,
  };
}

/** Focuses a session and ensures it is among the visible terminals. */
export function focusSession(
  selection: SelectionState,
  sessionId: SessionId,
): SelectionState {
  const visible = uniqueSessionIds([
    sessionId,
    ...selection.visibleSessionIds,
  ]).slice(0, MAX_VISIBLE_SESSIONS);
  return {
    ...selection,
    sessionId,
    visibleSessionIds: visible,
  };
}

/** Adds or removes a session from the terminal grid without dropping focus. */
export function toggleVisibleSession(
  selection: SelectionState,
  sessionId: SessionId,
): SelectionState {
  const exists = selection.visibleSessionIds.includes(sessionId);
  const visible = exists
    ? selection.visibleSessionIds.filter((id) => id !== sessionId)
    : uniqueSessionIds([...selection.visibleSessionIds, sessionId]).slice(
        0,
        MAX_VISIBLE_SESSIONS,
      );
  return {
    ...selection,
    sessionId,
    visibleSessionIds: visible,
  };
}

/** Drops selection that no longer exists after a snapshot or event. */
export function reconcileSelection(
  selection: SelectionState,
  snapshot: StateSnapshotResponse,
): SelectionState {
  const projectId =
    selection.projectId &&
    snapshot.projects.some((project) => project.id === selection.projectId)
      ? selection.projectId
      : (snapshot.projects[0]?.id ?? null);
  const sessions = snapshot.sessions.filter(
    (session) => session.projectId === projectId,
  );
  const visible = selection.visibleSessionIds.filter((id) =>
    sessions.some((session) => session.id === id),
  );
  const sessionId =
    selection.sessionId &&
    sessions.some((session) => session.id === selection.sessionId)
      ? selection.sessionId
      : (visible[0] ?? sessions[0]?.id ?? null);
  const nextVisible =
    visible.length > 0
      ? visible
      : sessionId
        ? [sessionId]
        : EMPTY_SESSION_IDS;
  if (
    selection.projectId === projectId &&
    selection.sessionId === sessionId &&
    selection.visibleSessionIds.length === nextVisible.length &&
    selection.visibleSessionIds.every((id, index) => id === nextVisible[index])
  ) {
    return selection;
  }
  return {
    projectId,
    sessionId,
    visibleSessionIds: nextVisible,
  };
}
