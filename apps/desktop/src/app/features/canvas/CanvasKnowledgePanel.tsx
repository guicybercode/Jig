import { useLayoutEffect, useRef, useState } from "react";

import { Icon } from "../../components/Icon";
import { KnowledgePanel } from "../knowledge/KnowledgePanel";
import type { KnowledgePanelProps } from "../knowledge/KnowledgePanel";
import type { KnowledgeProject } from "../knowledge/knowledge-types";
import { KnowledgeSourceInspector } from "../knowledge/KnowledgeSourceInspector";
import type { KnowledgeSourceInspectorProps } from "../knowledge/useKnowledgeSources";
import "./canvas-knowledge-panel.css";

/** Keeps library drafts mounted while the canvas overlay is dismissed. */
interface CanvasKnowledgePanelProps extends Omit<KnowledgePanelProps, "currentProject" | "client"> {
  readonly client: KnowledgePanelProps["client"] & KnowledgeSourceInspectorProps["client"];
  readonly connectionKey?: string;
  readonly open: boolean;
  readonly currentProject?: KnowledgeProject | null;
  readonly projects: readonly KnowledgeProject[];
  readonly targetTitle?: string;
  readonly onClose: () => void;
}

export function CanvasKnowledgePanel({
  open,
  client,
  connectionKey,
  currentProject,
  projects,
  targetTitle,
  onClose,
  ...props
}: CanvasKnowledgePanelProps) {
  const panelRef = useRef<HTMLElement>(null);
  const returnFocusRef = useRef<HTMLElement | null>(null);
  const composingRef = useRef(false);
  const [section, setSection] = useState<"library" | "sources">("library");
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
    Array.from(panel?.querySelectorAll<HTMLInputElement>('input[type="search"]') ?? [])
      .find((input) => !input.closest("[hidden]"))?.focus();
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
        <p>{section === "sources"
          ? "Inspect local instruction sources. Previewing never sends their content."
          : targetTitle ? <>Insert a snapshot into <strong>{targetTitle}</strong></> : "Select a terminal to insert a snapshot into its draft."}</p>
        <button type="button" aria-label="Close knowledge library" onClick={requestClose}>
          <Icon name="close" />
        </button>
      </header>
      <nav className="canvas-knowledge-panel__sections" aria-label="Knowledge sections">
        <button type="button" aria-pressed={section === "library"} onClick={() => setSection("library")}>
          Saved prompts &amp; context
        </button>
        <button type="button" aria-pressed={section === "sources"} onClick={() => setSection("sources")}>
          Rules &amp; skills
        </button>
      </nav>
      {missingProject ? (
        <p className="canvas-knowledge-panel__scope" role="status">
          {scopeProject.name} is no longer in the workspace. Its library drafts stay associated with that project.
        </p>
      ) : null}
      <div hidden={section !== "library"}>
        <KnowledgePanel {...props} client={client} currentProject={scopeProject} />
      </div>
      {open && section === "sources" ? (
        <div>
          <KnowledgeSourceInspector client={client} currentProject={scopeProject} connectionKey={connectionKey} />
        </div>
      ) : null}
    </section>
  );
}
