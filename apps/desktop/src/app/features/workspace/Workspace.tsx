import type { AppPlatform } from "../../../ipc/client";
import type {
  AgentRecord,
  ApiErrorData,
  CreateCustomAgentInput,
  CreateSessionInput,
  HelloResponse,
  Project,
  Session,
  StateSnapshot,
  Worktree,
} from "../../../ipc/types";
import { Icon } from "../../components/Icon";
import type { WorkspaceView } from "../../state/WorkspaceContext";
import { DiagnosticsView } from "../diagnostics/DiagnosticsView";
import { CanvasWorkspace } from "../canvas/CanvasWorkspace";
import type { LiveTerminalTransport } from "../terminal/LiveTerminal";

type ConnectionStatus = "connecting" | "connected" | "disconnected" | "fatal";

interface WorkspaceProps extends LiveTerminalTransport {
  readonly isCompact: boolean;
  readonly connectionStatus: ConnectionStatus;
  readonly connectionError?: ApiErrorData;
  readonly hello?: HelloResponse;
  readonly snapshot?: StateSnapshot;
  readonly platform: AppPlatform;
  readonly view: WorkspaceView;
  readonly projects: readonly Project[];
  readonly project?: Project;
  readonly sessions: readonly Session[];
  readonly agents: readonly AgentRecord[];
  readonly worktrees: readonly Worktree[];
  readonly selectedSessionId?: string;
  readonly sessionFocusRevision: number;
  readonly onRetry: () => void;
  readonly onOpenCanvas: () => void;
  readonly onSelectSession: (sessionId: string | null) => void;
  readonly onStartSession: (sessionId: string) => Promise<Session>;
  readonly onCreateCustomAgent: (input: CreateCustomAgentInput) => Promise<AgentRecord>;
  readonly onCreateSession: (input: CreateSessionInput) => Promise<Session>;
  readonly onRestartSession: (sessionId: string) => Promise<Session>;
  readonly onRenameSession: (sessionId: string) => void;
  readonly onStopSession: (sessionId: string) => void;
  readonly onDeleteSession: (sessionId: string) => void;
  readonly onRemoveWorktree: (worktreeId: string) => void;
  readonly onGitStatus: (sessionId: string) => void;
  readonly onOpenPath: (path: string) => Promise<void>;
  readonly onLoadDiagnostics: () => Promise<import("../../../ipc/types").DiagnosticsSnapshot>;
}

/** Selects the canvas experience or one of its connection and utility states. */
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
  return (
    <CanvasWorkspace
      isCompact={props.isCompact}
      isConnected={props.connectionStatus === "connected"}
      projects={props.projects}
      project={props.project}
      agents={props.agents}
      sessions={props.sessions}
      worktrees={props.worktrees}
      selectedSessionId={props.selectedSessionId}
      sessionFocusRevision={props.sessionFocusRevision}
      onSelectSession={props.onSelectSession}
      onCreateCustomAgent={props.onCreateCustomAgent}
      onCreateSession={props.onCreateSession}
      onStartSession={props.onStartSession}
      onRestartSession={props.onRestartSession}
      onRenameSession={props.onRenameSession}
      onStopSession={props.onStopSession}
      onDeleteSession={props.onDeleteSession}
      onRemoveWorktree={props.onRemoveWorktree}
      onGitStatus={props.onGitStatus}
      onOpenPath={props.onOpenPath}
      subscribeTerminal={props.subscribeTerminal}
      writeTerminal={props.writeTerminal}
      resizeTerminal={props.resizeTerminal}
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
          <h1 id="connection-title">{fatal ? "Jig cannot continue" : "Daemon disconnected"}</h1>
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
            <p className="canvas-settings__copy">Jig stores project references, sessions, and canvas layouts locally. Terminal output is not copied into application metadata.</p>
          </section>

          <section className="canvas-settings__section canvas-settings__section--wide" aria-labelledby="settings-keyboard-title">
            <div className="canvas-settings__section-heading">
              <span aria-hidden="true"><Icon name="terminal" /></span>
              <div><h2 id="settings-keyboard-title">Keyboard</h2><p>Move quickly without leaving the canvas.</p></div>
            </div>
            <dl className="canvas-settings__shortcuts">
              <div><dt>Command palette</dt><dd><kbd>{modifier}</kbd><kbd>K</kbd></dd></div>
              <div><dt>New session</dt><dd><kbd>{modifier}</kbd><kbd>T</kbd></dd></div>
              <div><dt>Focus session</dt><dd><kbd>{modifier}</kbd><kbd>1–9</kbd></dd></div>
            </dl>
            <p className="canvas-settings__note">Application shortcuts pause while a terminal owns keyboard focus.</p>
          </section>
        </div>
      </div>
    </main>
  );
}
