import { useCallback, useState } from "react";

import { AppHeader } from "./components/AppHeader";
import { DiagnosticsDialog } from "./features/diagnostics/DiagnosticsDialog";
import { ProjectSidebar } from "./features/navigation/ProjectSidebar";
import { LocalStatusBar } from "./features/status/LocalStatusBar";
import { CustomAgentDialog } from "./features/workspace/CustomAgentDialog";
import { NewSessionDialog } from "./features/workspace/NewSessionDialog";
import { WorkspaceEmptyState } from "./features/workspace/WorkspaceEmptyState";
import { WorkspaceMain } from "./features/workspace/WorkspaceMain";
import {
  WorkspaceProvider,
  useNotifications,
  useSelectedProject,
  useWorkspaceActions,
} from "./workspace";
import type { IpcClient } from "../ipc";

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
  const project = useSelectedProject();
  const notifications = useNotifications();
  const actions = useWorkspaceActions();
  const [diagnosticsOpen, setDiagnosticsOpen] = useState(false);
  const loadDiagnostics = useCallback(
    () => actions.getDiagnostics(),
    [actions],
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
      <LocalStatusBar
        onOpenDiagnostics={() => {
          setDiagnosticsOpen(true);
        }}
      />
      {notifications.length > 0 ? (
        <ul className="notification-list" aria-label="Workspace notifications">
          {notifications.map((notification) => (
            <li key={notification.id}>
              <p
                className={
                  notification.kind === "error" ? "form-error" : "workspace__meta"
                }
                role={notification.kind === "error" ? "alert" : undefined}
              >
                {notification.message}
              </p>
              <button
                type="button"
                className="button button--secondary"
                onClick={() => actions.dismissNotification(notification.id)}
              >
                Dismiss
              </button>
            </li>
          ))}
        </ul>
      ) : null}
      <NewSessionDialog />
      <CustomAgentDialog />
      <DiagnosticsDialog
        open={diagnosticsOpen}
        load={loadDiagnostics}
        onClose={() => {
          setDiagnosticsOpen(false);
        }}
      />
    </div>
  );
}
