import { useEffect, useRef } from "react";

import type { Session } from "../../../lib/ipc";
import { useWorkspace } from "../../workspace/WorkspaceContext";

interface TerminalGridProps {
  readonly sessions: Session[];
}

/** Renders up to four interactive terminals. xterm state lives outside React. */
export function TerminalGrid({ sessions }: TerminalGridProps) {
  const workspace = useWorkspace();
  const visible = sessions.filter((session) =>
    workspace.visibleSessionIds.includes(session.id),
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
          focused={session.id === workspace.focusedSessionId}
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
  const workspace = useWorkspace();
  const hostRef = useRef<HTMLTextAreaElement>(null);
  const appliedRef = useRef(0);

  useEffect(() => {
    let cancelled = false;
    const timer = window.setInterval(() => {
      void workspace.subscribeReplay(session.id).then((replay) => {
        if (cancelled || !hostRef.current) {
          return;
        }
        if (replay.lastSequence === appliedRef.current) {
          return;
        }
        appliedRef.current = replay.lastSequence;
        hostRef.current.value = base64ToText(replay.replayBase64);
        if (focused) {
          hostRef.current.scrollTop = hostRef.current.scrollHeight;
        }
      });
    }, 80);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [focused, session.id, workspace]);

  return (
    <article
      className={focused ? "terminal-pane terminal-pane--focused" : "terminal-pane"}
    >
      <header>
        <h3>{session.name}</h3>
        <span>
          {session.status}
          {session.worktreePath ? ` · ${session.worktreePath}` : ""}
        </span>
      </header>
      <textarea
        ref={hostRef}
        aria-label={`${session.name} terminal`}
        className="terminal-pane__io"
        spellCheck={false}
        onFocus={() => workspace.focusSession(session.id)}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            void workspace.writeSession(session.id, new TextEncoder().encode("\r"));
          } else if (event.key.length === 1 && !event.ctrlKey && !event.metaKey) {
            event.preventDefault();
            void workspace.writeSession(
              session.id,
              new TextEncoder().encode(event.key),
            );
          }
        }}
      />
    </article>
  );
}

function base64ToText(value: string): string {
  try {
    return atob(value);
  } catch {
    return "";
  }
}
