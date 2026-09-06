import { useEffect, useRef, useState } from "react";

import { KNOWLEDGE_BODY_BYTES, KNOWLEDGE_TITLE_BYTES } from "../../../ipc/knowledge-schema";
import { errorData } from "../../utils";
import type {
  KnowledgeDraft,
  KnowledgeLibraryProps,
  KnowledgeRecord,
} from "./knowledge-types";

interface LibraryListing {
  readonly projectId: string | null;
  readonly query: string;
  readonly records: readonly KnowledgeRecord[];
  readonly nextCursor: string | null;
  readonly loading: boolean;
  readonly error?: string;
}

/** Keeps editor drafts independent of asynchronous list responses and navigation. */
export function useKnowledgeLibrary({
  currentProject,
  onList,
  onSave,
  onDelete,
  onInsert,
}: KnowledgeLibraryProps) {
  const projectId = currentProject?.id ?? null;
  const contextKey = projectId ?? "global";
  const newKey = `new:${contextKey}`;
  const [selections, setSelections] = useState<Record<string, string | undefined>>({});
  const [drafts, setDrafts] = useState<Record<string, KnowledgeDraft | undefined>>({});
  const [reload, setReload] = useState(0);
  const [query, setQuery] = useState("");
  const normalizedQuery = trimKnowledgeText(query);
  const [busy, setBusy] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const operationInFlight = useRef(false);
  const moreInFlight = useRef(false);
  const listingVersion = useRef(0);
  const [listing, setListing] = useState<LibraryListing>({ projectId, query: "", records: [], nextCursor: null, loading: true });
  const selectedKey = selections[contextKey] ?? newKey;
  const draft = drafts[selectedKey] ?? emptyDraft(projectId);

  useEffect(() => {
    let cancelled = false;
    const requestVersion = ++listingVersion.current;
    moreInFlight.current = false;
    setLoadingMore(false);
    setListing((previous) => ({
      projectId,
      query: normalizedQuery,
      records: previous.projectId === projectId && previous.query === normalizedQuery ? previous.records : [],
      nextCursor: null,
      loading: true,
    }));
    async function load() {
      try {
        const page = await onList({ projectId, ...(normalizedQuery ? { query: normalizedQuery } : {}) });
        if (!cancelled && requestVersion === listingVersion.current) {
          setListing({ projectId, query: normalizedQuery, records: page.entries, nextCursor: page.nextCursor, loading: false });
        }
      } catch (error) {
        if (!cancelled && requestVersion === listingVersion.current) {
          setListing({ projectId, query: normalizedQuery, records: [], nextCursor: null, loading: false, error: errorData(error).message });
        }
      }
    }
    const timer = window.setTimeout(() => void load(), normalizedQuery ? 200 : 0);
    return () => { cancelled = true; window.clearTimeout(timer); };
  }, [onList, projectId, normalizedQuery, reload]);

  async function loadMore() {
    if (!listing.nextCursor || moreInFlight.current) return;
    const requestVersion = listingVersion.current;
    moreInFlight.current = true;
    setLoadingMore(true);
    setListing((previous) => ({ ...previous, error: undefined }));
    try {
      const page = await onList({
        projectId,
        ...(normalizedQuery ? { query: normalizedQuery } : {}),
        cursor: listing.nextCursor,
      });
      if (requestVersion === listingVersion.current) {
        setListing((previous) => ({
          ...previous,
          records: [...previous.records, ...page.entries.filter((record) => !previous.records.some((existing) => existing.id === record.id))],
          nextCursor: page.nextCursor,
        }));
      }
    } catch (error) {
      if (requestVersion === listingVersion.current) {
        setListing((previous) => ({ ...previous, error: errorData(error).message }));
      }
    } finally {
      if (requestVersion === listingVersion.current) {
        moreInFlight.current = false;
        setLoadingMore(false);
      }
    }
  }

  function updateDraft(patch: Partial<KnowledgeDraft>) {
    setDrafts((previous) => ({
      ...previous,
      [selectedKey]: { ...draft, ...patch, notice: undefined },
    }));
  }

  function selectRecord(record: KnowledgeRecord) {
    setDrafts((previous) => {
      const existing = previous[record.id];
      return {
        ...previous,
        [record.id]: existing && (hasDraftChanges(existing) || existing.error) ? existing : recordDraft(record),
      };
    });
    setSelections((previous) => ({ ...previous, [contextKey]: record.id }));
  }

  function selectNew() {
    setSelections((previous) => ({ ...previous, [contextKey]: newKey }));
  }

  function discardChanges() {
    setDrafts((previous) => ({
      ...previous,
      [selectedKey]: draft.original ? recordDraft(draft.original) : emptyDraft(projectId),
    }));
  }

  function validate(): "title" | "body" | null {
    const titleError = !trimKnowledgeText(draft.title)
      ? "Enter a title."
      : draft.title.includes("\0") ? "Remove null characters from the title."
      : utf8Size(trimKnowledgeText(draft.title)) > KNOWLEDGE_TITLE_BYTES ? "Use a title of at most 256 UTF-8 bytes." : undefined;
    const bodyError = !trimKnowledgeText(draft.body)
      ? "Enter some content."
      : draft.body.includes("\0") ? "Remove null characters from the content."
      : utf8Size(draft.body) > KNOWLEDGE_BODY_BYTES ? "Content must be at most 64 KiB (65,536 UTF-8 bytes)." : undefined;
    updateDraft({ titleError, bodyError });
    return titleError ? "title" : bodyError ? "body" : null;
  }

  async function save(asCopy = false) {
    if (operationInFlight.current || validate()) return;
    operationInFlight.current = true;
    setBusy(true);
    updateDraft({ error: undefined });
    try {
      const saved = await onSave({
        ...(draft.original && !asCopy
          ? { id: draft.original.id, expectedRevision: draft.original.revision }
          : {}),
        kind: draft.kind,
        projectId: draft.projectId,
        title: trimKnowledgeText(draft.title),
        body: draft.body,
      });
      listingVersion.current += 1;
      setDrafts((previous) => {
        const next = { ...previous };
        if (!draft.original) delete next[selectedKey];
        next[saved.id] = { ...recordDraft(saved), notice: asCopy ? "Copy saved locally." : "Saved locally." };
        return next;
      });
      setSelections((previous) => ({ ...previous, [contextKey]: saved.id }));
      setListing((previous) => previous.projectId === projectId ? {
        ...previous,
        records: [saved, ...previous.records.filter((record) => record.id !== saved.id)],
      } : previous);
      setReload((value) => value + 1);
    } catch (error) {
      const detail = errorData(error);
      const conflict = detail.code.includes("conflict");
      setDrafts((previous) => ({
        ...previous,
        [selectedKey]: {
          ...(previous[selectedKey] ?? draft),
          error: conflict
            ? "This item changed in another window. Your edits are preserved. Save a copy to keep both versions."
            : detail.message,
        },
      }));
    } finally {
      operationInFlight.current = false;
      setBusy(false);
    }
  }

  async function remove(original: KnowledgeRecord): Promise<boolean> {
    if (operationInFlight.current) return false;
    operationInFlight.current = true;
    setBusy(true);
    updateDraft({ error: undefined });
    try {
      await onDelete({ id: original.id, expectedRevision: original.revision });
      listingVersion.current += 1;
      setDrafts((previous) => {
        const next = { ...previous };
        delete next[original.id];
        return next;
      });
      setSelections((previous) => Object.fromEntries(
        Object.entries(previous).filter(([, id]) => id !== original.id),
      ));
      setListing((previous) => ({
        ...previous,
        records: previous.records.filter((record) => record.id !== original.id),
      }));
      setReload((value) => value + 1);
      return true;
    } catch (error) {
      setDrafts((previous) => ({
        ...previous,
        [original.id]: { ...(previous[original.id] ?? recordDraft(original)), error: errorData(error).message },
      }));
      return false;
    } finally {
      operationInFlight.current = false;
      setBusy(false);
    }
  }

  function insert() {
    if (!trimKnowledgeText(draft.body)) return;
    try {
      onInsert({
        sourceId: draft.original?.id ?? null,
        sourceRevision: draft.original?.revision ?? null,
        kind: draft.kind,
        title: trimKnowledgeText(draft.title),
        body: draft.body,
      });
      setDrafts((previous) => ({
        ...previous,
        [selectedKey]: { ...draft, notice: "Inserted into the session draft.", error: undefined },
      }));
    } catch (error) {
      updateDraft({ error: errorData(error).message });
    }
  }

  return {
    records: listing.projectId === projectId && listing.query === normalizedQuery ? listing.records : [],
    loading: listing.projectId !== projectId || listing.query !== normalizedQuery || listing.loading,
    listError: listing.projectId === projectId && listing.query === normalizedQuery ? listing.error : undefined,
    reload: () => setReload((value) => value + 1),
    query, setQuery, loadMore, loadingMore, nextCursor: listing.projectId === projectId && listing.query === normalizedQuery ? listing.nextCursor : null,
    selectedKey, draft, drafts, busy, updateDraft, selectRecord, selectNew,
    discardChanges, validate, save, remove, insert,
  };
}

/** Counts encoded bytes, matching the Rust validation limits for non-ASCII text. */
function utf8Size(value: string): number {
  return new TextEncoder().encode(value).byteLength;
}

/** Matches Rust str::trim instead of JavaScript's different Unicode whitespace set. */
export function trimKnowledgeText(value: string): string {
  return value.replace(/^\p{White_Space}+|\p{White_Space}+$/gu, "");
}

/** Creates a separate unsaved draft for each project and the global library. */
function emptyDraft(projectId: string | null): KnowledgeDraft {
  return { original: null, kind: "prompt", projectId, title: "", body: "" };
}

/** Takes a revision snapshot only when explicitly opening an unedited item. */
function recordDraft(record: KnowledgeRecord): KnowledgeDraft {
  return { original: record, kind: record.kind, projectId: record.projectId, title: record.title, body: record.body };
}

/** Computes unsaved state without duplicating it in React state. */
export function hasDraftChanges(draft: KnowledgeDraft): boolean {
  const original = draft.original;
  return original
    ? draft.title !== original.title || draft.body !== original.body || draft.kind !== original.kind || draft.projectId !== original.projectId
    : Boolean(draft.title || draft.body);
}
