import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { AppHeader } from "./components/AppHeader";
import { Icon } from "./components/Icon";
import { CommandPalette, type CommandPaletteCommand } from "./features/commands/CommandPalette";
import { CanvasSidebar } from "./features/navigation/CanvasSidebar";
import { ProjectSidebar } from "./features/navigation/ProjectSidebar";
import { SessionSidebar } from "./features/navigation/SessionSidebar";
import { LocalStatusBar } from "./features/status/LocalStatusBar";
import { Workspace } from "./features/workspace/Workspace";
import { useGlobalShortcuts } from "./hooks/useGlobalShortcuts";
import { AppDialogs } from "./AppDialogs";
import { useWorkspace } from "./state/WorkspaceContext";
import { isLiveStatus } from "./utils";

const CANVAS_SIDEBAR_COLLAPSED_KEY = "cli-master.canvas.sidebar-collapsed";

/** Composes the persistent desktop regions around centralized metadata state. */
export function AppShell() {
  const workspace = useWorkspace();
  const [navigationOpen, setNavigationOpen] = useState(false);
  const [canvasSidebarCollapsed, setCanvasSidebarCollapsed] = useState(
    readCanvasSidebarCollapsed,
  );
  const navigationRef = useRef<HTMLDivElement>(null);
  const modifier = workspace.platform === "macos" ? "⌘" : "Ctrl";
  const repositoryUnavailable =
    workspace.selectedProject?.availability === "missing" ||
    workspace.selectedProject?.availability === "not_repository";
  const canCreateSession =
    workspace.isConnected &&
    workspace.selectedProject !== null &&
    !repositoryUnavailable;
  const orderedProjectSessions = useMemo(
    () =>
      [...workspace.projectSessions].sort(
        (left, right) => right.updatedAtMs - left.updatedAtMs,
      ),
    [workspace.projectSessions],
  );
  const selectedAgent = workspace.agents.find(
    (agent) => agent.id === workspace.selectedSession?.agentId,
  );
  const usesCanvasShell =
    workspace.view === "canvas" ||
    workspace.view === "settings" ||
    workspace.view === "diagnostics";
  const canvasSidebarView =
    workspace.view === "settings" || workspace.view === "diagnostics"
      ? workspace.view
      : "canvas";

  const openAddProject = useCallback(() => {
    if (workspace.isConnected) {
      workspace.openOverlay({ kind: "add-project" });
    }
  }, [workspace]);

  const openNewSession = useCallback(() => {
    if (canCreateSession && workspace.selectedProject) {
      workspace.openOverlay({
        kind: "new-session",
        projectId: workspace.selectedProject.id,
      });
    }
  }, [canCreateSession, workspace]);

  const openCommandPalette = useCallback(() => {
    workspace.openOverlay({ kind: "command-palette" });
  }, [workspace]);

  const selectProject = useCallback(
    (projectId: string) => {
      workspace.selectProject(projectId);
      workspace.setView("session");
      setNavigationOpen(false);
    },
    [workspace],
  );

  const selectCanvasProject = useCallback(
    (projectId: string) => {
      workspace.selectProject(projectId);
      workspace.setView("canvas");
      setNavigationOpen(false);
    },
    [workspace],
  );

  const selectSession = useCallback(
    (sessionId: string) => {
      workspace.selectSession(sessionId);
      workspace.setView("session");
      setNavigationOpen(false);
    },
    [workspace],
  );

  const cycleProject = useCallback(() => {
    if (!workspace.projects.length) return;
    const currentIndex = workspace.projects.findIndex(
      (project) => project.id === workspace.selectedProjectId,
    );
    const nextProject = workspace.projects[(currentIndex + 1) % workspace.projects.length];
    if (nextProject) selectProject(nextProject.id);
  }, [selectProject, workspace.projects, workspace.selectedProjectId]);

  const focusSessionByNumber = useCallback(
    (sessionNumber: number) => {
      const session = orderedProjectSessions[sessionNumber - 1];
      if (session) selectSession(session.id);
    },
    [orderedProjectSessions, selectSession],
  );

  useGlobalShortcuts({
    platform: workspace.platform,
    onOpenCommandPalette: openCommandPalette,
    onNewSession: openNewSession,
    onOpenGrid: () => {
      if (workspace.selectedProject) workspace.setView("grid");
    },
    onFocusSession: focusSessionByNumber,
  });

  useEffect(() => {
    try {
      localStorage.setItem(
        CANVAS_SIDEBAR_COLLAPSED_KEY,
        String(canvasSidebarCollapsed),
      );
    } catch {
      // The sidebar remains operable when local persistence is unavailable.
    }
  }, [canvasSidebarCollapsed]);

  useEffect(() => {
    if (!navigationOpen) return undefined;
    const compactNavigation = window.matchMedia("(max-width: 47.99rem)");
    if (!compactNavigation.matches) {
      return undefined;
    }
    const navigation = navigationRef.current;
    if (!navigation) return undefined;
    const previouslyFocused =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    const inertTargets = Array.from(
      document.querySelectorAll<HTMLElement>(
        ".app-header, #workspace, .status-bar, .connection-banner",
      ),
    );
    const priorAriaHidden = inertTargets.map((target) =>
      target.getAttribute("aria-hidden"),
    );
    for (const target of inertTargets) {
      target.setAttribute("inert", "");
      target.setAttribute("aria-hidden", "true");
    }
    firstFocusable(navigation)?.focus();

    function handleNavigationKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        setNavigationOpen(false);
        return;
      }
      if (event.key !== "Tab") return;
      const focusable = navigation
        ? navigationFocusable(navigation)
        : [];
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (!first || !last) {
        event.preventDefault();
        return;
      }
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    }

    function handleNavigationBreakpoint(event: MediaQueryListEvent) {
      if (!event.matches) setNavigationOpen(false);
    }

    window.addEventListener("keydown", handleNavigationKeyDown);
    compactNavigation.addEventListener("change", handleNavigationBreakpoint);
    return () => {
      window.removeEventListener("keydown", handleNavigationKeyDown);
      compactNavigation.removeEventListener("change", handleNavigationBreakpoint);
      inertTargets.forEach((target, index) => {
        target.removeAttribute("inert");
        const ariaHidden = priorAriaHidden[index];
        if (ariaHidden === null) target.removeAttribute("aria-hidden");
        else if (ariaHidden !== undefined) target.setAttribute("aria-hidden", ariaHidden);
      });
      if (previouslyFocused?.isConnected) previouslyFocused.focus();
    };
  }, [navigationOpen]);

  const commands = useMemo<readonly CommandPaletteCommand[]>(() => {
    const session = workspace.selectedSession;
    const baseCommands: CommandPaletteCommand[] = [
      {
        id: "project.add",
        label: "Add Project",
        description: "Register a local Git repository.",
        shortcut: undefined,
        disabled: !workspace.isConnected,
        disabledReason: "Connect the local daemon first.",
        onSelect: openAddProject,
      },
      {
        id: "project.switch",
        label: "Switch Project",
        description: "Move to the next recent project.",
        disabled: workspace.projects.length === 0,
        disabledReason: "Add a project first.",
        onSelect: cycleProject,
      },
      {
        id: "session.new",
        label: "New Session",
        description: "Configure an agent and working tree.",
        shortcut: `${modifier} T`,
        disabled: !canCreateSession,
        disabledReason: repositoryUnavailable
          ? "The selected repository is unavailable."
          : "Select a project and connect the daemon first.",
        onSelect: openNewSession,
      },
      {
        id: "session.open",
        label: "Open Session",
        description: "Open the selected session workspace.",
        disabled: !session,
        disabledReason: "Select a session first.",
        onSelect: () => workspace.setView("session"),
      },
      {
        id: "session.stop",
        label: "Stop Session",
        description: "Stop only the selected agent process.",
        disabled:
          !session || !workspace.isConnected || !isLiveStatus(session.status),
        disabledReason:
          "Select a live session and connect the local daemon first.",
        onSelect: () => {
          if (session) workspace.openOverlay({ kind: "stop-session", sessionId: session.id });
        },
      },
      {
        id: "session.restart",
        label: "Restart Session",
        description: "Restart the selected agent process.",
        disabled: !session || !workspace.isConnected,
        disabledReason: "Select a session and connect the daemon first.",
        onSelect: () => {
          if (session) {
            void workspace
              .restartSession({ sessionId: session.id })
              .catch(() => undefined);
          }
        },
      },
      {
        id: "view.canvas",
        label: "Open Canvas",
        description: "Arrange terminals and notes in the spatial workspace.",
        onSelect: () => workspace.setView("canvas"),
      },
      {
        id: "view.grid",
        label: "Open Grid",
        description: "Show sessions for the selected project in a grid.",
        shortcut: `${modifier} Shift G`,
        disabled: !workspace.selectedProject,
        disabledReason: "Select a project first.",
        onSelect: () => workspace.setView("grid"),
      },
      {
        id: "view.settings",
        label: "Open Settings",
        description: "Review application and keyboard settings.",
        onSelect: () => workspace.setView("settings"),
      },
      {
        id: "view.diagnostics",
        label: "Open Diagnostics",
        description: "Inspect daemon connection and local paths.",
        onSelect: () => workspace.setView("diagnostics"),
      },
    ];
    const switchCommands = workspace.projects.map((project) => ({
      id: `project.switch.${project.id}`,
      label: `Switch Project: ${project.name}`,
      description: project.repositoryRoot ?? project.path,
      keywords: ["recent repository", project.currentBranch ?? ""],
      onSelect: () => selectProject(project.id),
    }));
    return [...baseCommands, ...switchCommands];
  }, [
    canCreateSession,
    cycleProject,
    modifier,
    openAddProject,
    openNewSession,
    repositoryUnavailable,
    selectProject,
    workspace,
  ]);

  return (
    <div
      className={
        usesCanvasShell
          ? `app-shell app-shell--canvas${
              canvasSidebarCollapsed
                ? " app-shell--canvas-sidebar-collapsed"
                : ""
            }`
          : "app-shell"
      }
    >
      <a className="skip-link" href="#workspace">Skip to workspace</a>
      <AppHeader
        project={workspace.selectedProject ?? undefined}
        platform={workspace.platform}
        canCreateSession={canCreateSession}
        navigationOpen={navigationOpen}
        onToggleNavigation={() => setNavigationOpen((open) => !open)}
        onNewSession={openNewSession}
        onOpenPalette={openCommandPalette}
      />
      {workspace.connection.status === "disconnected" && workspace.snapshot ? (
        <div className="connection-banner" role="alert">
          <span><strong>Daemon disconnected.</strong> Existing metadata may be stale.</span>
          <button className="button button--secondary" type="button" onClick={workspace.retry}>Reconnect</button>
        </div>
      ) : null}
      {workspace.operationError ? (
        <div className="connection-banner connection-banner--operation" role="alert">
          <span>
            <strong>{workspace.operationError.message}</strong>{" "}
            {workspace.operationError.action}
          </span>
          <button
            className="icon-button icon-button--small"
            type="button"
            aria-label="Dismiss operation error"
            onClick={workspace.clearOperationError}
          >
            <Icon name="close" />
          </button>
        </div>
      ) : null}
      <div className="app-shell__body">
        <div
          ref={navigationRef}
          className="navigation-pane"
          data-open={navigationOpen ? "true" : "false"}
          aria-hidden={
            usesCanvasShell && canvasSidebarCollapsed
              ? true
              : undefined
          }
          inert={
            usesCanvasShell && canvasSidebarCollapsed
              ? true
              : undefined
          }
        >
          <div className="navigation-pane__mobile-header">
            <strong>Navigation</strong>
            <button className="icon-button" type="button" aria-label="Close navigation" onClick={() => setNavigationOpen(false)}>
              <Icon name="close" />
            </button>
          </div>
          {usesCanvasShell ? (
            <CanvasSidebar
              projects={workspace.projects}
              sessions={workspace.sessions}
              selectedProjectId={workspace.selectedProjectId ?? undefined}
              activeView={canvasSidebarView}
              canManageProjects={workspace.isConnected}
              onSelectProject={selectCanvasProject}
              onOpenCanvas={() => {
                workspace.setView("canvas");
                setNavigationOpen(false);
              }}
              onHide={() => setCanvasSidebarCollapsed(true)}
              onAddProject={openAddProject}
              onOpenSettings={() => {
                workspace.setView("settings");
                setNavigationOpen(false);
              }}
              onOpenDiagnostics={() => {
                workspace.setView("diagnostics");
                setNavigationOpen(false);
              }}
            />
          ) : (
            <>
              <ProjectSidebar
                projects={workspace.projects}
                selectedProjectId={workspace.selectedProjectId ?? undefined}
                canManageProjects={workspace.isConnected}
                onSelectProject={selectProject}
                onAddProject={openAddProject}
                onRenameProject={(projectId) => workspace.openOverlay({ kind: "rename-project", projectId })}
                onRemoveProject={(projectId) => workspace.openOverlay({ kind: "remove-project", projectId })}
                onOpenSettings={() => { workspace.setView("settings"); setNavigationOpen(false); }}
                onOpenDiagnostics={() => { workspace.setView("diagnostics"); setNavigationOpen(false); }}
              />
              <SessionSidebar
                sessions={workspace.projectSessions}
                agents={workspace.agents}
                canCreateSession={canCreateSession}
                canManageWorktrees={workspace.isConnected}
                worktrees={workspace.worktrees.filter(
                  (worktree) =>
                    worktree.projectId === workspace.selectedProject?.id,
                )}
                selectedSessionId={workspace.selectedSessionId ?? undefined}
                projectSelected={workspace.selectedProject !== null}
                onSelectSession={selectSession}
                onNewSession={openNewSession}
                onRemoveWorktree={(worktreeId) =>
                  workspace.openOverlay({ kind: "remove-worktree", worktreeId })
                }
              />
            </>
          )}
        </div>
        {usesCanvasShell && canvasSidebarCollapsed ? (
          <button
            className="canvas-sidebar-reveal"
            type="button"
            aria-label="Show workspace sidebar"
            title="Show sidebar"
            onClick={() => setCanvasSidebarCollapsed(false)}
          >
            <Icon name="sidebar" />
          </button>
        ) : null}
        {navigationOpen ? <button className="navigation-backdrop" type="button" aria-label="Dismiss navigation" onClick={() => setNavigationOpen(false)} /> : null}
        <Workspace
          connectionStatus={workspace.connection.status}
          connectionError={workspace.connection.status === "disconnected" || workspace.connection.status === "fatal" ? workspace.connection.error : undefined}
          hello={workspace.hello ?? undefined}
          snapshot={workspace.snapshot ?? undefined}
          platform={workspace.platform}
          view={workspace.view}
          projects={workspace.projects}
          project={workspace.selectedProject ?? undefined}
          sessions={workspace.projectSessions}
          session={workspace.selectedSession ?? undefined}
          agent={selectedAgent}
          worktree={workspace.selectedWorktree ?? undefined}
          onRetry={workspace.retry}
          onAddProject={openAddProject}
          onNewSession={openNewSession}
          onOpenCanvas={() => workspace.setView("canvas")}
          onSelectProject={selectProject}
          onSelectSession={selectSession}
          onStartSession={(sessionId) => workspace.startSession({ sessionId })}
          onRestartSession={(sessionId) => workspace.restartSession({ sessionId })}
          onRenameSession={(sessionId) => workspace.openOverlay({ kind: "rename-session", sessionId })}
          onStopSession={(sessionId) => workspace.openOverlay({ kind: "stop-session", sessionId })}
          onDeleteSession={(sessionId) => workspace.openOverlay({ kind: "delete-session", sessionId })}
          onRemoveWorktree={(sessionId) => {
            const session = workspace.sessions.find((candidate) => candidate.id === sessionId);
            if (session?.worktreeId) workspace.openOverlay({ kind: "remove-worktree", worktreeId: session.worktreeId });
          }}
          onGitStatus={(sessionId) => workspace.openOverlay({ kind: "git-status", sessionId })}
          onOpenPath={workspace.openPath}
          onLoadDiagnostics={workspace.getDiagnostics}
        />
      </div>
      <LocalStatusBar
        connection={workspace.connection.status}
        hello={workspace.hello ?? undefined}
        project={workspace.selectedProject ?? undefined}
        sessions={workspace.sessions}
        selectedWorktree={workspace.selectedWorktree ?? undefined}
        onOpenDiagnostics={() => workspace.setView("diagnostics")}
      />
      <CommandPalette
        open={workspace.overlay?.kind === "command-palette"}
        commands={commands}
        onClose={workspace.closeOverlay}
      />
      <AppDialogs />
    </div>
  );
}

function readCanvasSidebarCollapsed(): boolean {
  try {
    return localStorage.getItem(CANVAS_SIDEBAR_COLLAPSED_KEY) === "true";
  } catch {
    return false;
  }
}

const NAVIGATION_FOCUSABLE = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "[tabindex]:not([tabindex='-1'])",
].join(",");

function navigationFocusable(container: HTMLElement): readonly HTMLElement[] {
  return Array.from(
    container.querySelectorAll<HTMLElement>(NAVIGATION_FOCUSABLE),
  ).filter(
    (element) =>
      element.getAttribute("aria-hidden") !== "true" &&
      element.getClientRects().length > 0,
  );
}

function firstFocusable(container: HTMLElement): HTMLElement | undefined {
  return navigationFocusable(container)[0];
}
