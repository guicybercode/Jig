import type { IpcClient } from "../../../ipc/client";
import type { OrganizationEntry } from "../../../ipc/domain";
import type { Project, Session } from "../../../ipc/types";

/** Host-owned selection and runtime status remain separate from editable organization. */
export interface OrganizationPanelProps {
  readonly client: Pick<IpcClient, "getOrganization" | "saveOrganization">;
  readonly currentProject?: Pick<Project, "id" | "name"> | null;
  readonly currentSession?: Pick<Session, "id" | "name" | "status"> | null;
  /** Change on reconnect when the host retains its IPC client object. */
  readonly connectionKey?: string | number;
  /** Notifies the host of a confirmed save so canvas sorting/filtering can refresh. */
  readonly onChanged?: (entry: OrganizationEntry) => void;
}

/** Retains user choices and their original revision until an explicit refresh or save. */
export interface OrganizationDraft {
  readonly original: OrganizationEntry;
  readonly pinned: boolean;
  readonly archived: boolean;
  readonly workflow: OrganizationEntry["workflow"];
  readonly error?: string;
  readonly notice?: string;
  readonly needsRebase?: boolean;
  readonly rebased?: boolean;
}
