import { useCallback } from "react";

import type { IpcClient } from "../../../ipc/client";
import { KnowledgeLibrary } from "./KnowledgeLibrary";
import type {
  KnowledgeDeleteInput,
  KnowledgeLibraryProps,
  KnowledgeListInput,
  KnowledgeSaveInput,
} from "./knowledge-types";

/** Canvas integration boundary using only the project-owned IPC client. */
export interface KnowledgePanelProps extends Omit<KnowledgeLibraryProps, "onList" | "onSave" | "onDelete"> {
  readonly client: Pick<IpcClient, "listKnowledge" | "saveKnowledge" | "deleteKnowledge">;
}

/** Keeps callback identities stable and preserves the client's method receiver. */
export function KnowledgePanel({ client, ...props }: KnowledgePanelProps) {
  const onList = useCallback((input: KnowledgeListInput) => client.listKnowledge(input), [client]);
  const onSave = useCallback((input: KnowledgeSaveInput) => client.saveKnowledge(input), [client]);
  const onDelete = useCallback((input: KnowledgeDeleteInput) => client.deleteKnowledge(input), [client]);
  return <KnowledgeLibrary {...props} onList={onList} onSave={onSave} onDelete={onDelete} />;
}
