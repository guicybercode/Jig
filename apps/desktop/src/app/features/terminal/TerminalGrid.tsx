import { useEffect } from "react";

import type { IpcClient } from "../../../ipc/client";
import type { TerminalView } from "../../../ipc/types";

interface TerminalGridProps {
  terminals: TerminalView[];
  ipc?: IpcClient;
}

/** Lays out one to four terminal panes without putting PTY bytes in React state. */
export function TerminalGrid({ terminals, ipc }: TerminalGridProps) {
  if (terminals.length === 0) {
    return null;
  }

  return (
    <section
      className={`terminal-grid terminal-grid--${Math.min(terminals.length, 4)}`}
      aria-label="Session terminals"
    >
      {terminals.map((terminal) => (
        <TerminalPane
          key={terminal.sessionId}
          terminal={terminal}
          ipc={ipc}
        />
      ))}
    </section>
  );
}

interface TerminalPaneProps {
  terminal: TerminalView;
  ipc?: IpcClient;
}

export function TerminalPane({ terminal, ipc }: TerminalPaneProps) {
  useEffect(() => {
    if (!ipc) {
      return undefined;
    }
    void ipc.subscribe(terminal.sessionId);
    return () => {
      void ipc.unsubscribe(terminal.sessionId);
    };
  }, [ipc, terminal.sessionId]);

  return (
    <article
      className="terminal-pane"
      aria-label={`Terminal ${terminal.name}`}
    >
      <header className="terminal-pane__header">{terminal.name}</header>
      <pre className="terminal-pane__body">PTY output is owned by the daemon.</pre>
    </article>
  );
}
