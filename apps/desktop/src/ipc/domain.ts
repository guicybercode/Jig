/** Additive daemon contracts. Existing UI DTOs remain owned by types.ts. */
export type * from "./types";
import type { GitTarget, Worktree } from "./types";

/** List managed worktrees; omission includes every registered project. */
export interface WorktreeListRequest {
  readonly projectId?: string;
}

export interface WorktreeListResponse {
  readonly worktrees: readonly Worktree[];
}

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

/** Registered project directory, session cwd, or managed worktree root. */
export type FileTarget = GitTarget | {
  readonly kind: "worktree";
  readonly worktreeId: string;
};
/** Canonical padded base64 of relative Unix path bytes; empty means list root. */
export type FilePath = string;
/** Opaque v1 SHA-256 revision; preserve exactly as returned by the daemon. */
export type FileRevision = string;

export interface FileEntry {
  readonly pathBase64: FilePath;
  readonly displayName: string;
  readonly kind: "file" | "directory" | "symlink" | "other";
  readonly sizeBytes?: number;
  readonly modifiedAtMs?: number;
}

export interface FileListRequest {
  readonly target: FileTarget;
  readonly pathBase64: FilePath;
  readonly limit?: number;
  readonly afterNameBase64?: string;
}

export interface FileListResponse {
  readonly entries: readonly FileEntry[];
  readonly nextAfterNameBase64?: string;
  readonly observedAtMs: number;
}

export interface FileReadRequest {
  readonly target: FileTarget;
  readonly pathBase64: FilePath;
}

/** Text is bounded to 128 KiB UTF-8 including any BOM; line endings are preserved. */
export interface FileReadResponse {
  readonly pathBase64: FilePath;
  readonly text: string;
  readonly revision: FileRevision;
  readonly sizeBytes: number;
  readonly modifiedAtMs?: number;
  readonly observedAtMs: number;
}

/** Existing regular text files only; stale revisions leave the buffer unsaved. */
export interface FileWriteRequest {
  readonly target: FileTarget;
  readonly pathBase64: FilePath;
  readonly text: string;
  readonly expectedRevision: FileRevision;
}

export interface FileWriteResponse {
  readonly pathBase64: FilePath;
  readonly revision: FileRevision;
  readonly sizeBytes: number;
  readonly modifiedAtMs?: number;
  readonly writtenAtMs: number;
}
