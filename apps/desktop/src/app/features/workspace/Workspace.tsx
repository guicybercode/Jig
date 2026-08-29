import type { AppPlatform } from "../../../ipc/client";
import type {
  AgentRecord,
  ApiErrorData,
  HelloResponse,
  Project,
  Session,
  StateSnapshot,
  Worktree,
} from "../../../ipc/types";
import { Icon } from "../../components/Icon";
import { StatusBadge } from "../../components/StatusBadge";
import { DiagnosticsView } from "../diagnostics/DiagnosticsView";
import { CanvasWorkspace } from "../canvas/CanvasWorkspace";
import { SessionWorkspace } from "../sessions/SessionWorkspace";

type WorkspaceViewName = "canvas" | "session" | "grid" | "settings" | "diagnostics";
type ConnectionStatus = "connecting" | "connected" | "disconnected" | "fatal";

interface WorkspaceProps {
  readonly connectionStatus: ConnectionStatus;
  readonly connectionError?: ApiErrorData;
  readonly hello?: HelloResponse;
  readonly snapshot?: StateSnapshot;
  readonly platform: AppPlatform;
  readonly view: WorkspaceViewName;
  readonly projects: readonly Project[];
  readonly project?: Project;
  readonly sessions: readonly Session[];
  readonly session?: Session;
  readonly agent?: AgentRecord;
  readonly worktree?: Worktree;
  readonly onRetry: () => void;
  readonly onAddProject: () => void;
  readonly onNewSession: () => void;
  readonly onOpenCanvas: () => void;
  readonly onSelectProject: (projectId: string) => void;
  readonly onSelectSession: (sessionId: string) => void;
  readonly onStartSession: (sessionId: string) => Promise<Session>;
  readonly onRestartSession: (sessionId: string) => Promise<Session>;
  readonly onRenameSession: (sessionId: string) => void;
  readonly onStopSession: (sessionId: string) => void;
  readonly onDeleteSession: (sessionId: string) => void;
  readonly onRemoveWorktree: (sessionId: string) => void;
  readonly onGitStatus: (sessionId: string) => void;
  readonly onOpenPath: (path: string) => Promise<void>;
  readonly onLoadDiagnostics: () => Promise<import("../../../ipc/types").DiagnosticsSnapshot>;
}

/** Selects the main experience for connection, project, session, and utility states. */
export function Workspace(props: WorkspaceProps) {
  if (props.connectionStatus === "connecting" && !props.snapshot) {
    return <LoadingWorkspace />;
  }
  if (props.connectionStatus === "fatal") {
    return <ConnectionWorkspace fatal error={props.connectionError} onRetry={props.onRetry} />;
  }
  if (props.connectionStatus === "disconnected" && !props.snapshot) {
    return <ConnectionWorkspace error={props.connectionError} onRetry={props.onRetry} />;
  }
  if (props.view === "settings") {
    return <SettingsWorkspace platform={props.platform} onOpenCanvas={props.onOpenCanvas} />;
  }
  if (props.view === "diagnostics") {
    return (
      <DiagnosticsView
        hello={props.hello}
        snapshot={props.snapshot}
        onOpenCanvas={props.onOpenCanvas}
        onLoad={props.onLoadDiagnostics}
        onRetryConnection={props.onRetry}
      />
    );
  }
  if (props.view === "canvas") {
    return (
      <CanvasWorkspace
        isConnected={props.connectionStatus === "connected"}
        projects={props.projects}
        project={props.project}
        sessions={props.sessions}
        onAddProject={props.onAddProject}
        onNewSession={props.onNewSession}
        onSelectSession={props.onSelectSession}
      />
    );
  }
  if (props.projects.length === 0) {
    return <NoProjectsWorkspace canMutate={props.connectionStatus === "connected"} onAddProject={props.onAddProject} />;
  }
  if (!props.project) {
    return <RecentProjectsWorkspace projects={props.projects} onSelectProject={props.onSelectProject} />;
  }
  if (props.view === "grid") {
    return <GridWorkspace project={props.project} sessions={props.sessions} onSelectSession={props.onSelectSession} />;
  }
  if (!props.session) {
    return <NoSessionsWorkspace canMutate={props.connectionStatus === "connected"} project={props.project} onNewSession={props.onNewSession} />;
  }
  return (
    <SessionWorkspace
      session={props.session}
      project={props.project}
      agent={props.agent}
      worktree={props.worktree}
      isConnected={props.connectionStatus === "connected"}
      onStart={props.onStartSession}
      onRestart={props.onRestartSession}
      onRename={props.onRenameSession}
      onStop={props.onStopSession}
      onDelete={props.onDeleteSession}
      onRemoveWorktree={props.onRemoveWorktree}
      onGitStatus={props.onGitStatus}
      onOpenPath={props.onOpenPath}
    />
  );
}

function LoadingWorkspace() {
  return (
    <main id="workspace" className="workspace" tabIndex={-1}>
      <div className="loading-workspace" role="status" aria-live="polite">
        <span className="spinner" aria-hidden="true" />
        <div><h1>Connecting to the local daemon</h1><p>Loading projects and sessions…</p></div>
      </div>
    </main>
  );
}

function ConnectionWorkspace({
  fatal = false,
  error,
  onRetry,
}: {
  readonly fatal?: boolean;
  readonly error?: ApiErrorData;
  readonly onRetry: () => void;
}) {
  return (
    <main id="workspace" className="workspace" tabIndex={-1}>
      <section className="empty-state empty-state--error" aria-labelledby="connection-title">
        <span className="empty-state__icon" aria-hidden="true"><Icon name="warning" /></span>
        <div className="empty-state__copy">
          <p className="workspace-header__eyebrow">{fatal ? "Incompatible local service" : "Local service offline"}</p>
          <h1 id="connection-title">{fatal ? "CLI Master cannot continue" : "Daemon disconnected"}</h1>
          <p>{error?.message ?? "The application could not connect to its local session daemon."}</p>
          {error?.action ? <p className="empty-state__action-copy">{error.action}</p> : null}
        </div>
        <button className="button button--primary" type="button" onClick={onRetry}>
          <Icon name="refresh" /> Retry Connection
        </button>
      </section>
    </main>
  );
}

function NoProjectsWorkspace({ canMutate, onAddProject }: { readonly canMutate: boolean; readonly onAddProject: () => void }) {
  return (
    <main id="workspace" className="workspace" tabIndex={-1}>
      <header className="workspace-header"><div><p className="workspace-header__eyebrow">Workspace</p><h1>No project selected</h1></div></header>
      <section className="empty-state" aria-labelledby="no-projects-title">
        <span className="empty-state__icon" aria-hidden="true"><Icon name="repository" /></span>
        <div className="empty-state__copy">
          <h2 id="no-projects-title">Add a repository to begin</h2>
          <p>The daemon validates the directory, resolves its repository root, and keeps only local project metadata.</p>
        </div>
        <button className="button button--primary" type="button" disabled={!canMutate} title={!canMutate ? "Reconnect the daemon first" : undefined} onClick={onAddProject}><Icon name="plus" /> Add Project</button>
      </section>
    </main>
  );
}

function RecentProjectsWorkspace({ projects, onSelectProject }: { readonly projects: readonly Project[]; readonly onSelectProject: (id: string) => void }) {
  return (
    <main id="workspace" className="workspace" tabIndex={-1}>
      <header className="workspace-header"><div><p className="workspace-header__eyebrow">Workspace</p><h1>Recent projects</h1></div></header>
      <div className="recent-projects">
        {projects.map((project) => (
          <button className="recent-project" type="button" key={project.id} onClick={() => onSelectProject(project.id)}>
            <Icon name="repository" /><span><strong>{project.name}</strong><small>{project.repositoryRoot ?? project.path}</small></span><Icon name="chevronRight" />
          </button>
        ))}
      </div>
    </main>
  );
}

function NoSessionsWorkspace({ canMutate, project, onNewSession }: { readonly canMutate: boolean; readonly project: Project; readonly onNewSession: () => void }) {
  const unavailable = project.availability === "missing" || project.availability === "not_repository";
  return (
    <main id="workspace" className="workspace" tabIndex={-1}>
      <header className="workspace-header">
        <div><p className="workspace-header__eyebrow">Project</p><h1>{project.name}</h1><p className="workspace-header__path">{project.repositoryRoot ?? project.path}</p></div>
        <span className={`project-health ${unavailable ? "project-health--error" : ""}`}>{unavailable ? "Repository unavailable" : project.currentBranch ?? "Repository ready"}</span>
      </header>
      {unavailable ? (
        <section className="empty-state empty-state--error" aria-labelledby="project-unavailable-title">
          <span className="empty-state__icon" aria-hidden="true"><Icon name="warning" /></span>
          <div className="empty-state__copy"><h2 id="project-unavailable-title">Project moved or is unavailable</h2><p>{project.availabilityMessage ?? "Remove this registration and add the repository again from its new location. Files are never deleted."}</p></div>
        </section>
      ) : (
        <section className="empty-state" aria-labelledby="no-sessions-title">
          <span className="empty-state__icon" aria-hidden="true"><Icon name="session" /></span>
          <div className="empty-state__copy"><h2 id="no-sessions-title">No sessions yet</h2><p>Create a session in the current working tree or an isolated Git worktree.</p></div>
          <button className="button button--primary" type="button" disabled={!canMutate} title={!canMutate ? "Reconnect the daemon first" : undefined} onClick={onNewSession}><Icon name="plus" /> New Session</button>
        </section>
      )}
    </main>
  );
}

function GridWorkspace({ project, sessions, onSelectSession }: { readonly project: Project; readonly sessions: readonly Session[]; readonly onSelectSession: (id: string) => void }) {
  return (
    <main id="workspace" className="workspace" tabIndex={-1}>
      <header className="workspace-header"><div><p className="workspace-header__eyebrow">{project.name}</p><h1>Session Grid</h1></div><span className="workspace-header__shortcut">⌘/Ctrl Shift G</span></header>
      {sessions.length === 0 ? <div className="empty-state"><p>No sessions are available for the grid.</p></div> : (
        <div className="session-grid" aria-label="Session grid">
          {sessions.map((session) => (
            <button className="session-grid__tile" type="button" key={session.id} onClick={() => onSelectSession(session.id)}>
              <span><strong>{session.name}</strong><StatusBadge status={session.status} compact /></span>
              <span className="terminal-grid-seam" data-terminal-root="true"><Icon name="terminal" /> Open session</span>
            </button>
          ))}
        </div>
      )}
    </main>
  );
}

function SettingsWorkspace({
  platform,
  onOpenCanvas,
}: {
  readonly platform: AppPlatform;
  readonly onOpenCanvas: () => void;
}) {
  const modifier = platform === "macos" ? "Command" : "Control";
  return (
    <main id="workspace" className="canvas-settings" tabIndex={-1}>
      <div className="canvas-settings__content">
        <button className="canvas-settings__back" type="button" onClick={onOpenCanvas}>
          <Icon name="chevronRight" /> Back to canvas
        </button>

        <header className="canvas-settings__header">
          <p>Workspace</p>
          <h1>Settings</h1>
          <span>Simple preferences and shortcuts for your local workspace.</span>
        </header>

        <div className="canvas-settings__grid">
          <section className="canvas-settings__section" aria-labelledby="settings-interface-title">
            <div className="canvas-settings__section-heading">
              <span aria-hidden="true"><Icon name="map" /></span>
              <div><h2 id="settings-interface-title">Interface</h2><p>How your workspace is presented.</p></div>
            </div>
            <dl className="canvas-settings__rows">
              <div><dt>Default workspace</dt><dd>Spatial canvas</dd></div>
              <div><dt>Navigation</dt><dd>Collapsible sidebar</dd></div>
              <div><dt>Canvas position</dt><dd>Saved automatically</dd></div>
            </dl>
          </section>

          <section className="canvas-settings__section" aria-labelledby="settings-local-title">
            <div className="canvas-settings__section-heading">
              <span aria-hidden="true"><Icon name="repository" /></span>
              <div><h2 id="settings-local-title">Local data</h2><p>Your project stays on this computer.</p></div>
            </div>
            <p className="canvas-settings__copy">CLI Master stores project references, sessions, and canvas layouts locally. Terminal output is not copied into application metadata.</p>
          </section>

          <section className="canvas-settings__section canvas-settings__section--wide" aria-labelledby="settings-keyboard-title">
            <div className="canvas-settings__section-heading">
              <span aria-hidden="true"><Icon name="terminal" /></span>
              <div><h2 id="settings-keyboard-title">Keyboard</h2><p>Move quickly without leaving the canvas.</p></div>
            </div>
            <dl className="canvas-settings__shortcuts">
              <div><dt>Command palette</dt><dd><kbd>{modifier}</kbd><kbd>K</kbd></dd></div>
              <div><dt>New session</dt><dd><kbd>{modifier}</kbd><kbd>T</kbd></dd></div>
              <div><dt>Session grid</dt><dd><kbd>{modifier}</kbd><kbd>Shift</kbd><kbd>G</kbd></dd></div>
              <div><dt>Focus session</dt><dd><kbd>{modifier}</kbd><kbd>1–9</kbd></dd></div>
            </dl>
            <p className="canvas-settings__note">Application shortcuts pause while a terminal owns keyboard focus.</p>
          </section>
        </div>
      </div>
    </main>
  );
}
