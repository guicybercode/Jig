import { AppHeader } from "./components/AppHeader";
import { ProjectSidebar } from "./features/navigation/ProjectSidebar";
import { LocalStatusBar } from "./features/status/LocalStatusBar";
import { WorkspaceEmptyState } from "./features/workspace/WorkspaceEmptyState";

/** Composes the persistent desktop application regions. */
export function AppShell() {
  return (
    <div className="app-shell">
      <a className="skip-link" href="#workspace">
        Skip to workspace
      </a>
      <AppHeader />
      <div className="app-shell__body">
        <ProjectSidebar />
        <WorkspaceEmptyState />
      </div>
      <LocalStatusBar />
    </div>
  );
}
