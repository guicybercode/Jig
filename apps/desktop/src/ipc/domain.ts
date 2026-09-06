/** Additive daemon contracts. Existing UI DTOs remain owned by types.ts. */
export type * from "./types";
import type { Worktree } from "./types";

/** List managed worktrees; omission includes every registered project. */
export interface WorktreeListRequest {
  readonly projectId?: string;
}

export interface WorktreeListResponse {
  readonly worktrees: readonly Worktree[];
}
