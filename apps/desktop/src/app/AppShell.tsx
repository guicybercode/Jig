import { useEffect, useMemo, useState } from "react";

import { AgentApiProvider } from "../ipc/AgentApiProvider";
import { createMemoryAgentApi } from "../ipc/memoryAgentApi";
import type { IpcClient } from "../ipc";
import { AppHeader } from "./components/AppHeader";
import { AgentsView } from "./features/agents/AgentsView";
import {
  CommandPalette,
  type CommandPaletteCommand,
} from "./features/commands/CommandPalette";
import { ProjectSidebar } from "./features/navigation/ProjectSidebar";
import { LocalStatusBar } from "./features/status/LocalStatusBar";
import { CustomAgentDialog } from "./features/workspace/CustomAgentDialog";
import { NewSessionDialog } from "./features/workspace/NewSessionDialog";
import { WorkspaceEmptyState } from "./features/workspace/WorkspaceEmptyState";
import { WorkspaceMain } from "./features/workspace/WorkspaceMain";
import {
  WorkspaceProvider,
  useDaemonReady,
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
  const daemonReady = useDaemonReady();
  const notifications = useNotifications();
  const actions = useWorkspaceActions();
  const [view, setView] = useState<"workspace" | "agents">("workspace");
  const [commandPaletteOpen, setCommandPaletteOpen] = useState(false);

  useEffect(() => {
    function handleShortcut(event: KeyboardEvent) {
      if (!isCommandPaletteShortcut(event)) {
        return;
      }
      event.preventDefault();
      setCommandPaletteOpen(true);
    }

    window.addEventListener("keydown", handleShortcut);
    return () => window.removeEventListener("keydown", handleShortcut);
  }, []);

  const commandPaletteCommands: readonly CommandPaletteCommand[] = [
    {
      id: "session.create",
      label: "New Session",
      disabled: !daemonReady || project === null,
      disabledReason: "Connect the daemon and select a project first.",
      onSelect: () => actions.openDialog("newSession"),
    },
    {
      id: "agents.open",
      label: "Open Agents",
      onSelect: () => setView("agents"),
    },
    {
      id: "workspace.refresh",
      label: "Refresh Workspace",
      disabled: !daemonReady,
      disabledReason: "Connect the local daemon first.",
      onSelect: () => void actions.refresh(),
    },
    {
      id: "daemon.reconnect",
      label: "Reconnect to Daemon",
      onSelect: () => void actions.reconnect(),
    },
  ];

  return (
    <div className="app-shell">
      <a className="skip-link" href="#workspace">
        Skip to workspace
      </a>
      <AppHeader
        commandPaletteOpen={commandPaletteOpen}
        onOpenCommandPalette={() => setCommandPaletteOpen(true)}
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
      <LocalStatusBar />
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
      <CommandPalette
        open={commandPaletteOpen}
        commands={commandPaletteCommands}
        onClose={() => setCommandPaletteOpen(false)}
      />
    </div>
  );
}

/** Preserves terminal and text-editing chords while exposing the global launcher. */
function isCommandPaletteShortcut(event: KeyboardEvent): boolean {
  if (
    event.defaultPrevented ||
    event.isComposing ||
    event.altKey ||
    event.shiftKey ||
    event.key.toLocaleLowerCase() !== "k" ||
    event.metaKey === event.ctrlKey
  ) {
    return false;
  }

  const target = event.target;
  if (!(target instanceof HTMLElement)) {
    return true;
  }
  if (
    target.isContentEditable ||
    target.closest("input, textarea, select, [contenteditable], [role='dialog']")
  ) {
    return false;
  }
  return !(
    event.ctrlKey && target.closest("[data-terminal-root='true'], .xterm")
  );
}
