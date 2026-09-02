import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { Icon } from "./components/Icon";
import { CommandPalette, type CommandPaletteCommand } from "./features/commands/CommandPalette";
import { CanvasSidebar } from "./features/navigation/CanvasSidebar";
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
  const [sessionFocusRevision, setSessionFocusRevision] = useState(0);
  const closeNavigationOnDesktop = useCallback(
    () => setNavigationOpen(false),
    [],
  );
  const isCompactNavigation = useCompactNavigation(closeNavigationOnDesktop);
  const navigationRef = useRef<HTMLDivElement>(null);
  const navigationTriggerRef = useRef<HTMLButtonElement>(null);
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
  const canvasSidebarView =
    workspace.view === "settings" || workspace.view === "diagnostics"
      ? workspace.view
      : "canvas";

  const closeNavigationForOverlay = useCallback(() => {
    if (navigationOpen && isCompactNavigation) {
      setNavigationOpen(false);
    }
  }, [isCompactNavigation, navigationOpen]);

  const openAddProject = useCallback(() => {
    if (workspace.isConnected) {
      closeNavigationForOverlay();
      workspace.openOverlay({ kind: "add-project" });
    }
  }, [closeNavigationForOverlay, workspace]);

  const openNewSession = useCallback(() => {
    if (canCreateSession && workspace.selectedProject) {
      closeNavigationForOverlay();
      workspace.openOverlay({
        kind: "new-session",
        projectId: workspace.selectedProject.id,
      });
    }
  }, [canCreateSession, closeNavigationForOverlay, workspace]);

  const openCommandPalette = useCallback(() => {
    closeNavigationForOverlay();
    workspace.openOverlay({ kind: "command-palette" });
  }, [closeNavigationForOverlay, workspace]);

  const selectCanvasProject = useCallback(
    (projectId: string) => {
      workspace.selectProject(projectId);
      workspace.setView("canvas");
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
    if (nextProject) selectCanvasProject(nextProject.id);
  }, [selectCanvasProject, workspace.projects, workspace.selectedProjectId]);

  const requestSessionFocus = useCallback(
    (sessionId: string) => {
      workspace.selectSession(sessionId);
      workspace.setView("canvas");
      setNavigationOpen(false);
      setSessionFocusRevision((revision) => revision + 1);
    },
    [workspace],
  );

  const focusSessionByNumber = useCallback(
    (sessionNumber: number) => {
      const session = orderedProjectSessions[sessionNumber - 1];
      if (session) requestSessionFocus(session.id);
    },
    [orderedProjectSessions, requestSessionFocus],
  );

  useGlobalShortcuts({
    platform: workspace.platform,
    onOpenCommandPalette: openCommandPalette,
    onNewSession: openNewSession,
    onFocusSession: focusSessionByNumber,
  });

  const toggleNavigation = useCallback(() => {
    if (isCompactNavigation) {
      setNavigationOpen((open) => !open);
      return;
    }
    setCanvasSidebarCollapsed((collapsed) => !collapsed);
  }, [isCompactNavigation]);

  const hideNavigation = useCallback(() => {
    if (isCompactNavigation) {
      setNavigationOpen(false);
      return;
    }
    setCanvasSidebarCollapsed(true);
    queueMicrotask(() => navigationTriggerRef.current?.focus());
  }, [isCompactNavigation]);

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
    if (!navigationOpen || !isCompactNavigation) {
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
        ".skip-link, #workspace, .canvas-session-actions, .status-bar, .connection-banner",
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

    window.addEventListener("keydown", handleNavigationKeyDown);
    return () => {
      window.removeEventListener("keydown", handleNavigationKeyDown);
      inertTargets.forEach((target, index) => {
        target.removeAttribute("inert");
        const ariaHidden = priorAriaHidden[index];
        if (ariaHidden === null) target.removeAttribute("aria-hidden");
        else if (ariaHidden !== undefined) target.setAttribute("aria-hidden", ariaHidden);
      });
      if (previouslyFocused?.isConnected) {
        previouslyFocused.focus();
      }
    };
  }, [isCompactNavigation, navigationOpen]);

  useEffect(() => {
    const suppressWebviewContextMenu = (event: MouseEvent) => {
      event.preventDefault();
    };
    document.addEventListener("contextmenu", suppressWebviewContextMenu);
    return () => {
      document.removeEventListener("contextmenu", suppressWebviewContextMenu);
    };
  }, []);

  const commands = useMemo<readonly CommandPaletteCommand[]>(() => {
    const session = workspace.selectedSession;
    const project = workspace.selectedProject;
    const worktree = workspace.selectedWorktree;
    const hasMissingManagedWorktree = Boolean(session?.worktreeId && !worktree);
    const sessionPath = hasMissingManagedWorktree
      ? undefined
      : worktree?.path ?? session?.worktreePath ?? session?.cwd;
    const isSessionLive = session ? isLiveStatus(session.status) : false;
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
        id: "project.rename",
        label: "Rename Project",
        description: "Change only the selected project's display name.",
        disabled: !project || !workspace.isConnected,
        disabledReason: "Select a project and connect the local daemon first.",
        onSelect: () => {
          if (project) {
            workspace.openOverlay({ kind: "rename-project", projectId: project.id });
          }
        },
      },
      {
        id: "project.remove",
        label: "Remove Project",
        description: "Forget the selected project without deleting its directory.",
        disabled: !project || !workspace.isConnected,
        disabledReason: "Select a project and connect the local daemon first.",
        onSelect: () => {
          if (project) {
            workspace.openOverlay({ kind: "remove-project", projectId: project.id });
          }
        },
      },
      {
        id: "session.start",
        label: "Start Session",
        description: "Start a fresh process for the selected session.",
        disabled:
          !session ||
          !workspace.isConnected ||
          isSessionLive ||
          hasMissingManagedWorktree,
        disabledReason: hasMissingManagedWorktree
          ? "The selected session's managed worktree is unavailable."
          : "Select a stopped session and connect the local daemon first.",
        onSelect: () => {
          if (session) {
            void workspace.startSession({ sessionId: session.id }).catch(() => undefined);
          }
        },
      },
      {
        id: "session.stop",
        label: "Stop Session",
        description: "Stop only the selected agent process.",
        disabled:
          !session || !workspace.isConnected || !isSessionLive,
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
        disabled: !session || !workspace.isConnected || hasMissingManagedWorktree,
        disabledReason: hasMissingManagedWorktree
          ? "The selected session's managed worktree is unavailable."
          : "Select a session and connect the daemon first.",
        onSelect: () => {
          if (session) {
            void workspace
              .restartSession({ sessionId: session.id })
              .catch(() => undefined);
          }
        },
      },
      {
        id: "session.rename",
        label: "Rename Session",
        description: "Change only the selected session's display name.",
        disabled: !session || !workspace.isConnected,
        disabledReason: "Select a session and connect the local daemon first.",
        onSelect: () => {
          if (session) {
            workspace.openOverlay({ kind: "rename-session", sessionId: session.id });
          }
        },
      },
      {
        id: "session.git-status",
        label: "Show Git Status",
        description: "Inspect the selected session's working tree.",
        disabled: !session || !workspace.isConnected || hasMissingManagedWorktree,
        disabledReason: hasMissingManagedWorktree
          ? "The selected session's managed worktree is unavailable."
          : "Select a session and connect the local daemon first.",
        onSelect: () => {
          if (session) {
            workspace.openOverlay({ kind: "git-status", sessionId: session.id });
          }
        },
      },
      {
        id: "session.open-path",
        label: "Open Session Path",
        description: "Reveal the selected session directory in the system file manager.",
        disabled: !sessionPath,
        disabledReason: hasMissingManagedWorktree
          ? "The selected session's managed worktree is unavailable."
          : "Select a session first.",
        onSelect: () => {
          if (sessionPath) {
            void workspace.openPath(sessionPath).catch(() => undefined);
          }
        },
      },
      {
        id: "session.delete",
        label: "Delete Session",
        description: "Delete stopped session metadata without removing its worktree.",
        disabled: !session || !workspace.isConnected || isSessionLive,
        disabledReason: isSessionLive
          ? "Stop the selected session before deleting its metadata."
          : "Select a stopped session and connect the local daemon first.",
        onSelect: () => {
          if (session) {
            workspace.openOverlay({ kind: "delete-session", sessionId: session.id });
          }
        },
      },
      {
        id: "session.remove-worktree",
        label: "Remove Session Worktree",
        description: "Begin the guarded removal flow for the selected worktree.",
        disabled: !session || !worktree || !workspace.isConnected || isSessionLive,
        disabledReason: isSessionLive
          ? "Stop the selected session before removing its worktree."
          : "Select a stopped session with an available managed worktree.",
        onSelect: () => {
          if (worktree) {
            workspace.openOverlay({ kind: "remove-worktree", worktreeId: worktree.id });
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
      onSelect: () => selectCanvasProject(project.id),
    }));
    const focusCommands: CommandPaletteCommand[] = orderedProjectSessions.map(
      (projectSession, index) => ({
        id: `session.focus.${projectSession.id}`,
        label: `Focus Session: ${projectSession.name}`,
        description: "Reveal and focus this terminal node on the canvas.",
        shortcut: index < 9 ? `${modifier} ${index + 1}` : undefined,
        onSelect: () => requestSessionFocus(projectSession.id),
      }),
    );
    const sessionIds = new Set(workspace.sessions.map(({ id }) => id));
    const retainedWorktreeCommands: CommandPaletteCommand[] = workspace.worktrees
      .filter(
        (candidate) =>
          candidate.projectId === project?.id &&
          (!candidate.sessionId || !sessionIds.has(candidate.sessionId)),
      )
      .map((candidate) => ({
        id: `worktree.remove.${candidate.id}`,
        label: `Remove Retained Worktree: ${candidate.branch}`,
        description: candidate.path,
        disabled: !workspace.isConnected,
        disabledReason: "Connect the local daemon first.",
        onSelect: () =>
          workspace.openOverlay({
            kind: "remove-worktree",
            worktreeId: candidate.id,
          }),
      }));
    return [
      ...baseCommands,
      ...switchCommands,
      ...focusCommands,
      ...retainedWorktreeCommands,
    ];
  }, [
    canCreateSession,
    cycleProject,
    modifier,
    openAddProject,
    openNewSession,
    orderedProjectSessions,
    repositoryUnavailable,
    requestSessionFocus,
    selectCanvasProject,
    workspace,
  ]);

  const isNavigationHidden = isCompactNavigation
    ? !navigationOpen
    : canvasSidebarCollapsed;

  return (
    <div
      className={`app-shell${
        canvasSidebarCollapsed ? " app-shell--canvas-sidebar-collapsed" : ""
      }`}
    >
      <a className="skip-link" href="#workspace">Skip to workspace</a>
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
          id="canvas-navigation"
          ref={navigationRef}
          className="navigation-pane"
          data-open={navigationOpen ? "true" : "false"}
          role={isCompactNavigation ? "dialog" : undefined}
          aria-label={isCompactNavigation ? "Workspace navigation" : undefined}
          aria-modal={isCompactNavigation && navigationOpen ? true : undefined}
          aria-hidden={isNavigationHidden ? true : undefined}
          inert={isNavigationHidden ? true : undefined}
        >
          <div className="navigation-pane__mobile-header">
            <strong>Navigation</strong>
            <button className="icon-button" type="button" aria-label="Close navigation" onClick={() => setNavigationOpen(false)}>
              <Icon name="close" />
            </button>
          </div>
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
            onHide={hideNavigation}
            onAddProject={openAddProject}
            onRenameProject={(projectId) => {
              closeNavigationForOverlay();
              workspace.openOverlay({ kind: "rename-project", projectId });
            }}
            onRemoveProject={(projectId) => {
              closeNavigationForOverlay();
              workspace.openOverlay({ kind: "remove-project", projectId });
            }}
            onOpenSettings={() => {
              workspace.setView("settings");
              setNavigationOpen(false);
            }}
            onOpenDiagnostics={() => {
              workspace.setView("diagnostics");
              setNavigationOpen(false);
            }}
          />
        </div>
        {navigationOpen ? <button className="navigation-backdrop" type="button" aria-label="Dismiss navigation" onClick={() => setNavigationOpen(false)} /> : null}
        <div className="canvas-session-actions" role="toolbar" aria-label="Workspace actions">
          <button
            ref={navigationTriggerRef}
            className="canvas-tool canvas-navigation-trigger"
            type="button"
            aria-label={
              isNavigationHidden
                ? isCompactNavigation
                  ? "Open navigation"
                  : "Show workspace sidebar"
                : "Hide navigation"
            }
            aria-controls="canvas-navigation"
            aria-expanded={!isNavigationHidden}
            onClick={toggleNavigation}
          >
            <Icon name="menu" />
          </button>
          <button
            className="canvas-tool"
            type="button"
            aria-label={`Open command palette, ${modifier}+K`}
            title={`Open command palette (${modifier}+K)`}
            onClick={openCommandPalette}
          >
            <Icon name="search" />
          </button>
          {workspace.selectedProject ? (
            <button
              className="canvas-pill-button"
              type="button"
              aria-label="New Session"
              aria-disabled={!canCreateSession ? true : undefined}
              title={
                canCreateSession
                  ? "Create a session"
                  : "Select an available project and connect the daemon first."
              }
              onClick={openNewSession}
            >
              <Icon name="plus" /> <span>New session</span>
            </button>
          ) : (
            <button
              className="canvas-pill-button"
              type="button"
              aria-label="Add Project"
              aria-disabled={!workspace.isConnected ? true : undefined}
              title={
                workspace.isConnected
                  ? "Add a project"
                  : "Reconnect the daemon to add a project."
              }
              onClick={openAddProject}
            >
              <Icon name="plus" /> <span>Add project</span>
            </button>
          )}
        </div>
        <Workspace
          isCompact={isCompactNavigation}
          connectionStatus={workspace.connection.status}
          connectionError={workspace.connection.status === "disconnected" || workspace.connection.status === "fatal" ? workspace.connection.error : undefined}
          hello={workspace.hello ?? undefined}
          snapshot={workspace.snapshot ?? undefined}
          platform={workspace.platform}
          view={workspace.view}
          projects={workspace.projects}
          project={workspace.selectedProject ?? undefined}
          agents={workspace.agents}
          sessions={workspace.sessions}
          worktrees={workspace.worktrees}
          selectedSessionId={workspace.selectedSessionId ?? undefined}
          sessionFocusRevision={sessionFocusRevision}
          onRetry={workspace.retry}
          onOpenCanvas={() => workspace.setView("canvas")}
          onSelectSession={(sessionId) => workspace.selectSession(sessionId)}
          onCreateCustomAgent={workspace.createCustomAgent}
          onCreateSession={(input) =>
            workspace.createSession(input, { select: false })
          }
          onStartSession={(sessionId) => workspace.startSession({ sessionId })}
          subscribeTerminal={workspace.subscribeTerminal}
          writeTerminal={workspace.writeTerminal}
          resizeTerminal={workspace.resizeTerminal}
          onRestartSession={(sessionId) => workspace.restartSession({ sessionId })}
          onRenameSession={(sessionId) => workspace.openOverlay({ kind: "rename-session", sessionId })}
          onStopSession={(sessionId) => workspace.openOverlay({ kind: "stop-session", sessionId })}
          onDeleteSession={(sessionId) => workspace.openOverlay({ kind: "delete-session", sessionId })}
          onRemoveWorktree={(worktreeId) =>
            workspace.openOverlay({ kind: "remove-worktree", worktreeId })
          }
          onGitStatus={(sessionId) => workspace.openOverlay({ kind: "git-status", sessionId })}
          onOpenPath={workspace.openPath}
          onLoadDiagnostics={workspace.getDiagnostics}
        />
      </div>
      <LocalStatusBar
        connection={workspace.connection.status}
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

const COMPACT_NAVIGATION_QUERY = "(max-width: 47.99rem)";

/** Keeps drawer behavior aligned with the CSS compact-navigation breakpoint. */
function useCompactNavigation(onExitCompact: () => void): boolean {
  const [isCompact, setIsCompact] = useState(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
      return false;
    }
    return window.matchMedia(COMPACT_NAVIGATION_QUERY).matches;
  });

  useEffect(() => {
    if (typeof window.matchMedia !== "function") {
      return undefined;
    }
    const query = window.matchMedia(COMPACT_NAVIGATION_QUERY);
    const update = (event: MediaQueryListEvent) => {
      setIsCompact(event.matches);
      if (!event.matches) {
        onExitCompact();
      }
    };
    query.addEventListener("change", update);
    return () => query.removeEventListener("change", update);
  }, [onExitCompact]);

  return isCompact;
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
