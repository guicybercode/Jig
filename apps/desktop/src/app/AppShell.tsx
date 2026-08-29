import { useCallback, useMemo, useState } from "react";

import { AgentApiProvider } from "../ipc/AgentApiProvider";
import { createMemoryAgentApi } from "../ipc/memoryAgentApi";
import type { IpcClient } from "../ipc";
import { AppHeader } from "./components/AppHeader";
import { AgentsView } from "./features/agents/AgentsView";
import {
  DiagnosticsDialog as ApplicationDiagnosticsDialog,
} from "./features/diagnostics/DiagnosticsDialog";
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

interface AppShellProps {
  readonly client?: IpcClient;
}

/** Composes the persistent desktop application regions. */
export function AppShell({ client }: AppShellProps) {
  const agentApi = useMemo(() => createMemoryAgentApi(), []);
  return (
    <WorkspaceProvider client={client}>
      <AgentApiProvider api={agentApi}>
        <AppShellLayout />
      </AgentApiProvider>
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
  const [view, setView] = useState<"workspace" | "agents">("workspace");

  return (
    <div className="app-shell">
      <a className="skip-link" href="#workspace">
        Skip to workspace
      </a>
      <AppHeader
        agentsActive={view === "agents"}
        onOpenAgents={() =>
          setView((current) => (current === "agents" ? "workspace" : "agents"))
        }
      />
      <div className="app-shell__body">
        <ProjectSidebar />
        {view === "agents" ? (
          <AgentsView hasProject={project !== null} />
        ) : project ? (
          <WorkspaceMain project={project} />
        ) : (
          <WorkspaceEmptyState />
        )}
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
      <ApplicationDiagnosticsDialog
        open={diagnosticsOpen}
        load={loadDiagnostics}
        onClose={() => {
          setDiagnosticsOpen(false);
        }}
      />
    </div>
  );
}
