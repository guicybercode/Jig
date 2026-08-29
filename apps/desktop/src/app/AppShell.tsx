import { useState } from "react";

import type { IpcClient } from "../ipc/client";
import { AppHeader } from "./components/AppHeader";
import { CommandPalette } from "./features/commands/CommandPalette";
import { ConfirmDialog } from "./features/confirm/ConfirmDialog";
import { ErrorBanner } from "./features/errors/ErrorBanner";
import { ProjectSidebar } from "./features/navigation/ProjectSidebar";
import { CreateSessionDialog } from "./features/sessions/CreateSessionDialog";
import { LocalStatusBar } from "./features/status/LocalStatusBar";
import { TerminalGrid } from "./features/terminal/TerminalGrid";
import { WorkspaceEmptyState } from "./features/workspace/WorkspaceEmptyState";
import {
  disconnectedWorkspace,
  type WorkspaceCommandId,
  type WorkspaceModel,
} from "./workspace/model";

interface AppShellProps {
  initial?: WorkspaceModel;
  ipc?: IpcClient;
}

/** Composes the persistent desktop application regions. */
export function AppShell({
  initial = disconnectedWorkspace,
  ipc,
}: AppShellProps) {
  const [workspace, setWorkspace] = useState(initial);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [createOpen, setCreateOpen] = useState(false);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [projectAdded, setProjectAdded] = useState(false);

  const selectedProject =
    workspace.projects.find(
      (project) => project.id === workspace.selectedProjectId,
    ) ?? null;
  const newSessionDisabledReason = getNewSessionDisabledReason(
    workspace.daemonConnected,
    selectedProject !== null,
  );
  const canCreateSession = newSessionDisabledReason === null;

  function runCommand(commandId: WorkspaceCommandId) {
    if (commandId === "project.add" && workspace.daemonConnected) {
      setProjectAdded(true);
    }
    if (commandId === "session.create" && canCreateSession) {
      setCreateOpen(true);
    }
    if (commandId === "worktree.remove") {
      setConfirmRemove(true);
    }
  }

  return (
    <div className="app-shell">
      <a className="skip-link" href="#workspace">
        Skip to workspace
      </a>
      <AppHeader
        newSessionDisabledReason={newSessionDisabledReason}
        onNewSession={() => setCreateOpen(true)}
        onOpenCommandPalette={() => setPaletteOpen(true)}
      />
      <ErrorBanner error={workspace.error} />
      {projectAdded ? (
        <p
          className="app-notice"
          role="status"
          aria-live="polite"
          aria-atomic="true"
        >
          Project picker is available from the desktop bridge.
        </p>
      ) : null}
      <div className="app-shell__body">
        <ProjectSidebar
          daemonConnected={workspace.daemonConnected}
          projects={workspace.projects}
          sessions={workspace.sessions}
          selectedProjectId={workspace.selectedProjectId}
          onSelectProject={(projectId) =>
            setWorkspace((current) => ({
              ...current,
              selectedProjectId: projectId,
            }))
          }
          onAddProject={() => runCommand("project.add")}
          onSelectSession={() => undefined}
        />
        {workspace.terminals.length > 0 ? (
          <main id="workspace" className="workspace" tabIndex={-1}>
            <header className="workspace__header">
              <div>
                <p className="workspace__eyebrow">Workspace</p>
                <h1>{selectedProject?.name ?? "Sessions"}</h1>
              </div>
              <span className="workspace__mode">Local</span>
            </header>
            <TerminalGrid terminals={workspace.terminals} ipc={ipc} />
          </main>
        ) : (
          <WorkspaceEmptyState selectedProject={selectedProject} />
        )}
      </div>
      <LocalStatusBar daemonConnected={workspace.daemonConnected} />
      <CommandPalette
        open={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        onRun={runCommand}
      />
      <CreateSessionDialog
        open={createOpen}
        projectName={selectedProject?.name ?? "this project"}
        onCancel={() => setCreateOpen(false)}
        onCreate={async (input) => {
          if (!ipc || !selectedProject) {
            setCreateOpen(false);
            return;
          }

          await ipc.createSession({
            projectId: selectedProject.id,
            name: input.name,
            agentId: input.agentId,
            isolateWorktree: input.isolateWorktree,
          });
          setCreateOpen(false);
        }}
      />
      <ConfirmDialog
        open={confirmRemove}
        title="Remove worktree"
        message="Remove the selected worktree from disk? CLI Master will refuse if it has uncommitted changes. This cannot be undone."
        confirmLabel="Remove worktree"
        onCancel={() => setConfirmRemove(false)}
        onConfirm={() => setConfirmRemove(false)}
      />
    </div>
  );
}

/** Explains the nearest action required before a session can be created. */
function getNewSessionDisabledReason(
  daemonConnected: boolean,
  hasSelectedProject: boolean,
): string | null {
  if (!daemonConnected && !hasSelectedProject) {
    return "Connect the local daemon and add a project first.";
  }

  if (!daemonConnected) {
    return "Connect the local daemon first.";
  }

  if (!hasSelectedProject) {
    return "Add a project first.";
  }

  return null;
}
