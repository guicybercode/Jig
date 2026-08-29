import type { ApiError, ProjectView, SessionView, TerminalView } from "../../ipc/types";

export interface WorkspaceModel {
  daemonConnected: boolean;
  projects: ProjectView[];
  sessions: SessionView[];
  selectedProjectId: string | null;
  error: ApiError | null;
  terminals: TerminalView[];
}

export const disconnectedWorkspace: WorkspaceModel = {
  daemonConnected: false,
  projects: [],
  sessions: [],
  selectedProjectId: null,
  error: null,
  terminals: [],
};

export const workspaceCommands = [
  { id: "project.add", label: "Add Project" },
  { id: "session.create", label: "New Session" },
  { id: "session.stop", label: "Stop Session" },
  { id: "worktree.remove", label: "Remove Worktree" },
] as const;

export type WorkspaceCommandId = (typeof workspaceCommands)[number]["id"];
