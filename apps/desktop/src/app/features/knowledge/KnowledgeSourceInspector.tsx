import { useId, useState } from "react";

import type { KnowledgeProvider, KnowledgeSourceAvailability, KnowledgeSourceEntry, KnowledgeSourceScope } from "../../../ipc/domain";
import { trimKnowledgeText } from "./useKnowledgeLibrary";
import { useKnowledgeSources, type KnowledgeSourceInspectorProps } from "./useKnowledgeSources";
import "./knowledge-library.css";
import "./knowledge-source-inspector.css";

/** Inspects documented local rule and skill sources without executing or editing their contents. */
export function KnowledgeSourceInspector(props: KnowledgeSourceInspectorProps) {
  const sources = useKnowledgeSources(props);
  const id = useId();
  const [query, setQuery] = useState("");
  const normalizedQuery = trimKnowledgeText(query).toLocaleLowerCase();
  const entries = sources.result?.entries ?? [];
  const visibleEntries = entries.filter((entry) => matchesSource(entry, normalizedQuery));
  const selected = sources.selected;
  const preview = sources.read;

  return (
    <section className="knowledge-library knowledge-inspector" aria-labelledby={`${id}-heading`}>
      <header className="knowledge-library__header">
        <div>
          <h2 id={`${id}-heading`}>Rules &amp; skills</h2>
          <p>Inspect known instruction sources on this device.</p>
        </div>
        <button className="button button--secondary" type="button" disabled={sources.loading} onClick={() => void sources.rescan()}>
          {sources.loading ? "Scanning…" : "Rescan sources"}
        </button>
      </header>
      <p className="knowledge-inspector__provenance">Discovered files are shown with their origin. Native CLI loading remains unverified.</p>
      {sources.scanError ? (
        <div className="knowledge-inspector__notice knowledge-library__error" role="alert">
          <p>{sources.scanError.message}</p>
          {sources.scanError.action ? <p>{sources.scanError.action}</p> : null}
          <button className="button button--secondary" type="button" onClick={() => void sources.rescan()}>Retry discovery</button>
        </div>
      ) : null}
      {sources.result?.truncated ? <p className="knowledge-inspector__notice knowledge-inspector__warning">Inventory incomplete: a discovery limit was reached. Some sources may be missing.</p> : null}
      {sources.result?.issues.length ? (
        <details className="knowledge-inspector__issues" open>
          <summary>{sources.result.issues.length} discovery {sources.result.issues.length === 1 ? "issue" : "issues"}</summary>
          <ul tabIndex={0} aria-label="Discovery issue details">
            {sources.result.issues.map((issue, index) => (
              <li key={`${issue.code}-${index}`}>
                <p>{issue.message}</p>
                {issue.sourcePath ? <code>{issue.sourcePath}</code> : null}
              </li>
            ))}
          </ul>
        </details>
      ) : null}
      <div className="knowledge-library__layout">
        <div className="knowledge-library__browser">
          <label className="knowledge-library__field" htmlFor={`${id}-search`}>
            <span>Search discovered sources</span>
            <input id={`${id}-search`} type="search" value={query} placeholder="Name, provider or path" onChange={(event) => setQuery(event.currentTarget.value)} />
          </label>
          <p className="knowledge-library__hint">{props.currentProject ? `${props.currentProject.name} + global and administrator sources` : "Global and administrator sources"}</p>
          <p className="knowledge-library__list-status" role="status">
            {sources.loading ? "Discovering local rules and skills…" : `${visibleEntries.length} of ${entries.length} discovered sources shown.`}
          </p>
          <ul className="knowledge-library__list" aria-label="Discovered rules and skills" aria-busy={sources.loading}>
            {visibleEntries.map((entry) => (
              <li key={entry.entryId}>
                <button
                  className="knowledge-library__item"
                  type="button"
                  aria-pressed={selected?.entryId === entry.entryId}
                  aria-labelledby={`${id}-${entry.entryId}-name`}
                  aria-describedby={`${id}-${entry.entryId}-summary`}
                  onClick={() => void sources.selectSource(entry)}
                >
                  <strong id={`${id}-${entry.entryId}-name`}>{entry.name}</strong>
                  <span id={`${id}-${entry.entryId}-summary`} className="knowledge-library__meta">
                    {providerLabels[entry.provider]} · {entry.kind === "rule" ? "Rule" : "Skill"} · {scopeLabels[entry.scope]}
                  </span>
                  <code className="knowledge-library__preview">{entry.sourcePath}</code>
                  <span className={entry.availability === "available" ? "knowledge-library__meta" : "knowledge-inspector__unavailable"}>{availabilityLabels[entry.availability]}</span>
                </button>
              </li>
            ))}
          </ul>
          {!sources.loading && !sources.scanError && !visibleEntries.length ? (
            <p className="knowledge-library__empty">{normalizedQuery ? "No discovered sources match this search." : "No sources found at the supported locations."}</p>
          ) : null}
        </div>
        <section className="knowledge-library__editor" aria-labelledby={`${id}-preview-heading`}>
          <h3 id={`${id}-preview-heading`}>{selected ? selected.name : "Source preview"}</h3>
          {selected ? <>
            <dl className="knowledge-inspector__metadata">
              <div><dt>Provider</dt><dd>{providerLabels[selected.provider]}</dd></div>
              <div><dt>Type</dt><dd>{selected.kind === "rule" ? "Rule" : "Skill"}</dd></div>
              <div><dt>Scope</dt><dd>{scopeLabels[selected.scope]}</dd></div>
              {selected.scopeDirectory ? <div><dt>Directory</dt><dd><code>{selected.scopeDirectory}</code>{selected.scopeDirectory === "." ? " (project root)" : ""}</dd></div> : null}
              <div><dt>Source path</dt><dd><code>{selected.sourcePath}</code></dd></div>
              <div><dt>Availability</dt><dd>{availabilityLabels[selected.availability]}</dd></div>
              {selected.viaSymlink ? <div><dt>Origin</dt><dd>Resolved directory link</dd></div> : null}
            </dl>
            <div className="knowledge-inspector__precedence"><h4>Loading and precedence</h4><p>{selected.precedenceHint}</p></div>
            {selected.availability !== "available" ? <p className="knowledge-inspector__warning">{unavailableExplanation(selected.availability)}</p> : null}
            {preview?.loading ? <p role="status">Reading source…</p> : null}
            {preview?.error ? (
              <div className="knowledge-library__error" role="alert">
                <p>{preview.error.message}</p>
                {preview.error.action ? <p>{preview.error.action}</p> : null}
                <p>Rescan sources to check the latest file and obtain a fresh preview.</p>
              </div>
            ) : null}
            {preview?.result ? <>
              <p className="knowledge-library__hint">Read-only source content</p>
              {preview.result.content ? <pre className="knowledge-inspector__content" tabIndex={0} aria-label="Source content"><code>{preview.result.content}</code></pre> : <p>This source is empty.</p>}
            </> : null}
          </> : <p className="knowledge-library__hint">Select a source to inspect its origin and preview its content.</p>}
        </section>
      </div>
    </section>
  );
}

/** Searches the complete bounded inventory's metadata without reading additional files. */
function matchesSource(entry: KnowledgeSourceEntry, query: string): boolean {
  return !query || [entry.name, entry.sourcePath, entry.scopeDirectory, entry.kind, providerLabels[entry.provider], scopeLabels[entry.scope]]
    .some((value) => value.toLocaleLowerCase().includes(query));
}

const providerLabels: Readonly<Record<KnowledgeProvider, string>> = { codex: "Codex", claude: "Claude Code", cursor: "Cursor" };
const scopeLabels: Readonly<Record<KnowledgeSourceScope, string>> = { global: "Global", project: "Project", admin: "Administrator" };
const availabilityLabels: Readonly<Record<KnowledgeSourceAvailability, string>> = {
  available: "Ready to read",
  too_large: "Too large to preview",
  symlink: "File symlink not followed",
  non_regular: "Not a regular file",
  unreadable: "Unable to read safely",
};

/** Explains unavailable candidates while retaining their source provenance. */
function unavailableExplanation(availability: KnowledgeSourceAvailability): string {
  switch (availability) {
    case "too_large": return "This source exceeds the 64 KiB preview limit.";
    case "symlink": return "The source file is a symbolic link. Its content is unavailable for preview.";
    case "non_regular": return "This source is not a regular text file and cannot be previewed.";
    case "unreadable": return "The source could not be opened safely with current permissions.";
    case "available": return "";
  }
}
