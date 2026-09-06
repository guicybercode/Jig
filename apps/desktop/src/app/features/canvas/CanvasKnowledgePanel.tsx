import { useLayoutEffect, useRef, useState } from "react";

import { Icon } from "../../components/Icon";
import { KnowledgePanel } from "../knowledge/KnowledgePanel";
import type { KnowledgePanelProps } from "../knowledge/KnowledgePanel";
import type { KnowledgeProject } from "../knowledge/knowledge-types";
import "./canvas-knowledge-panel.css";

/** Keeps library drafts mounted while the canvas overlay is dismissed. */
interface CanvasKnowledgePanelProps extends Omit<KnowledgePanelProps, "currentProject"> {
  readonly open: boolean;
  readonly currentProject?: KnowledgeProject | null;
  readonly projects: readonly KnowledgeProject[];
  readonly targetTitle?: string;
  readonly onClose: () => void;
}

export function CanvasKnowledgePanel({
  open,
  currentProject,
  projects,
  targetTitle,
  onClose,
  ...props
}: CanvasKnowledgePanelProps) {
  const panelRef = useRef<HTMLElement>(null);
  const returnFocusRef = useRef<HTMLElement | null>(null);
  const composingRef = useRef(false);
  // Keep the last project identity when it disappears instead of relabeling
  // its in-memory drafts as global. Switching to another real project is safe.
  const [scopeProject, setScopeProject] = useState(currentProject ?? null);
  const nextScope = currentProject ?? (
    scopeProject && !projects.some((project) => project.id === scopeProject.id)
      ? scopeProject
      : null
  );
  if (scopeProject !== nextScope) setScopeProject(nextScope);

  useLayoutEffect(() => {
    if (!open) return;
    const previous = document.activeElement;
    returnFocusRef.current = previous instanceof HTMLElement ? previous : null;
    const panel = panelRef.current;
    panel?.querySelector<HTMLInputElement>('input[type="search"]')?.focus();
    return () => {
      if (panel?.contains(document.activeElement) && returnFocusRef.current?.isConnected) {
        returnFocusRef.current.focus();
      }
    };
  }, [open]);

  function requestClose() {
    if (returnFocusRef.current?.isConnected) returnFocusRef.current.focus();
    onClose();
  }

  const missingProject = scopeProject && !projects.some((project) => project.id === scopeProject.id);
  return (
    <section
      ref={panelRef}
      className="canvas-knowledge-panel"
      aria-label="Knowledge library"
      hidden={!open}
      data-browser-obstruction={open ? "true" : undefined}
      data-shortcut-scope="knowledge-library"
      onPointerDown={(event) => event.stopPropagation()}
      onCompositionStartCapture={() => { composingRef.current = true; }}
      onCompositionEndCapture={() => { composingRef.current = false; }}
      onKeyDown={(event) => {
        if (
          event.key === "Escape" && !event.defaultPrevented && !event.repeat
          && !composingRef.current && !event.nativeEvent.isComposing
          && event.nativeEvent.keyCode !== 229
          && !(event.target instanceof Element && event.target.closest("dialog"))
        ) {
          event.preventDefault();
          event.stopPropagation();
          requestClose();
        }
      }}
    >
      <header className="canvas-knowledge-panel__target">
        <p>{targetTitle ? <>Insert a snapshot into <strong>{targetTitle}</strong></> : "Select a terminal to insert a snapshot into its draft."}</p>
        <button type="button" aria-label="Close knowledge library" onClick={requestClose}>
          <Icon name="close" />
        </button>
      </header>
      {missingProject ? (
        <p className="canvas-knowledge-panel__scope" role="status">
          {scopeProject.name} is no longer in the workspace. Its library drafts stay associated with that project.
        </p>
      ) : null}
      <KnowledgePanel {...props} currentProject={scopeProject} />
    </section>
  );
}
