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

/** Mirrors core::knowledge::discovery; discovery never establishes native activation. */
export type KnowledgeSourceKind = "rule" | "skill";
export type KnowledgeProvider = "codex" | "claude" | "cursor";
export type KnowledgeSourceScope = "global" | "project" | "admin";
export type KnowledgeSourceAvailability = "available" | "too_large" | "symlink" | "non_regular" | "unreadable";

/** Known global sources plus optional registered-project sources. */
export interface KnowledgeDiscoverRequest {
  readonly projectId?: string | null;
}

/** Read selectors are daemon-owned capabilities, never paths supplied by a client. */
export interface KnowledgeReadRequest {
  readonly scanId: string;
  readonly entryId: string;
}

/** Source location and policy metadata observed by one bounded inventory. */
export interface KnowledgeSourceEntry {
  readonly entryId: string;
  readonly kind: KnowledgeSourceKind;
  readonly provider: KnowledgeProvider;
  readonly scope: KnowledgeSourceScope;
  readonly sourcePath: string;
  readonly name: string;
  readonly scopeDirectory: string;
  readonly precedenceHint: string;
  readonly viaSymlink: boolean;
  readonly availability: KnowledgeSourceAvailability;
}

/** A partial discovery result or unsupported case with a safe explanation. */
export interface KnowledgeDiscoveryIssue {
  readonly code: string;
  readonly sourcePath: string | null;
  readonly message: string;
}

/** Metadata-only inventory; its scan capability expires in the daemon. */
export interface KnowledgeDiscoverResponse {
  readonly scanId: string;
  readonly entries: readonly KnowledgeSourceEntry[];
  readonly truncated: boolean;
  readonly issues: readonly KnowledgeDiscoveryIssue[];
}

/** Explicitly requested, revalidated source with raw bounded UTF-8 content. */
export interface KnowledgeReadResponse {
  readonly entry: KnowledgeSourceEntry;
  readonly content: string;
}

/** Organization metadata targets existing catalog identities only. */
export type OrganizationTarget = { readonly kind: "project"; readonly id: string } | { readonly kind: "session"; readonly id: string };
export type OrganizationWorkflow = "backlog" | "in_progress" | "in_review" | "blocked" | "done";

/** Visibility and user-managed workflow, independent of process state. */
export interface OrganizationEntry {
  readonly target: OrganizationTarget;
  readonly pinned: boolean;
  readonly archived: boolean;
  readonly workflow: OrganizationWorkflow | null;
  readonly revision: number;
  readonly updatedAtMs: number | null;
}

/** Between one and 100 distinct targets; results retain request order. */
export interface OrganizationGetRequest { readonly targets: readonly OrganizationTarget[]; }
export interface OrganizationGetResponse { readonly entries: readonly OrganizationEntry[]; }

/** Explicit replacement of all flags; zero is the revision of unwritten defaults. */
export interface OrganizationSaveRequest {
  readonly target: OrganizationTarget;
  readonly expectedRevision: number;
  readonly pinned: boolean;
  readonly archived: boolean;
  readonly workflow: OrganizationWorkflow | null;
}
