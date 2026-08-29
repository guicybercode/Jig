import { AppHeader } from "./components/AppHeader";
import { ProjectSidebar } from "./features/navigation/ProjectSidebar";
import { LocalStatusBar } from "./features/status/LocalStatusBar";
import { WorkspaceEmptyState } from "./features/workspace/WorkspaceEmptyState";
import { WorkspaceMain } from "./features/workspace/WorkspaceMain";
import { WorkspaceProvider, useWorkspace } from "./workspace/WorkspaceContext";
import type { IpcClient } from "../lib/ipc";

interface AppShellProps {
  readonly client?: IpcClient;
}

/** Composes the persistent desktop application regions. */
export function AppShell({ client }: AppShellProps) {
  return (
    <WorkspaceProvider client={client}>
      <AppShellLayout />
    </WorkspaceProvider>
  );
}

function AppShellLayout() {
  const workspace = useWorkspace();
  const project = workspace.projects.find(
    (item) => item.id === workspace.selectedProjectId,
  );

  return (
    <div className="app-shell">
      <a className="skip-link" href="#workspace">
        Skip to workspace
      </a>
      <AppHeader />
      <div className="app-shell__body">
        <ProjectSidebar />
        {project ? <WorkspaceMain project={project} /> : <WorkspaceEmptyState />}
      </div>
      <LocalStatusBar />
    </div>
  );
}
