import type {
  KnowledgeDeleteRequest,
  KnowledgeEntry,
  KnowledgeKind,
  KnowledgeListRequest,
  KnowledgeListResponse,
  KnowledgeSaveRequest,
} from "../../../ipc/domain";

/** Feature names alias the authoritative domain mirror instead of duplicating it. */
export type KnowledgeRecord = KnowledgeEntry;
export type KnowledgeListInput = KnowledgeListRequest;
export type KnowledgePage = KnowledgeListResponse;
export type KnowledgeSaveInput = KnowledgeSaveRequest;
export type KnowledgeDeleteInput = KnowledgeDeleteRequest;
export type { KnowledgeKind } from "../../../ipc/domain";

/** Plain text the host can append to a session draft after an explicit click. */
export interface KnowledgeInsertion {
  readonly sourceId: string | null;
  readonly kind: KnowledgeKind;
  readonly title: string;
  readonly body: string;
}

/** Project identity for labeling and scoping the local library. */
export interface KnowledgeProject {
  readonly id: string;
  readonly name: string;
}

/** Host-owned operations; the library never writes to a terminal itself. */
export interface KnowledgeLibraryProps {
  readonly currentProject?: KnowledgeProject | null;
  readonly onList: (input: KnowledgeListInput) => Promise<KnowledgePage>;
  readonly onSave: (input: KnowledgeSaveInput) => Promise<KnowledgeRecord>;
  readonly onDelete: (input: KnowledgeDeleteInput) => Promise<void>;
  readonly onInsert: (content: KnowledgeInsertion) => void;
  /** Explain why insertion is unavailable, for example when no session is selected. */
  readonly insertDisabledReason?: string;
}

/** In-memory editor state; it survives selection and scope changes while mounted. */
export interface KnowledgeDraft {
  readonly original: KnowledgeRecord | null;
  readonly kind: KnowledgeKind;
  readonly projectId: string | null;
  readonly title: string;
  readonly body: string;
  readonly error?: string;
  readonly notice?: string;
  readonly titleError?: string;
  readonly bodyError?: string;
}
