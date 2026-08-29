import { useCallback, useEffect, useRef } from "react";

import type { Session } from "../../../ipc";
import {
  TerminalSurface,
  type TerminalDimensions,
  type TerminalInput,
  type TerminalSurfaceHandle,
} from "../terminal";
import { useSelection, useWorkspaceActions, useWorkspaceRuntime } from "../../workspace";
import { terminalInputToBase64 } from "../../workspace/terminal-codec";

interface TerminalGridProps {
  readonly sessions: readonly Session[];
}

/** Renders up to four xterm surfaces. PTY bytes never enter React state. */
export function TerminalGrid({ sessions }: TerminalGridProps) {
  const selection = useSelection();
  const visible = sessions.filter((session) =>
    selection.visibleSessionIds.includes(session.id),
  );
  const shown = visible.length > 0 ? visible.slice(0, 4) : sessions.slice(0, 1);

  if (shown.length === 0) {
    return (
      <section className="panel" aria-label="Terminals">
        <p>Start a session to open an interactive terminal.</p>
      </section>
    );
  }

  return (
    <section
      className={`terminal-grid terminal-grid--${Math.min(shown.length, 4)}`}
      aria-label="Terminal grid"
    >
      {shown.map((session) => (
        <TerminalPane
          key={session.id}
          session={session}
          focused={session.id === selection.sessionId}
        />
      ))}
    </section>
  );
}

function TerminalPane({
  session,
  focused,
}: {
  readonly session: Session;
  readonly focused: boolean;
}) {
  const actions = useWorkspaceActions();
  const { terminals } = useWorkspaceRuntime();
  const handleRef = useRef<TerminalSurfaceHandle | null>(null);

  const onInput = useCallback(
    (input: TerminalInput) => {
      void actions.writeSession(session.id, terminalInputToBase64(input));
    },
    [actions, session.id],
  );

  const onResize = useCallback(
    (dimensions: TerminalDimensions) => {
      void actions.resizeSession(session.id, dimensions.cols, dimensions.rows);
    },
    [actions, session.id],
  );

  useEffect(() => {
    const cursor = handleRef.current?.getCursor() ?? 0;
    void actions.subscribeSession(session.id, cursor);
  }, [actions, session.id]);

  return (
    <article
      className={
        focused ? "terminal-pane terminal-pane--focused" : "terminal-pane"
      }
    >
      <header>
        <h3>{session.name}</h3>
        <span>
          {session.status}
          {session.worktreePath ? ` · ${session.worktreePath}` : ""}
        </span>
      </header>
      <TerminalSurface
        accessibleLabel={`${session.name} terminal`}
        className="terminal-pane__surface"
        onInput={onInput}
        onResize={onResize}
        ref={(handle) => {
          handleRef.current = handle;
          if (handle) {
            terminals.attach(session.id, handle);
          } else {
            terminals.detach(session.id);
          }
        }}
      />
    </article>
  );
}
