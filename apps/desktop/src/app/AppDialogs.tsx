import { useWorkspace } from "./state/WorkspaceContext";
import {
  AddProjectDialog,
  RemoveProjectDialog,
  RenameProjectDialog,
} from "./features/projects/ProjectDialogs";
import { NewSessionDialog } from "./features/sessions/NewSessionDialog";
import {
  DeleteSessionDialog,
  GitStatusDialog,
  RemoveWorktreeDialog,
  RenameSessionDialog,
  StopSessionDialog,
} from "./features/sessions/SessionDialogs";

/** Resolves typed overlay targets and mounts only the active workflow. */
export function AppDialogs() {
  const workspace = useWorkspace();
  const overlay = workspace.overlay;

  if (!overlay || overlay.kind === "command-palette") {
    return null;
  }

  switch (overlay.kind) {
    case "add-project":
      return (
        <AddProjectDialog
          open
          onClose={workspace.closeOverlay}
          onAdd={workspace.addProject}
        />
      );
    case "new-session": {
      const project = workspace.projects.find(
        (candidate) => candidate.id === overlay.projectId,
      );
      return project ? (
        <NewSessionDialog
          open
          project={project}
          agents={workspace.agents}
          agentDetections={workspace.agentDetections}
          onClose={workspace.closeOverlay}
          onCreateCustomAgent={workspace.createCustomAgent}
          onCreate={workspace.createSession}
        />
      ) : null;
    }
    case "rename-project": {
      const project = workspace.projects.find(
        (candidate) => candidate.id === overlay.projectId,
      );
      return project ? (
        <RenameProjectDialog
          open
          project={project}
          onClose={workspace.closeOverlay}
          onRename={(projectId, name) =>
            workspace.renameProject({ projectId, name })
          }
        />
      ) : null;
    }
    case "remove-project": {
      const project = workspace.projects.find(
        (candidate) => candidate.id === overlay.projectId,
      );
      return project ? (
        <RemoveProjectDialog
          open
          project={project}
          onClose={workspace.closeOverlay}
          onRemove={workspace.removeProject}
        />
      ) : null;
    }
    case "rename-session": {
      const session = workspace.sessions.find(
        (candidate) => candidate.id === overlay.sessionId,
      );
      return session ? (
        <RenameSessionDialog
          open
          session={session}
          onClose={workspace.closeOverlay}
          onRename={(sessionId, name) =>
            workspace.renameSession({ sessionId, name })
          }
        />
      ) : null;
    }
    case "stop-session": {
      const session = workspace.sessions.find(
        (candidate) => candidate.id === overlay.sessionId,
      );
      return session ? (
        <StopSessionDialog
          open
          session={session}
          onClose={workspace.closeOverlay}
          onStop={(sessionId) => workspace.stopSession({ sessionId })}
        />
      ) : null;
    }
    case "delete-session": {
      const session = workspace.sessions.find(
        (candidate) => candidate.id === overlay.sessionId,
      );
      return session ? (
        <DeleteSessionDialog
          open
          session={session}
          worktree={workspace.worktrees.find(
            (candidate) => candidate.id === session.worktreeId,
          )}
          onClose={workspace.closeOverlay}
          onDelete={(sessionId) => workspace.deleteSession({ sessionId })}
        />
      ) : null;
    }
    case "remove-worktree": {
      const worktree = workspace.worktrees.find(
        (candidate) => candidate.id === overlay.worktreeId,
      );
      return worktree ? (
        <RemoveWorktreeDialog
          open
          worktree={worktree}
          onClose={workspace.closeOverlay}
          onPrepare={workspace.prepareWorktreeRemoval}
          onRemove={(preparation) =>
            preparation.status === "ready"
              ? workspace.removeWorktree({
                  worktreeId: preparation.worktreeId,
                  confirmationToken: preparation.confirmationToken,
                })
              : Promise.reject(new Error("Removal confirmation is unavailable."))
          }
        />
      ) : null;
    }
    case "git-status": {
      const session = workspace.sessions.find(
        (candidate) => candidate.id === overlay.sessionId,
      );
      return session ? (
        <GitStatusDialog
          open
          session={session}
          onClose={workspace.closeOverlay}
          onLoad={workspace.getGitStatus}
        />
      ) : null;
    }
  }
}
