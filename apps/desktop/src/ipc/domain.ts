/** Additive daemon contracts. Existing UI DTOs remain owned by types.ts. */
export type * from "./types";

/** User-authored local text; no Portal connection or transcript capture. */
export type KnowledgeKind = "prompt" | "context";

/** Mirrors the validated Rust knowledge entry. */
export interface KnowledgeEntry {
  readonly id: string;
  readonly kind: KnowledgeKind;
  readonly projectId: string | null;
  readonly title: string;
  readonly body: string;
  readonly revision: number;
  readonly createdAtMs: number;
  readonly updatedAtMs: number;
}

/** Null/omitted project selects globals; a project includes its global entries. */
export interface KnowledgeListRequest {
  readonly projectId?: string | null;
  readonly kind?: KnowledgeKind;
  readonly cursor?: string;
  readonly query?: string;
}

/** Bodies are included in bounded, cursor-based result pages. */
export interface KnowledgeListResponse {
  readonly entries: readonly KnowledgeEntry[];
  readonly nextCursor: string | null;
}

/** A create omits id/revision; an update requires both. */
export interface KnowledgeSaveRequest {
  readonly id?: string;
  readonly expectedRevision?: number;
  readonly kind: KnowledgeKind;
  readonly projectId: string | null;
  readonly title: string;
  readonly body: string;
}

/** A stale delete leaves the existing text intact. */
export interface KnowledgeDeleteRequest {
  readonly id: string;
  readonly expectedRevision: number;
}
