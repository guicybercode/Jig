import { IpcContractError, requireArray, requireRecord, requireString } from "./schema";
import type { KnowledgeEntry, KnowledgeListResponse } from "./domain";

/** Keep constants aligned with core::knowledge validation. */
export const KNOWLEDGE_TITLE_BYTES = 256;
export const KNOWLEDGE_BODY_BYTES = 65_536;

function uuid(value: unknown, version7 = false): string {
  const text = requireString(value, "knowledge identifier");
  const pattern = version7
    ? /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i
    : /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
  if (!pattern.test(text)) throw new IpcContractError("Invalid knowledge identifier");
  return text;
}

function integer(value: unknown, minimum: number): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < minimum) {
    throw new IpcContractError("Invalid knowledge revision or timestamp");
  }
  return value;
}

function content(value: unknown, maximum: number): string {
  const text = requireString(value, "knowledge text");
  if (!/[^\p{White_Space}]/u.test(text) || text.includes("\0") || new TextEncoder().encode(text).length > maximum) {
    throw new IpcContractError("Invalid knowledge text length or encoding");
  }
  return text;
}

/** Validate content without including its text in contract failures. */
export function decodeKnowledgeEntry(value: unknown): KnowledgeEntry {
  const row = requireRecord(value, "knowledge entry");
  if (row.kind !== "prompt" && row.kind !== "context") {
    throw new IpcContractError("Invalid knowledge kind");
  }
  const createdAtMs = integer(row.createdAtMs, 0);
  const updatedAtMs = integer(row.updatedAtMs, createdAtMs);
  return {
    id: uuid(row.id, true),
    kind: row.kind,
    projectId: row.projectId === null ? null : uuid(row.projectId),
    title: content(row.title, KNOWLEDGE_TITLE_BYTES),
    body: content(row.body, KNOWLEDGE_BODY_BYTES),
    revision: integer(row.revision, 1),
    createdAtMs,
    updatedAtMs,
  };
}

/** Enforce page shape, stable ordering and a usable continuation cursor. */
export function decodeKnowledgePage(value: unknown): KnowledgeListResponse {
  const page = requireRecord(value, "knowledge page");
  const entries = requireArray(page.entries, "knowledge entries").map(decodeKnowledgeEntry);
  if (entries.length > 50 || entries.some((entry, index) => index > 0 && entry.id <= entries[index - 1].id)) {
    throw new IpcContractError("Invalid knowledge page ordering or size");
  }
  const nextCursor = page.nextCursor === null ? null : uuid(page.nextCursor, true);
  if (nextCursor !== null && nextCursor !== entries[entries.length - 1]?.id) {
    throw new IpcContractError("Invalid knowledge page cursor");
  }
  return { entries, nextCursor };
}
