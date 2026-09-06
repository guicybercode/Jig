import { useId, useRef, useState, type FormEvent } from "react";

import { Dialog } from "../../components/Dialog";
import type { KnowledgeLibraryProps, KnowledgeRecord } from "./knowledge-types";
import { hasDraftChanges, trimKnowledgeText, useKnowledgeLibrary } from "./useKnowledgeLibrary";
import "./knowledge-library.css";

/** Local content library. The host owns persistence and insertion into a session draft. */
export function KnowledgeLibrary(props: KnowledgeLibraryProps) {
  const library = useKnowledgeLibrary(props);
  const { draft, busy } = library;
  const id = useId();
  const titleRef = useRef<HTMLInputElement>(null);
  const bodyRef = useRef<HTMLTextAreaElement>(null);
  const cancelDeleteRef = useRef<HTMLButtonElement>(null);
  const [deleteTarget, setDeleteTarget] = useState<KnowledgeRecord | null>(null);
  const [deleteFailed, setDeleteFailed] = useState(false);
  const dirty = hasDraftChanges(draft);
  const projectId = props.currentProject?.id ?? null;

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    saveDraft();
  }

  function saveDraft(asCopy = false) {
    const invalidField = library.validate();
    if (invalidField === "title") titleRef.current?.focus();
    else if (invalidField === "body") bodyRef.current?.focus();
    else void library.save(asCopy);
  }

  async function confirmDelete() {
    if (!deleteTarget) return;
    setDeleteFailed(false);
    if (await library.remove(deleteTarget)) {
      setDeleteTarget(null);
      titleRef.current?.focus();
    } else {
      setDeleteFailed(true);
    }
  }

  return (
    <section className="knowledge-library" aria-labelledby={`${id}-heading`}>
      <header className="knowledge-library__header">
        <div>
          <h2 id={`${id}-heading`}>Prompts &amp; context</h2>
          <p>Reusable content saved on this device.</p>
        </div>
        <button
          type="button"
          className="button button--secondary"
          disabled={busy}
          onClick={() => { library.selectNew(); titleRef.current?.focus(); }}
        >
          New item
        </button>
      </header>

      <div className="knowledge-library__layout">
        <div className="knowledge-library__browser">
          <label className="knowledge-library__field" htmlFor={`${id}-search`}>
            <span>Search prompts and context</span>
            <input
              id={`${id}-search`}
              type="search"
              value={library.query}
              placeholder="Search titles and content"
              onChange={(event) => library.setQuery(event.currentTarget.value)}
            />
          </label>
          <div className="knowledge-library__list-heading">
            <p>{props.currentProject ? `${props.currentProject.name} + global` : "Global library"}</p>
            <button className="button button--quiet" type="button" disabled={library.loading || busy} onClick={library.reload}>
              Refresh
            </button>
          </div>
          {library.listError ? (
            <div className="knowledge-library__error" role="alert">
              <p>{library.listError}</p>
              <button className="button button--secondary" type="button" onClick={library.reload}>Retry loading</button>
            </div>
          ) : null}
          <p className="knowledge-library__list-status" role="status">
            {library.loading ? "Loading saved content…" : `${library.records.length} ${library.records.length === 1 ? "item" : "items"} loaded${library.nextCursor ? "; more available" : ""}.`}
          </p>
          <ul className="knowledge-library__list" aria-label="Saved prompts and context" aria-busy={library.loading}>
            {library.records.map((record) => {
              const itemDraft = library.drafts[record.id];
              return (
                <li key={record.id}>
                  <button
                    className="knowledge-library__item"
                    type="button"
                    aria-pressed={library.selectedKey === record.id}
                    aria-labelledby={`${id}-${record.id}-title`}
                    aria-describedby={`${id}-${record.id}-meta`}
                    disabled={busy}
                    onClick={() => library.selectRecord(record)}
                  >
                    <strong id={`${id}-${record.id}-title`}>{record.title}</strong>
                    <span id={`${id}-${record.id}-meta`} className="knowledge-library__meta">
                      {record.kind === "prompt" ? "Prompt" : "Context"}
                      {" · "}
                      {record.projectId ? "Project" : "Global"}
                      {itemDraft && hasDraftChanges(itemDraft) ? " · Unsaved" : ""}
                    </span>
                    <span className="knowledge-library__preview">{record.body.slice(0, 160)}</span>
                  </button>
                </li>
              );
            })}
          </ul>
          {!library.loading && !library.listError && !library.records.length ? (
            <p className="knowledge-library__empty">
              {trimKnowledgeText(library.query) ? "No saved content matches this search." : "No saved content yet. Create a prompt or context note to reuse across sessions."}
            </p>
          ) : null}
          {library.nextCursor ? (
            <button className="button button--secondary" type="button" disabled={library.loading || library.loadingMore || busy} onClick={() => void library.loadMore()}>
              {library.loadingMore ? "Loading more…" : "Load more"}
            </button>
          ) : null}
        </div>

        <form className="knowledge-library__editor" aria-labelledby={`${id}-editor-heading`} noValidate onSubmit={submit}>
          <div className="knowledge-library__editor-heading">
            <h3 id={`${id}-editor-heading`}>{draft.original ? "Edit saved content" : "New saved content"}</h3>
            {dirty ? <span className="knowledge-library__meta">Unsaved changes</span> : null}
          </div>
          {draft.error ? <p className="knowledge-library__error" role="alert">{draft.error}</p> : null}
          <div className="knowledge-library__options">
            <label className="knowledge-library__field" htmlFor={`${id}-kind`}>
              <span>Type</span>
              <select id={`${id}-kind`} value={draft.kind} disabled={busy} onChange={(event) => {
                const kind = event.currentTarget.value;
                if (kind === "prompt" || kind === "context") library.updateDraft({ kind });
              }}>
                <option value="prompt">Prompt</option>
                <option value="context">Context</option>
              </select>
            </label>
            <label className="knowledge-library__field" htmlFor={`${id}-scope`}>
              <span>Scope</span>
              <select id={`${id}-scope`} value={draft.projectId ?? ""} disabled={busy} onChange={(event) => library.updateDraft({ projectId: event.currentTarget.value || null })}>
                <option value="">Global</option>
                {props.currentProject ? <option value={props.currentProject.id}>Project: {props.currentProject.name}</option> : null}
                {draft.projectId && draft.projectId !== projectId ? <option value={draft.projectId}>Previously selected project</option> : null}
              </select>
            </label>
          </div>
          <label className="knowledge-library__field" htmlFor={`${id}-title`}>
            <span>Title</span>
            <input
              ref={titleRef}
              id={`${id}-title`}
              value={draft.title}
              disabled={busy}
              required
              aria-invalid={Boolean(draft.titleError)}
              aria-describedby={draft.titleError ? `${id}-title-error` : undefined}
              onChange={(event) => library.updateDraft({ title: event.currentTarget.value, titleError: undefined })}
            />
          </label>
          {draft.titleError ? <p id={`${id}-title-error`} className="knowledge-library__error">{draft.titleError}</p> : null}
          <label className="knowledge-library__field knowledge-library__content" htmlFor={`${id}-body`}>
            <span>Content</span>
            <textarea
              ref={bodyRef}
              id={`${id}-body`}
              value={draft.body}
              disabled={busy}
              required
              rows={10}
              aria-invalid={Boolean(draft.bodyError)}
              aria-describedby={draft.bodyError ? `${id}-body-error` : `${id}-body-hint`}
              onChange={(event) => library.updateDraft({ body: event.currentTarget.value, bodyError: undefined })}
            />
          </label>
          {draft.bodyError ? <p id={`${id}-body-error`} className="knowledge-library__error">{draft.bodyError}</p> : null}
          <p id={`${id}-body-hint`} className="knowledge-library__hint">Insert this text into a session draft when you are ready to use it.</p>
          <div className="knowledge-library__actions">
            <button className="button button--primary" type="submit" disabled={busy} aria-busy={busy}>{busy ? "Saving changes…" : "Save locally"}</button>
            {draft.original ? <button className="button button--secondary" type="button" disabled={busy} onClick={() => saveDraft(true)}>Save as copy</button> : null}
            <button className="button button--secondary" type="button" disabled={busy || !trimKnowledgeText(draft.body) || Boolean(props.insertDisabledReason)} aria-describedby={props.insertDisabledReason ? `${id}-insert-hint` : undefined} onClick={library.insert}>Insert into draft</button>
          </div>
          {props.insertDisabledReason ? <p id={`${id}-insert-hint`} className="knowledge-library__hint">{props.insertDisabledReason}</p> : null}
          <p className="knowledge-library__notice" role="status">{draft.notice ?? ""}</p>
          <div className="knowledge-library__secondary-actions">
            <button className="button button--quiet" type="button" disabled={busy || !dirty} onClick={library.discardChanges}>Discard changes</button>
            {draft.original ? <button className="button button--danger-secondary" type="button" disabled={busy} onClick={() => { setDeleteFailed(false); setDeleteTarget(draft.original); }}>Delete item</button> : null}
          </div>
        </form>
      </div>

      <Dialog
        open={Boolean(deleteTarget)}
        title="Delete saved content"
        description={deleteTarget ? `Delete “${deleteTarget.title}” from your local library?` : undefined}
        size="small"
        closeDisabled={busy}
        initialFocusRef={cancelDeleteRef}
        onClose={() => setDeleteTarget(null)}
        footer={<>
          <button ref={cancelDeleteRef} className="button button--secondary" type="button" disabled={busy} onClick={() => setDeleteTarget(null)}>Keep item</button>
          <button className="button button--danger" type="button" disabled={busy} onClick={() => void confirmDelete()}>{busy ? "Deleting…" : "Confirm delete"}</button>
        </>}
      >
        <p>Existing session drafts keep any text already inserted. Unsaved edits to this library item will be discarded.</p>
        {deleteFailed ? <p className="knowledge-library__error" role="alert">{deleteTarget ? library.drafts[deleteTarget.id]?.error : "Could not delete this item. Try again."}</p> : null}
      </Dialog>
    </section>
  );
}
