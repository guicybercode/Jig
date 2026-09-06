import { useLayoutEffect, useRef, useState } from "react";

import type { AgentRecord, Session } from "../../../ipc/types";
import { Icon } from "../../components/Icon";
import { normalizeBrowserUrl, type CanvasNode } from "./canvas-state";

interface CanvasElementSearchProps {
  readonly nodes: readonly CanvasNode[];
  readonly sessions: ReadonlyMap<string, Session>;
  readonly agents: readonly AgentRecord[];
  readonly onFocusNode: (node: CanvasNode) => void;
  readonly onClose: () => void;
}

/** Searches canvas metadata while the live terminal cards remain mounted. */
export function CanvasElementSearch({
  nodes,
  sessions,
  agents,
  onFocusNode,
  onClose,
}: CanvasElementSearchProps) {
  const [query, setQuery] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const resultsRef = useRef(new Map<string, HTMLButtonElement>());
  const terms = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
  const entries = nodes.map((node) => {
    const session = node.kind === "terminal" && node.sessionId
      ? sessions.get(node.sessionId)
      : undefined;
    const agentId = session?.agentId ?? (node.kind === "terminal" ? node.agentId : undefined);
    const agent = agents.find((candidate) => candidate.id === agentId);
    const details = node.kind === "note"
      ? node.text
      : node.kind === "browser"
      ? normalizeBrowserUrl(node.url)
      : [agent?.displayName, agent?.command.executable ?? node.executable, session?.branch, session?.cwd ?? node.workingDirectory]
          .filter(Boolean).join(" · ");
    return { node, details, searchable: `${node.title} ${node.kind} ${details}`.toLocaleLowerCase() };
  }).filter((entry) => terms.every((term) => entry.searchable.includes(term)));

  useLayoutEffect(() => {
    inputRef.current?.focus();
  }, []);

  return (
    <section
      id="canvas-layers-panel"
      className="canvas-layers-panel"
      aria-labelledby="canvas-layers-title"
      data-browser-obstruction="true"
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          onClose();
        }
      }}
    >
      <header>
        <div>
          <span>Workspace</span>
          <h2 id="canvas-layers-title">Canvas items</h2>
        </div>
        <button className="canvas-layers-panel__close" type="button" aria-label="Close canvas items" onClick={onClose}>
          <Icon name="close" />
        </button>
      </header>
      <label className="canvas-item-search">
        <Icon name="search" />
        <span className="visually-hidden">Search canvas items</span>
        <input
          ref={inputRef}
          type="search"
          value={query}
          placeholder="Title, note, URL, agent, branch or path…"
          onChange={(event) => setQuery(event.currentTarget.value)}
          onKeyDown={(event) => {
            const first = entries[0];
            if (event.key === "ArrowDown" && first) {
              event.preventDefault();
              resultsRef.current.get(first.node.id)?.focus();
            } else if (event.key === "Enter" && first) {
              event.preventDefault();
              onFocusNode(first.node);
            }
          }}
        />
      </label>
      <p className="canvas-item-search__count" role="status">
        {entries.length} of {nodes.length} items
      </p>
      {entries.length === 0 ? (
        <p className="canvas-item-search__empty">No matching items. Try a title, note, URL, agent, branch or path.</p>
      ) : (
        <ul aria-label="Canvas search results">
          {entries.map(({ node, details }, index) => (
            <li key={node.id}>
              <button
                ref={(element) => {
                  if (element) resultsRef.current.set(node.id, element);
                  else resultsRef.current.delete(node.id);
                }}
                type="button"
                className="canvas-item-search__result"
                onClick={() => onFocusNode(node)}
                onKeyDown={(event) => {
                  if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
                  event.preventDefault();
                  const next = entries[index + (event.key === "ArrowDown" ? 1 : -1)];
                  if (next) resultsRef.current.get(next.node.id)?.focus();
                  else if (event.key === "ArrowUp") inputRef.current?.focus();
                }}
              >
                <Icon name={node.kind === "terminal" ? "terminal" : node.kind === "browser" ? "browser" : "note"} />
                <span>
                  <strong>{node.title}</strong>{" "}
                  <small>{details || (node.kind === "terminal" ? "Terminal draft" : node.kind === "browser" ? "Browser without an address" : "Empty note")}</small>
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
